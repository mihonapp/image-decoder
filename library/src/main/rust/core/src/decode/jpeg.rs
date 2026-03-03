use crate::borders::find_borders;
use crate::decode::{DecodeError, Decoder};
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};

/// JPEG decoder backed by `libjpeg-turbo` via the `turbojpeg` crate.
pub struct JpegDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    #[allow(dead_code)]
    target_profile_data: Option<Vec<u8>>,
}

impl JpegDecoder {
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
    let mut decompressor = turbojpeg::Decompressor::new()
        .map_err(|e| DecodeError::DecodingFailed(format!("TurboJPEG init: {e}")))?;

    let header = decompressor
        .read_header(data)
        .map_err(|e| DecodeError::DecodingFailed(format!("JPEG header: {e}")))?;

    let image_width = header.width as u32;
    let image_height = header.height as u32;
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

        // Formula aligns with C macro `TJSCALED`: ceil(dim * num / denom)
        let idct_width = (full_width + scale_denom - 1) / scale_denom;
        let idct_height = (full_height + scale_denom - 1) / scale_denom;

        let mut decompressor = turbojpeg::Decompressor::new()
            .map_err(|e| DecodeError::DecodingFailed(format!("TurboJPEG init: {e}")))?;

        decompressor
            .read_header(&self.data)
            .map_err(|e| DecodeError::DecodingFailed(format!("JPEG header: {e}")))?;

        let mut idct_pixels = vec![0u8; (idct_width * idct_height * 4) as usize];
        let image = turbojpeg::Image {
            pixels: idct_pixels.as_mut_slice(),
            width: idct_width as usize,
            pitch: (idct_width * 4) as usize,
            height: idct_height as usize,
            format: turbojpeg::PixelFormat::RGBA,
        };

        decompressor
            .decompress(&self.data, image)
            .map_err(|e| DecodeError::DecodingFailed(format!("JPEG decode: {e}")))?;

        let scaled_in_rect = Rect {
            x: in_rect.x / scale_denom,
            y: in_rect.y / scale_denom,
            width: in_rect.width / scale_denom,
            height: in_rect.height / scale_denom,
        };

        let remaining_sample_size = sample_size / scale_denom;

        downsample_region(
            &idct_pixels,
            idct_width,
            4,
            scaled_in_rect,
            out_rect,
            remaining_sample_size,
            out_pixels,
        )?;

        Ok(())
    }

    fn use_transform(&self) -> bool {
        false
    }

    fn lcms_in_type(&self) -> u32 {
        0
    }
}