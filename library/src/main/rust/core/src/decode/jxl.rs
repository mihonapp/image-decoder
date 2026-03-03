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

/// Read only the JXL basic info (width, height) from the header without
/// decoding any pixel data. Uses the low-level jpegxl-sys FFI so that
/// only `BasicInfo` events are subscribed (no `FullImage`).
fn read_basic_info(data: &[u8]) -> Result<(u32, u32), DecodeError> {
    use jpegxl_sys::decode::{
        JxlDecoderCreate, JxlDecoderDestroy, JxlDecoderGetBasicInfo, JxlDecoderProcessInput,
        JxlDecoderSetInput, JxlDecoderStatus, JxlDecoderSubscribeEvents,
    };
    use std::mem::MaybeUninit;
    use std::ptr;

    unsafe {
        let dec = JxlDecoderCreate(ptr::null());
        if dec.is_null() {
            return Err(DecodeError::DecodingFailed(
                "JXL: failed to create decoder".into(),
            ));
        }

        // Subscribe only to BasicInfo — no pixel decoding will happen.
        let status = JxlDecoderSubscribeEvents(dec, JxlDecoderStatus::BasicInfo as i32);
        if status != JxlDecoderStatus::Success {
            JxlDecoderDestroy(dec);
            return Err(DecodeError::DecodingFailed(
                "JXL: failed to subscribe events".into(),
            ));
        }

        let status = JxlDecoderSetInput(dec, data.as_ptr(), data.len());
        if status != JxlDecoderStatus::Success {
            JxlDecoderDestroy(dec);
            return Err(DecodeError::DecodingFailed(
                "JXL: failed to set input".into(),
            ));
        }

        let status = JxlDecoderProcessInput(dec);
        if status != JxlDecoderStatus::BasicInfo {
            JxlDecoderDestroy(dec);
            return Err(DecodeError::DecodingFailed(format!(
                "JXL: expected BasicInfo, got {status:?}"
            )));
        }

        let mut info = MaybeUninit::uninit();
        let status = JxlDecoderGetBasicInfo(dec, info.as_mut_ptr());
        if status != JxlDecoderStatus::Success {
            JxlDecoderDestroy(dec);
            return Err(DecodeError::DecodingFailed(
                "JXL: failed to get basic info".into(),
            ));
        }

        let info = info.assume_init();
        let w = info.xsize;
        let h = info.ysize;
        JxlDecoderDestroy(dec);
        Ok((w, h))
    }
}

fn parse_info(data: &[u8], crop_borders: bool) -> Result<ImageInfo, DecodeError> {
    let (image_width, image_height) = read_basic_info(data)?;

    let mut bounds = Rect::full(image_width, image_height);

    if crop_borders {
        if let Ok(mut rgba) = decode_rgba(data) {
            // Convert RGBA→grayscale in-place: overwrite the first W*H bytes
            // of the buffer so we don't allocate a second Vec.
            let pixel_count = (image_width * image_height) as usize;
            for i in 0..pixel_count {
                let base = i * 4;
                rgba[i] = rgb_to_luma(rgba[base], rgba[base + 1], rgba[base + 2]);
            }
            bounds = find_borders(&rgba[..pixel_count], image_width, image_height);
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
