use crate::borders::find_borders;
use crate::decode::{DecodeError, Decoder};
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};

/// HEIF / AVIF decoder backed by `libheif-rs`.
pub struct HeifDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    #[allow(dead_code)]
    target_profile_data: Option<Vec<u8>>,
}

impl HeifDecoder {
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
    let ctx = libheif_rs::HeifContext::read_from_bytes(data)
        .map_err(|e| DecodeError::DecodingFailed(format!("HEIF context: {e}")))?;

    let handle = ctx
        .primary_image_handle()
        .map_err(|e| DecodeError::DecodingFailed(format!("HEIF handle: {e}")))?;

    let image_width = handle.width();
    let image_height = handle.height();

    let mut bounds = Rect::full(image_width, image_height);

    if crop_borders {
        if let Ok(rgba) = decode_rgba_from_ctx(&ctx) {
            let mut gray = vec![0u8; (image_width * image_height) as usize];
            for i in 0..(image_width * image_height) as usize {
                let r = rgba[i * 4] as u16;
                let g = rgba[i * 4 + 1] as u16;
                let b = rgba[i * 4 + 2] as u16;
                gray[i] = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
            }
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

fn decode_rgba_from_ctx(ctx: &libheif_rs::HeifContext) -> Result<Vec<u8>, DecodeError> {
    let handle = ctx
        .primary_image_handle()
        .map_err(|e| DecodeError::DecodingFailed(format!("HEIF handle: {e}")))?;

    let lib_heif = libheif_rs::LibHeif::new();
    let image = lib_heif
        .decode(
            &handle,
            libheif_rs::ColorSpace::Rgb(libheif_rs::RgbChroma::Rgba),
            None,
        )
        .map_err(|e| DecodeError::DecodingFailed(format!("HEIF decode: {e}")))?;

    let plane = image
        .planes()
        .interleaved
        .ok_or_else(|| DecodeError::DecodingFailed("HEIF: no interleaved plane".into()))?;

    let width = image.width() as usize;
    let height = image.height() as usize;
    let stride = plane.stride;

    // Copy row-by-row to handle stride != width*4
    let mut rgba = vec![0u8; width * height * 4];
    for y in 0..height {
        let src_start = y * stride;
        let src_end = src_start + width * 4;
        let dst_start = y * width * 4;
        let dst_end = dst_start + width * 4;
        rgba[dst_start..dst_end].copy_from_slice(&plane.data[src_start..src_end]);
    }

    Ok(rgba)
}

impl Decoder for HeifDecoder {
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
        let ctx = libheif_rs::HeifContext::read_from_bytes(&self.data)
            .map_err(|e| DecodeError::DecodingFailed(format!("HEIF: {e}")))?;

        let rgba = decode_rgba_from_ctx(&ctx)?;
        let full_width = self.info.image_width;

        downsample_region(&rgba, full_width, 4, in_rect, out_rect, sample_size, out_pixels)
    }

    fn use_transform(&self) -> bool {
        false
    }

    fn lcms_in_type(&self) -> u32 {
        0
    }
}
