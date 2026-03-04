use crate::borders::find_borders;
use crate::color::rgb_to_luma;
use crate::decode::{DecodeError, Decoder};
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};

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
        let lib_heif = libheif_rs::LibHeif::new();
        if let Ok(image) = lib_heif.decode(
            &handle,
            libheif_rs::ColorSpace::Rgb(libheif_rs::RgbChroma::Rgb),
            None,
        ) {
            if let Some(plane) = image.planes().interleaved {
                let stride = plane.stride as usize;

                let luma: Vec<u8> = plane
                    .data
                    .chunks(stride)
                    .take(image_height as usize)
                    .flat_map(|src_row| {
                        let valid_src_pixels = &src_row[..image_width as usize * 3];
                        valid_src_pixels
                            .chunks_exact(3)
                            .map(|rgb| rgb_to_luma(rgb[0], rgb[1], rgb[2]))
                    })
                    .collect();

                bounds = find_borders(&luma, image_width, image_height);
            }
        }
    }

    Ok(ImageInfo {
        image_width,
        image_height,
        is_animated: false,
        bounds,
    })
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

        let width = image.width() as u32;
        let height = image.height() as u32;
        let stride = plane.stride as u32;

        if stride == width * 4 {
            downsample_region(
                plane.data,
                width,
                4,
                in_rect,
                out_rect,
                sample_size,
                out_pixels,
            )
        } else {
            // Multiply in usize space to prevent u32 wrap-around panic
            let w_usize = width as usize;
            let h_usize = height as usize;
            let stride_usize = stride as usize;

            let rgba: Vec<u8> = plane
                .data
                .chunks(stride_usize)
                .take(h_usize)
                .flat_map(|row| row[..w_usize * 4].iter().copied())
                .collect();
            downsample_region(&rgba, width, 4, in_rect, out_rect, sample_size, out_pixels)
        }
    }

    fn use_transform(&self) -> bool {
        false
    }

    fn lcms_in_type(&self) -> u32 {
        0
    }
}
