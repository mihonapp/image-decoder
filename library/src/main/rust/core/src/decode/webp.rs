use crate::borders::find_borders;
use crate::color::rgb_to_luma;
use crate::decode::{DecodeError, Decoder};
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};

/// WebP decoder backed by the `webp` crate.
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
    // Get dimensions from the bitstream header without decoding pixels.
    let features = webp::BitstreamFeatures::new(data)
        .ok_or_else(|| DecodeError::DecodingFailed("WebP: invalid bitstream".into()))?;

    let image_width = features.width();
    let image_height = features.height();

    let mut bounds = Rect::full(image_width, image_height);

    if crop_borders {
        // Full decode is only needed for border detection.
        let decoder = webp::Decoder::new(data);
        let image = decoder
            .decode()
            .ok_or_else(|| DecodeError::DecodingFailed("WebP decode failed".into()))?;

        let is_alpha = image.is_alpha();
        let pixel_count = (image_width * image_height) as usize;
        let mut gray_buf = vec![0u8; pixel_count];
        
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
        sample_size: u32,
    ) -> Result<(), DecodeError> {
        let decoder = webp::Decoder::new(&self.data);
        let image = decoder
            .decode()
            .ok_or_else(|| DecodeError::DecodingFailed("WebP decode failed".into()))?;

        let is_alpha = image.is_alpha();
        let full_width = self.info.image_width;
        let components = if is_alpha { 4 } else { 3 };

        downsample_region(
            &*image,
            full_width,
            components,
            in_rect,
            out_rect,
            sample_size,
            out_pixels,
        )?;

        // If the source was RGB (3 components), `downsample_region` will write an RGB output
        // into the front of `out_pixels`.  However, `out_pixels` is sized for RGBA (4 bytes per pixel).
        // We must expand the RGB result in-place to RGBA.
        if !is_alpha {
            let pixel_count = (out_rect.width * out_rect.height) as usize;
            for i in (0..pixel_count).rev() {
                let src_base = i * 3;
                let dst_base = i * 4;
                // Read first to avoid overwriting if memory overlaps
                let r = out_pixels[src_base];
                let g = out_pixels[src_base + 1];
                let b = out_pixels[src_base + 2];
                out_pixels[dst_base] = r;
                out_pixels[dst_base + 1] = g;
                out_pixels[dst_base + 2] = b;
                out_pixels[dst_base + 3] = 255;
            }
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
