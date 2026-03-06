use crate::borders::find_borders;
use crate::color::{rgb_to_luma, transform_pixels};
use crate::decode::{DecodeError, Decoder};
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};

pub struct HeifDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    source_profile_data: Option<Vec<u8>>,
    target_profile_data: Option<Vec<u8>>,
}

impl HeifDecoder {
    pub fn new(
        data: Vec<u8>,
        crop_borders: bool,
        target_profile: Option<&[u8]>,
    ) -> Result<Self, DecodeError> {
        let info = parse_info(&data, crop_borders)?;
        let source_profile_data = extract_heif_icc(&data);
        Ok(Self {
            data,
            info,
            source_profile_data,
            target_profile_data: target_profile.map(|p| p.to_vec()),
        })
    }
}

fn extract_heif_icc(data: &[u8]) -> Option<Vec<u8>> {
    let ctx = libheif_rs::HeifContext::read_from_bytes(data).ok()?;
    let handle = ctx.primary_image_handle().ok()?;
    let profile = handle.color_profile_raw()?;
    if profile.data.is_empty() {
        None
    } else {
        Some(profile.data)
    }
}

fn parse_info(data: &[u8], crop_borders: bool) -> Result<ImageInfo, DecodeError> {
    let ctx = libheif_rs::HeifContext::read_from_bytes(data)
        .map_err(|e| DecodeError::DecodingFailed(format!("HEIF context: {e}")))?;

    let handle = ctx
        .primary_image_handle()
        .map_err(|e| DecodeError::DecodingFailed(format!("HEIF handle: {e}")))?;

    let image_width = handle.width();
    let image_height = handle.height();
    super::check_dimensions(image_width, image_height)?;

    let mut bounds = Rect::full(image_width, image_height);

    if crop_borders {
        let lib_heif = libheif_rs::LibHeif::new();
        if let Ok(image) = lib_heif.decode(
            &handle,
            libheif_rs::ColorSpace::Rgb(libheif_rs::RgbChroma::Rgb),
            None,
        ) {
            if let Some(plane) = image.planes().interleaved {
                let stride = plane.stride;
                let width_usize = image_width as usize;
                let height_usize = image_height as usize;
                let pixel_count = width_usize * height_usize;

                let mut luma = Vec::with_capacity(pixel_count);
                {
                    // Intentionally avoid zero-filling: write every byte first, then set_len.
                    let dst =
                        unsafe { std::slice::from_raw_parts_mut(luma.as_mut_ptr(), pixel_count) };

                    dst.chunks_mut(width_usize)
                        .zip(plane.data.chunks(stride).take(height_usize))
                        .for_each(|(dst_row, src_row)| {
                            dst_row
                                .iter_mut()
                                .zip(src_row[..width_usize * 3].chunks_exact(3))
                                .for_each(|(dst_px, rgb)| {
                                    *dst_px = rgb_to_luma(rgb[0], rgb[1], rgb[2]);
                                });
                        });
                }
                unsafe {
                    luma.set_len(pixel_count);
                }

                bounds = find_borders(&luma, image_width, image_height);
            }
        }
    }

    Ok(ImageInfo {
        image_width,
        image_height,
        is_animated: false,
        bounds,
    })
}

impl Decoder for HeifDecoder {
    fn info(&self) -> &ImageInfo {
        &self.info
    }

    fn decode(
        &self,
        out_pixels: &mut [u8],
        out_rect: Rect,
        in_rect: Rect,
        sample_size: u32,
    ) -> Result<(), DecodeError> {
        let ctx = libheif_rs::HeifContext::read_from_bytes(&self.data)
            .map_err(|e| DecodeError::DecodingFailed(format!("HEIF: {e}")))?;

        let handle = ctx
            .primary_image_handle()
            .map_err(|e| DecodeError::DecodingFailed(format!("HEIF handle: {e}")))?;

        // Only decode alpha if the image actually has it.
        // Saves 33% memory bandwidth on standard opaque photos.
        let has_alpha = handle.has_alpha_channel();
        let chroma = if has_alpha {
            libheif_rs::RgbChroma::Rgba
        } else {
            libheif_rs::RgbChroma::Rgb
        };
        let components = if has_alpha { 4 } else { 3 };

        let lib_heif = libheif_rs::LibHeif::new();
        let image = lib_heif
            .decode(&handle, libheif_rs::ColorSpace::Rgb(chroma), None)
            .map_err(|e| DecodeError::DecodingFailed(format!("HEIF decode: {e}")))?;

        let plane = image
            .planes()
            .interleaved
            .ok_or_else(|| DecodeError::DecodingFailed("HEIF: no interleaved plane".into()))?;

        let width = image.width();
        let height = image.height();
        let stride = plane.stride as u32;

        // By padding the width to match the stride, we trick the resizer into perfectly
        // traversing the SIMD padding natively, entirely avoiding the massive RGBA buffer copy.
        if stride.is_multiple_of(components) {
            let padded_width = stride / components;
            downsample_region(
                plane.data,
                padded_width,
                components,
                in_rect,
                out_rect,
                sample_size,
                out_pixels,
            )?;
        } else {
            let w_usize = width as usize;
            let h_usize = height as usize;
            let stride_usize = stride as usize;
            let comp_usize = components as usize;

            let buffer_size = w_usize
                .checked_mul(h_usize)
                .and_then(|s| s.checked_mul(comp_usize))
                .ok_or_else(|| DecodeError::DecodingFailed("HEIF dimensions overflow".into()))?;

            let mut pixel_buf = Vec::with_capacity(buffer_size);
            {
                // Intentionally avoid zero-filling: write every byte first, then set_len.
                let dst =
                    unsafe { std::slice::from_raw_parts_mut(pixel_buf.as_mut_ptr(), buffer_size) };
                dst.chunks_exact_mut(w_usize * comp_usize)
                    .zip(plane.data.chunks(stride_usize).take(h_usize))
                    .for_each(|(dst_row, src_row)| {
                        dst_row.copy_from_slice(&src_row[..w_usize * comp_usize]);
                    });
            }
            unsafe {
                pixel_buf.set_len(buffer_size);
            }

            downsample_region(
                &pixel_buf,
                width,
                components,
                in_rect,
                out_rect,
                sample_size,
                out_pixels,
            )?;
        }

        // If we downsampled a 3-channel RGB image, the output is sitting in the
        // front of `out_pixels`. We must expand it in-place to 4-channel RGBA.
        if !has_alpha {
            let pixel_count = (out_rect.width * out_rect.height) as usize;
            for i in (0..pixel_count).rev() {
                let src_base = i * 3;
                let dst_base = i * 4;
                let r = out_pixels[src_base];
                let g = out_pixels[src_base + 1];
                let b = out_pixels[src_base + 2];
                out_pixels[dst_base] = r;
                out_pixels[dst_base + 1] = g;
                out_pixels[dst_base + 2] = b;
                out_pixels[dst_base + 3] = 255;
            }
        }

        if let Some(ref src_icc) = self.source_profile_data {
            let pixel_count = (out_rect.width * out_rect.height) as usize;
            transform_pixels(
                out_pixels,
                pixel_count,
                Some(src_icc),
                self.target_profile_data.as_deref(),
            )?;
        }

        Ok(())
    }

    fn use_transform(&self) -> bool {
        self.source_profile_data.is_some()
    }

    fn lcms_in_type(&self) -> u32 {
        0
    }
}
