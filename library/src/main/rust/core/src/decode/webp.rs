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

        let mut rgba_buf = image.to_image().into_rgba8().into_raw();
        // Convert RGBA to grayscale in-place to avoid a second allocation.
        let pixel_count = (image_width * image_height) as usize;
        for i in 0..pixel_count {
            let base = i * 4;
            rgba_buf[i] = rgb_to_luma(rgba_buf[base], rgba_buf[base + 1], rgba_buf[base + 2]);
        }
        bounds = find_borders(&rgba_buf[..pixel_count], image_width, image_height);
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

        let rgba = image.to_image().into_rgba8();
        let full_width = self.info.image_width;

        downsample_region(
            rgba.as_raw(),
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
