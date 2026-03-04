use crate::borders::find_borders;
use crate::color::rgb_to_luma;
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

    let (color_type, _) = reader.output_color_type();
    let samples = color_type.samples();
    let out_h = reader.info().height as usize;
    let buffer_size = (width * height) as usize;

    let gray: Vec<u8> = std::iter::from_fn(|| match reader.next_row() {
        Ok(Some(row)) => Some(Ok(row.data().to_vec())),
        Ok(None) => None,
        Err(e) => Some(Err(DecodeError::DecodingFailed(format!("PNG row: {e}")))),
    })
    .take(out_h)
    .try_fold(
        Vec::with_capacity(buffer_size),
        |mut acc, row_result| -> Result<_, DecodeError> {
            let src = row_result?;
            if samples >= 3 {
                acc.extend(
                    src.chunks_exact(samples)
                        .map(|p| rgb_to_luma(p[0], p[1], p[2])),
                );
            } else {
                acc.extend(src.chunks_exact(samples).map(|p| p[0]));
            }
            Ok(acc)
        },
    )?;

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
        let out_h = reader.info().height as usize;
        let buffer_size = (self.info.image_width * self.info.image_height * 4) as usize;

        let rgba_pixels: Vec<u8> = std::iter::from_fn(|| match reader.next_row() {
            Ok(Some(row)) => Some(Ok(row.data().to_vec())),
            Ok(None) => None,
            Err(e) => Some(Err(DecodeError::DecodingFailed(format!("PNG row: {e}")))),
        })
        .take(out_h)
        .try_fold(
            Vec::with_capacity(buffer_size),
            |mut acc, row_result| -> Result<_, DecodeError> {
                let src_row = row_result?;
                match samples {
                    1 => acc.extend(src_row.iter().flat_map(|&luma| [luma, luma, luma, 255])),
                    2 => acc.extend(
                        src_row
                            .chunks_exact(2)
                            .flat_map(|s| [s[0], s[0], s[0], s[1]]),
                    ),
                    3 => acc.extend(
                        src_row
                            .chunks_exact(3)
                            .flat_map(|s| [s[0], s[1], s[2], 255]),
                    ),
                    4 => acc.extend_from_slice(&src_row[..out_w * 4]),
                    _ => {}
                }
                Ok(acc)
            },
        )?;

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
