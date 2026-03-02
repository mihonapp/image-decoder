use crate::borders::find_borders;
use crate::decode::{DecodeError, Decoder};
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};
use std::io::Cursor;

/// PNG decoder backed by the `png` crate.
pub struct PngDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    #[allow(dead_code)]
    target_profile_data: Option<Vec<u8>>,
}

impl PngDecoder {
    pub fn new(
        data: Vec<u8>,
        crop_borders: bool,
        target_profile: Option<&[u8]>,
    ) -> Result<Self, DecodeError> {
        let info = parse_info(&data, crop_borders)?;
        Ok(Self {
            data,
            info,
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

    let buf_size = reader.output_buffer_size()
        .ok_or_else(|| DecodeError::DecodingFailed("PNG: cannot determine buffer size".into()))?;
    let mut buf = vec![0u8; buf_size];
    let output_info = reader
        .next_frame(&mut buf)
        .map_err(|e| DecodeError::DecodingFailed(format!("PNG frame: {e}")))?;

    let samples = output_info.color_type.samples();
    let out_w = output_info.width as usize;
    let out_h = output_info.height as usize;
    let line_size = output_info.line_size;
    let mut gray = vec![0u8; (width * height) as usize];

    for y in 0..out_h {
        for x in 0..out_w {
            let src_idx = y * line_size + x * samples;
            let dst_idx = y * out_w + x;
            gray[dst_idx] = if samples >= 3 {
                let r = buf[src_idx] as u16;
                let g = buf[src_idx + 1] as u16;
                let b = buf[src_idx + 2] as u16;
                ((r * 299 + g * 587 + b * 114) / 1000) as u8
            } else {
                buf[src_idx]
            };
        }
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

        let buf_size = reader.output_buffer_size()
            .ok_or_else(|| DecodeError::DecodingFailed("PNG: cannot determine buffer size".into()))?;
        let mut buf = vec![0u8; buf_size];
        let output_info = reader
            .next_frame(&mut buf)
            .map_err(|e| DecodeError::DecodingFailed(format!("PNG frame: {e}")))?;

        let samples = output_info.color_type.samples();
        let out_w = output_info.width as usize;
        let out_h = output_info.height as usize;
        let line_size = output_info.line_size;

        // For the output we always want RGBA (4 components).
        let mut rgba_pixels =
            vec![0u8; (self.info.image_width * self.info.image_height * 4) as usize];

        for y in 0..out_h {
            for x in 0..out_w {
                let src_idx = y * line_size + x * samples;
                let dst_idx = (y * out_w + x) * 4;
                match samples {
                1 => {
                    rgba_pixels[dst_idx] = buf[src_idx];
                    rgba_pixels[dst_idx + 1] = buf[src_idx];
                    rgba_pixels[dst_idx + 2] = buf[src_idx];
                    rgba_pixels[dst_idx + 3] = 255;
                }
                2 => {
                    rgba_pixels[dst_idx] = buf[src_idx];
                    rgba_pixels[dst_idx + 1] = buf[src_idx];
                    rgba_pixels[dst_idx + 2] = buf[src_idx];
                    rgba_pixels[dst_idx + 3] = buf[src_idx + 1];
                }
                3 => {
                    rgba_pixels[dst_idx] = buf[src_idx];
                    rgba_pixels[dst_idx + 1] = buf[src_idx + 1];
                    rgba_pixels[dst_idx + 2] = buf[src_idx + 2];
                    rgba_pixels[dst_idx + 3] = 255;
                }
                4 => {
                    rgba_pixels[dst_idx] = buf[src_idx];
                    rgba_pixels[dst_idx + 1] = buf[src_idx + 1];
                    rgba_pixels[dst_idx + 2] = buf[src_idx + 2];
                    rgba_pixels[dst_idx + 3] = buf[src_idx + 3];
                }
                _ => {}
            }
            }
        }

        downsample_region(
            &rgba_pixels,
            self.info.image_width,
            4,
            in_rect,
            out_rect,
            sample_size,
            out_pixels,
        )
    }

    fn use_transform(&self) -> bool {
        false
    }

    fn lcms_in_type(&self) -> u32 {
        0
    }
}
