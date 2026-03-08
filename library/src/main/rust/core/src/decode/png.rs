use crate::borders::find_borders;
use crate::color::{rgb_to_luma, transform_pixels};
use crate::decode::{DecodeError, Decoder};
use crate::icc::extract_png_icc;
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};
use std::io::Cursor;

/// PNG decoder backed by the `png` crate.
pub struct PngDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    source_profile_data: Option<Vec<u8>>,
    target_profile_data: Option<Vec<u8>>,
}

impl PngDecoder {
    pub fn new(
        data: Vec<u8>,
        crop_borders: bool,
        target_profile: Option<&[u8]>,
    ) -> Result<Self, DecodeError> {
        let info = parse_info(&data, crop_borders)?;
        let source_profile_data = extract_png_icc(&data);
        Ok(Self {
            data,
            info,
            source_profile_data,
            target_profile_data: target_profile.map(|p| p.to_vec()),
        })
    }
}

fn parse_info(data: &[u8], crop_borders: bool) -> Result<ImageInfo, DecodeError> {
    let decoder = png::Decoder::new(Cursor::new(data));
    let reader = decoder
        .read_info()
        .map_err(|e| DecodeError::DecodingFailed(format!("PNG header: {e}")))?;
    let png_info = reader.info();

    let image_width = png_info.width;
    let image_height = png_info.height;
    super::check_dimensions(image_width, image_height)?;

    let mut bounds = Rect::full(image_width, image_height);

    if crop_borders {
        // Decode to grayscale for border detection
        if let Ok(gray) = decode_grayscale(data, image_width, image_height) {
            bounds = find_borders(&gray, image_width, image_height);
        }
    }

    Ok(ImageInfo {
        image_width,
        image_height,
        is_animated: false,
        bounds,
    })
}

/// Decode the PNG to single-channel grayscale for border detection.
fn decode_grayscale(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, DecodeError> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| DecodeError::DecodingFailed(format!("PNG gray: {e}")))?;

    let (color_type, _) = reader.output_color_type();
    let samples = color_type.samples();
    let out_w = reader.info().width as usize;

    // Prevent u32 overflow
    let buffer_size = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| DecodeError::DecodingFailed("PNG dimensions overflow".into()))?;

    let mut gray = Vec::with_capacity(buffer_size);
    {
        // Intentionally avoid zero-filling: write every byte first, then set_len.
        let dst = unsafe { std::slice::from_raw_parts_mut(gray.as_mut_ptr(), buffer_size) };

        dst.chunks_exact_mut(out_w)
            .try_for_each(|dst_row| -> Result<(), DecodeError> {
                let row = reader
                    .next_row()
                    .map_err(|e| DecodeError::DecodingFailed(format!("PNG row: {e}")))?;

                if let Some(r) = row {
                    let src_row = r.data();
                    if samples >= 3 {
                        dst_row
                            .iter_mut()
                            .zip(src_row.chunks_exact(samples))
                            .for_each(|(dst_px, src)| {
                                *dst_px = rgb_to_luma(src[0], src[1], src[2]);
                            });
                    } else {
                        dst_row
                            .iter_mut()
                            .zip(src_row.chunks_exact(samples))
                            .for_each(|(dst_px, src)| {
                                *dst_px = src[0];
                            });
                    }
                }
                Ok(())
            })?;
    }
    unsafe {
        gray.set_len(buffer_size);
    }

    Ok(gray)
}

impl Decoder for PngDecoder {
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
        let mut decoder = png::Decoder::new(Cursor::new(&self.data));
        // Ensure 8-bit expanded output regardless of source format.
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|e| DecodeError::DecodingFailed(format!("PNG: {e}")))?;

        let (color_type, _) = reader.output_color_type();
        let samples = color_type.samples();
        let out_w = reader.info().width as usize;

        // Safely calculate buffer size
        let buffer_size = (self.info.image_width as usize)
            .checked_mul(self.info.image_height as usize)
            .and_then(|s| s.checked_mul(4))
            .ok_or_else(|| DecodeError::DecodingFailed("PNG dimensions overflow".into()))?;

        let mut rgba_pixels = Vec::with_capacity(buffer_size);
        {
            // Intentionally avoid zero-filling: write every byte first, then set_len.
            let dst =
                unsafe { std::slice::from_raw_parts_mut(rgba_pixels.as_mut_ptr(), buffer_size) };

            // Iterate over pre-allocated destination rows functionally
            dst.chunks_exact_mut(out_w * 4)
                .try_for_each(|dst_row| -> Result<(), DecodeError> {
                    let row = reader
                        .next_row()
                        .map_err(|e| DecodeError::DecodingFailed(format!("PNG row: {e}")))?;

                    if let Some(r) = row {
                        let src_row = r.data();
                        match samples {
                            1 => {
                                dst_row.chunks_exact_mut(4).zip(src_row.iter()).for_each(
                                    |(dst_px, &luma)| {
                                        dst_px[0] = luma;
                                        dst_px[1] = luma;
                                        dst_px[2] = luma;
                                        dst_px[3] = 255;
                                    },
                                );
                            }
                            2 => {
                                dst_row
                                    .chunks_exact_mut(4)
                                    .zip(src_row.chunks_exact(2))
                                    .for_each(|(dst_px, src)| {
                                        dst_px[0] = src[0];
                                        dst_px[1] = src[0];
                                        dst_px[2] = src[0];
                                        dst_px[3] = src[1];
                                    });
                            }
                            3 => {
                                dst_row
                                    .chunks_exact_mut(4)
                                    .zip(src_row.chunks_exact(3))
                                    .for_each(|(dst_px, src)| {
                                        dst_px[0] = src[0];
                                        dst_px[1] = src[1];
                                        dst_px[2] = src[2];
                                        dst_px[3] = 255;
                                    });
                            }
                            4 => {
                                dst_row.copy_from_slice(&src_row[..out_w * 4]);
                            }
                            _ => {}
                        }
                    }
                    Ok(())
                })?;
        }
        unsafe {
            rgba_pixels.set_len(buffer_size);
        }

        downsample_region(
            &rgba_pixels,
            self.info.image_width,
            4,
            in_rect,
            out_rect,
            sample_size,
            out_pixels,
        )?;

        // Apply ICC colour transform if the source has an embedded profile.
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
}
