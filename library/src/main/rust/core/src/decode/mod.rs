pub mod heif;
pub mod jpeg;
pub mod jxl;
pub mod png;
pub mod webp;

use crate::types::{ImageInfo, Rect};
use thiserror::Error;

/// Maximum number of decoded pixels allowed before we reject the image as
/// oversized. This prevents OOM on maliciously crafted headers.
/// 256 megapixels ≈ 1 GiB of RGBA data.
pub const MAX_PIXEL_COUNT: u64 = 256 * 1024 * 1024;

/// Check that `width × height` does not exceed [`MAX_PIXEL_COUNT`].
pub fn check_dimensions(width: u32, height: u32) -> Result<(), DecodeError> {
    let pixels = width as u64 * height as u64;
    if pixels > MAX_PIXEL_COUNT || width == 0 || height == 0 {
        return Err(DecodeError::DecodingFailed(format!(
            "Image dimensions {width}×{height} ({pixels} px) exceed safety limit"
        )));
    }
    Ok(())
}

/// Errors that can occur during image decoding.
#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("Unsupported image format")]
    UnsupportedFormat,
    #[error("Decoding failed: {0}")]
    DecodingFailed(String),
    #[error("Invalid region: {0}")]
    InvalidRegion(String),
    #[error("Color management error: {0}")]
    ColorManagement(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Trait implemented by each format-specific decoder.
///
/// Mirrors the C++ `BaseDecoder` pure virtual interface. Each implementation
/// holds the raw image bytes and pre-parsed [`ImageInfo`] (including cropped
/// bounds when `crop_borders` was requested).
pub trait Decoder: Send {
    /// Return image metadata parsed from the header.
    fn info(&self) -> &ImageInfo;

    /// Decode the region described by `in_rect` (in original image
    /// coordinates), downsampled by `sample_size`, writing exactly
    /// `out_rect.width * out_rect.height` RGBA pixels into `out_pixels`.
    ///
    /// `out_pixels` length must be `out_rect.width * out_rect.height * 4`.
    fn decode(
        &self,
        out_pixels: &mut [u8],
        out_rect: Rect,
        in_rect: Rect,
        sample_size: u32,
    ) -> Result<(), DecodeError>;

    /// Whether a color transform should be applied after decoding.
    fn use_transform(&self) -> bool;

    /// The lcms2 input pixel type constant for the decoded buffer.
    fn lcms_in_type(&self) -> u32;
}

/// Detect the image type from the first bytes of a file.
///
/// Requires at least 32 bytes; returns `None` when the format cannot be
/// determined.
pub fn find_type(header: &[u8]) -> Option<crate::types::ImageType> {
    crate::format::detect(header)
}

/// Create a new decoder for the given image data.
///
/// `data` is the complete file content. When `crop_borders` is true the
/// decoder will pre-scan the image and narrow `ImageInfo::bounds`.
/// `target_profile` is the raw ICC profile bytes for the display (defaults
/// to sRGB internally when `None`).
pub fn new_decoder(
    data: Vec<u8>,
    crop_borders: bool,
    target_profile: Option<&[u8]>,
) -> Result<Box<dyn Decoder>, DecodeError> {
    let fmt = crate::format::detect_format(&data).ok_or(DecodeError::UnsupportedFormat)?;

    match fmt {
        crate::types::Format::Jpeg => {
            let d = jpeg::JpegDecoder::new(data, crop_borders, target_profile)?;
            Ok(Box::new(d))
        }
        crate::types::Format::Png => {
            let d = png::PngDecoder::new(data, crop_borders, target_profile)?;
            Ok(Box::new(d))
        }
        crate::types::Format::Webp => {
            let d = webp::WebpDecoder::new(data, crop_borders, target_profile)?;
            Ok(Box::new(d))
        }
        crate::types::Format::Jxl => {
            let d = jxl::JxlDecoder::new(data, crop_borders, target_profile)?;
            Ok(Box::new(d))
        }
        crate::types::Format::Heif | crate::types::Format::Avif => {
            let d = heif::HeifDecoder::new(data, crop_borders, target_profile)?;
            Ok(Box::new(d))
        }
        crate::types::Format::Gif => Err(DecodeError::UnsupportedFormat),
    }
}
