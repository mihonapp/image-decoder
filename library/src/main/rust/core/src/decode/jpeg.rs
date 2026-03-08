use crate::borders::find_borders;
use crate::color::transform_pixels;
use crate::decode::{DecodeError, Decoder};
use crate::icc::extract_jpeg_icc;
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};
use std::os::raw::c_int;

use turbojpeg_sys::{
    tj3Decompress8, tj3DecompressHeader, tj3Destroy, tj3GetErrorStr, tj3Init, tj3SetCroppingRegion,
    tj3SetScalingFactor, tjregion, tjscalingfactor, TJINIT_TJINIT_DECOMPRESS, TJPF_TJPF_RGBA,
};

pub struct JpegDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    source_profile_data: Option<Vec<u8>>,
    target_profile_data: Option<Vec<u8>>,
}

impl JpegDecoder {
    pub fn new(
        data: Vec<u8>,
        crop_borders: bool,
        target_profile: Option<&[u8]>,
    ) -> Result<Self, DecodeError> {
        let info = parse_info(&data, crop_borders)?;
        let source_profile_data = extract_jpeg_icc(&data);
        Ok(Self {
            data,
            info,
            source_profile_data,
            target_profile_data: target_profile.map(|p| p.to_vec()),
        })
    }
}

fn parse_info(data: &[u8], crop_borders: bool) -> Result<ImageInfo, DecodeError> {
    let mut decompressor = turbojpeg::Decompressor::new()
        .map_err(|e| DecodeError::DecodingFailed(format!("TurboJPEG init: {e}")))?;

    let header = decompressor
        .read_header(data)
        .map_err(|e| DecodeError::DecodingFailed(format!("JPEG header: {e}")))?;

    let image_width = header.width as u32;
    let image_height = header.height as u32;
    super::check_dimensions(image_width, image_height)?;
    let mut bounds = Rect::full(image_width, image_height);

    if crop_borders {
        if let Ok(image) = turbojpeg::decompress(data, turbojpeg::PixelFormat::GRAY) {
            bounds = find_borders(&image.pixels, image_width, image_height);
        }
    }

    Ok(ImageInfo {
        image_width,
        image_height,
        is_animated: false,
        bounds,
    })
}

impl Decoder for JpegDecoder {
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
        let full_width = self.info.image_width;
        let full_height = self.info.image_height;

        let mut scale_denom = 1;
        while scale_denom * 2 <= sample_size && scale_denom < 8 {
            scale_denom *= 2;
        }

        // Must align to MCU block size (16) for tj3SetCroppingRegion
        let crop_x = (in_rect.x / 16) * 16;
        let crop_y = (in_rect.y / 16) * 16;
        let crop_w = std::cmp::min(in_rect.width + (in_rect.x - crop_x), full_width - crop_x);
        let crop_h = std::cmp::min(in_rect.height + (in_rect.y - crop_y), full_height - crop_y);

        let scaled_crop_w = crop_w.div_ceil(scale_denom);
        let scaled_crop_h = crop_h.div_ceil(scale_denom);

        let buffer_size = (scaled_crop_w as usize)
            .checked_mul(scaled_crop_h as usize)
            .and_then(|s| s.checked_mul(4))
            .ok_or_else(|| DecodeError::DecodingFailed("JPEG dimensions overflow".into()))?;

        let mut idct_pixels = Vec::with_capacity(buffer_size);

        unsafe {
            let handle = tj3Init(TJINIT_TJINIT_DECOMPRESS as c_int);
            if handle.is_null() {
                return Err(DecodeError::DecodingFailed("tj3Init failed".into()));
            }

            if tj3DecompressHeader(
                handle,
                self.data.as_ptr(),
                self.data.len().try_into().unwrap(),
            ) != 0
            {
                let err_ptr = tj3GetErrorStr(handle);
                let err_msg = if !err_ptr.is_null() {
                    std::ffi::CStr::from_ptr(err_ptr)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    "Unknown error".into()
                };
                tj3Destroy(handle);
                return Err(DecodeError::DecodingFailed(format!(
                    "tj3DecompressHeader failed: {}",
                    err_msg
                )));
            }

            let scaling = tjscalingfactor {
                num: 1,
                denom: scale_denom as c_int,
            };
            tj3SetScalingFactor(handle, scaling);

            let region = tjregion {
                x: crop_x as c_int,
                y: crop_y as c_int,
                w: crop_w as c_int,
                h: crop_h as c_int,
            };

            tj3SetCroppingRegion(handle, region);
            idct_pixels.set_len(buffer_size);

            let status = tj3Decompress8(
                handle,
                self.data.as_ptr(),
                self.data.len().try_into().unwrap(),
                idct_pixels.as_mut_ptr(),
                (scaled_crop_w * 4) as c_int,
                TJPF_TJPF_RGBA as c_int,
            );

            if status != 0 {
                let err_ptr = tj3GetErrorStr(handle);
                let err_msg = if !err_ptr.is_null() {
                    std::ffi::CStr::from_ptr(err_ptr)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    "Unknown error".into()
                };
                tj3Destroy(handle);
                return Err(DecodeError::DecodingFailed(format!(
                    "tj3Decompress8 failed: {}",
                    err_msg
                )));
            }

            tj3Destroy(handle);
        }

        let local_in_x = in_rect.x - crop_x;
        let local_in_y = in_rect.y - crop_y;

        let scaled_in_rect = Rect {
            x: local_in_x / scale_denom,
            y: local_in_y / scale_denom,
            width: in_rect.width / scale_denom,
            height: in_rect.height / scale_denom,
        };

        let remaining_sample_size =
            if scaled_in_rect.width == out_rect.width && scaled_in_rect.height == out_rect.height {
                1
            } else {
                2
            };

        downsample_region(
            &idct_pixels,
            scaled_crop_w,
            4,
            scaled_in_rect,
            out_rect,
            remaining_sample_size,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid JPEG (1x1 white pixel) for header parsing.
    fn minimal_jpeg() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
            0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D,
            0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D,
            0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28,
            0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
            0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01,
            0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10,
            0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00,
            0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06,
            0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42,
            0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16,
            0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37,
            0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55,
            0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73,
            0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
            0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
            0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA,
            0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6,
            0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
            0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08,
            0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x7B, 0x94, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0xFF, 0xD9,
        ]
    }

    #[test]
    fn parse_jpeg_header() {
        let data = minimal_jpeg();
        let info = parse_info(&data, false).unwrap();
        assert_eq!(info.image_width, 1);
        assert_eq!(info.image_height, 1);
        assert!(!info.is_animated);
    }
}
