use crate::borders::find_borders;
use crate::color::rgb_to_luma;
use crate::decode::{DecodeError, Decoder};
use crate::types::{ImageInfo, Rect};

pub struct WebpDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    #[allow(dead_code)]
    target_profile_data: Option<Vec<u8>>,
}

impl WebpDecoder {
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
    let mut image_width = 0;
    let mut image_height = 0;

    let status = unsafe {
        libwebp_sys::WebPGetInfo(
            data.as_ptr(),
            data.len(),
            &mut image_width,
            &mut image_height,
        )
    };

    if status == 0 {
        return Err(DecodeError::DecodingFailed(
            "WebP: invalid bitstream".into(),
        ));
    }

    let image_width = image_width as u32;
    let image_height = image_height as u32;
    let mut bounds = Rect::full(image_width, image_height);

    if crop_borders {
        let decoder = webp::Decoder::new(data);
        if let Some(image) = decoder.decode() {
            let is_alpha = image.is_alpha();
            let pixel_count = (image_width * image_height) as usize;

            let mut gray_buf = Vec::with_capacity(pixel_count);
            unsafe {
                gray_buf.set_len(pixel_count);
            }

            let src_raw = &*image;
            if is_alpha {
                for i in 0..pixel_count {
                    let base = i * 4;
                    gray_buf[i] = rgb_to_luma(src_raw[base], src_raw[base + 1], src_raw[base + 2]);
                }
            } else {
                for i in 0..pixel_count {
                    let base = i * 3;
                    gray_buf[i] = rgb_to_luma(src_raw[base], src_raw[base + 1], src_raw[base + 2]);
                }
            }
            bounds = find_borders(&gray_buf, image_width, image_height);
        } else {
            return Err(DecodeError::DecodingFailed(
                "WebP border decode failed".into(),
            ));
        }
    }

    Ok(ImageInfo {
        image_width,
        image_height,
        is_animated: false,
        bounds,
    })
}

impl Decoder for WebpDecoder {
    fn info(&self) -> &ImageInfo {
        &self.info
    }

    fn decode(
        &self,
        out_pixels: &mut [u8],
        out_rect: Rect,
        in_rect: Rect,
        _sample_size: u32,
    ) -> Result<(), DecodeError> {
        // Prevent i32 overflow for FFI parameters
        if out_rect.width > i32::MAX as u32
            || out_rect.height > i32::MAX as u32
            || in_rect.width > i32::MAX as u32
            || in_rect.height > i32::MAX as u32
            || in_rect.x > i32::MAX as u32
            || in_rect.y > i32::MAX as u32
        {
            return Err(DecodeError::InvalidRegion(
                "Dimensions exceed maximum allowed size".into(),
            ));
        }

        // Explicitly verify the output buffer is large enough
        let expected_size = (out_rect.width as usize)
            .checked_mul(out_rect.height as usize)
            .and_then(|s| s.checked_mul(4))
            .ok_or_else(|| DecodeError::DecodingFailed("Output dimensions overflow".into()))?;

        if out_pixels.len() < expected_size {
            return Err(DecodeError::DecodingFailed(
                "Output buffer too small".into(),
            ));
        }

        let mut config: libwebp_sys::WebPDecoderConfig = unsafe { std::mem::zeroed() };
        if !unsafe { libwebp_sys::WebPInitDecoderConfig(&mut config) } {
            return Err(DecodeError::DecodingFailed(
                "WebPInitDecoderConfig failed".into(),
            ));
        }

        config.output.colorspace = libwebp_sys::WEBP_CSP_MODE::MODE_RGBA;
        config.output.is_external_memory = 1;
        config.output.u.RGBA.rgba = out_pixels.as_mut_ptr();
        config.output.u.RGBA.stride = (out_rect.width * 4) as i32;
        config.output.u.RGBA.size = expected_size;

        let original_width = self.info.image_width;
        let original_height = self.info.image_height;

        if in_rect.width != original_width || in_rect.height != original_height {
            config.options.use_cropping = 1;
            config.options.crop_left = in_rect.x as i32;
            config.options.crop_top = in_rect.y as i32;
            config.options.crop_width = in_rect.width as i32;
            config.options.crop_height = in_rect.height as i32;
        }

        if out_rect.width != in_rect.width || out_rect.height != in_rect.height {
            config.options.use_scaling = 1;
            config.options.scaled_width = out_rect.width as i32;
            config.options.scaled_height = out_rect.height as i32;
        }

        let status =
            unsafe { libwebp_sys::WebPDecode(self.data.as_ptr(), self.data.len(), &mut config) };

        // 3. Always clean up the decoder config properly to prevent hidden C-side leaks
        unsafe { libwebp_sys::WebPFreeDecBuffer(&mut config.output) };

        if status != libwebp_sys::VP8StatusCode::VP8_STATUS_OK {
            return Err(DecodeError::DecodingFailed(format!(
                "WebP decode failed: {:?}",
                status
            )));
        }

        Ok(())
    }

    fn use_transform(&self) -> bool {
        false
    }

    fn lcms_in_type(&self) -> u32 {
        0
    }
}
