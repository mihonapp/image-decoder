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
                let stride = plane.stride as usize;
                let width_usize = image_width as usize;
                let height_usize = image_height as usize;
                let pixel_count = width_usize * height_usize;

                let mut luma = Vec::with_capacity(pixel_count);
                unsafe {
                    luma.set_len(pixel_count);
                }

                luma.chunks_mut(width_usize)
                    .zip(plane.data.chunks(stride).take(height_usize))
                    .for_each(|(dst_row, src_row)| {
                        dst_row
                            .iter_mut()
                            .zip(src_row[..width_usize * 3].chunks_exact(3))
                            .for_each(|(dst, rgb)| {
                                *dst = rgb_to_luma(rgb[0], rgb[1], rgb[2]);
                            });
                    });

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

        let lib_heif = libheif_rs::LibHeif::new();
        let image = lib_heif
            .decode(
                &handle,
                libheif_rs::ColorSpace::Rgb(libheif_rs::RgbChroma::Rgba),
                None,
            )
            .map_err(|e| DecodeError::DecodingFailed(format!("HEIF decode: {e}")))?;

        let plane = image
            .planes()
            .interleaved
            .ok_or_else(|| DecodeError::DecodingFailed("HEIF: no interleaved plane".into()))?;

        let width = image.width() as u32;
        let height = image.height() as u32;
        let stride = plane.stride as u32;

        // By padding the width to match the stride, we trick the resizer into perfectly
        // traversing the SIMD padding natively, entirely avoiding the massive RGBA buffer copy.
        if stride % 4 == 0 {
            let padded_width = stride / 4;
            downsample_region(
                plane.data,
                padded_width,
                4,
                in_rect,
                out_rect,
                sample_size,
                out_pixels,
            )?;
        } else {
            // Ultimate safety fallback for non-RGBA aligned strides (exceedingly rare)
            let w_usize = width as usize;
            let h_usize = height as usize;
            let stride_usize = stride as usize;

            let buffer_size = w_usize
                .checked_mul(h_usize)
                .and_then(|s| s.checked_mul(4))
                .ok_or_else(|| DecodeError::DecodingFailed("HEIF dimensions overflow".into()))?;

            let mut rgba = Vec::with_capacity(buffer_size);
            unsafe {
                rgba.set_len(buffer_size);
            }

            rgba.chunks_exact_mut(w_usize * 4)
                .zip(plane.data.chunks(stride_usize).take(h_usize))
                .for_each(|(dst_row, src_row)| {
                    dst_row.copy_from_slice(&src_row[..w_usize * 4]);
                });

            downsample_region(&rgba, width, 4, in_rect, out_rect, sample_size, out_pixels)?;
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
