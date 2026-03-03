use crate::borders::find_borders;
use crate::color::rgb_to_luma;
use crate::decode::{DecodeError, Decoder};
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};

use jpegxl_rs::decode::{decoder_builder, PixelFormat};
use jpegxl_rs::parallel::resizable_runner::ResizableRunner;

/// JPEG XL decoder backed by libjxl via `jpegxl-rs`.
pub struct JxlDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    #[allow(dead_code)]
    target_profile_data: Option<Vec<u8>>,
}

impl JxlDecoder {
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
    let (metadata, _) = decode_internal(data)?;

    let image_width = metadata.width;
    let image_height = metadata.height;

    let mut bounds = Rect::full(image_width, image_height);

    if crop_borders {
        if let Ok(rgba) = decode_rgba(data) {
            let gray: Vec<u8> = rgba
                .chunks_exact(4)
                .map(|px| rgb_to_luma(px[0], px[1], px[2]))
                .collect();
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

/// Decode the JXL data using libjxl with multi-threaded parallel runner.
/// Returns (metadata, raw pixel bytes).
fn decode_internal(data: &[u8]) -> Result<(jpegxl_rs::decode::Metadata, Vec<u8>), DecodeError> {
    let runner = ResizableRunner::default();
    let decoder = decoder_builder()
        .parallel_runner(&runner)
        .pixel_format(PixelFormat {
            num_channels: 4,
            ..Default::default()
        })
        .build()
        .map_err(|e| DecodeError::DecodingFailed(format!("JXL decoder init: {e}")))?;

    let (metadata, pixels) = decoder
        .decode_with::<u8>(data)
        .map_err(|e| DecodeError::DecodingFailed(format!("JXL decode: {e}")))?;

    Ok((metadata, pixels))
}

/// Decode the JXL image to an RGBA u8 buffer.
fn decode_rgba(data: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let (_metadata, rgba) = decode_internal(data)?;
    Ok(rgba)
}

impl Decoder for JxlDecoder {
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
        let rgba = decode_rgba(&self.data)?;
        let full_width = self.info.image_width;

        downsample_region(
            &rgba,
            full_width,
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
