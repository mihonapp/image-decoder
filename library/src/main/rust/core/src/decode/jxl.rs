use crate::borders::find_borders;
use crate::color::rgb_to_luma;
use crate::decode::{DecodeError, Decoder};
use crate::resize::downsample_region;
use crate::types::{ImageInfo, Rect};

/// JPEG XL decoder backed by `jxl-oxide`.
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
    let image = jxl_oxide::JxlImage::builder()
        .read(std::io::Cursor::new(data))
        .map_err(|e| DecodeError::DecodingFailed(format!("JXL header: {e}")))?;

    let image_width = image.width();
    let image_height = image.height();

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

/// Decode the JXL image to an RGBA u8 buffer.
fn decode_rgba(data: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let image = jxl_oxide::JxlImage::builder()
        .read(std::io::Cursor::new(data))
        .map_err(|e| DecodeError::DecodingFailed(format!("JXL: {e}")))?;

    let width = image.width() as usize;
    let height = image.height() as usize;

    let render = image
        .render_frame(0)
        .map_err(|e| DecodeError::DecodingFailed(format!("JXL render: {e}")))?;

    let fb = render.image_all_channels();
    let num_channels = fb.channels();
    let buf = fb.buf(); // interleaved f32 samples

    let mut rgba = vec![255u8; width * height * 4];

    // Use chunk iterators to avoid per-pixel index arithmetic and bounds checks.
    let to_u8 = |v: f32| (v * 255.0).clamp(0.0, 255.0) as u8;

    match num_channels {
        4 => {
            for (dst, src) in rgba.chunks_exact_mut(4).zip(buf.chunks_exact(4)) {
                let [r, g, b, a] = [src[0], src[1], src[2], src[3]];
                dst[0] = to_u8(r);
                dst[1] = to_u8(g);
                dst[2] = to_u8(b);
                dst[3] = to_u8(a);
            }
        }
        3 => {
            for (dst, src) in rgba.chunks_exact_mut(4).zip(buf.chunks_exact(3)) {
                let [r, g, b] = [src[0], src[1], src[2]];
                dst[0] = to_u8(r);
                dst[1] = to_u8(g);
                dst[2] = to_u8(b);
                // dst[3] already 255
            }
        }
        1 => {
            for (dst, src) in rgba.chunks_exact_mut(4).zip(buf.iter()) {
                let v = (*src * 255.0).clamp(0.0, 255.0) as u8;
                dst[0] = v;
                dst[1] = v;
                dst[2] = v;
                // dst[3] already 255
            }
        }
        _ => {
            for (dst, src) in rgba.chunks_exact_mut(4).zip(buf.chunks_exact(num_channels)) {
                dst[0] = (src[0] * 255.0).clamp(0.0, 255.0) as u8;
                dst[1] = if num_channels > 1 {
                    (src[1] * 255.0).clamp(0.0, 255.0) as u8
                } else {
                    dst[0]
                };
                dst[2] = if num_channels > 2 {
                    (src[2] * 255.0).clamp(0.0, 255.0) as u8
                } else {
                    dst[0]
                };
                dst[3] = if num_channels > 3 {
                    (src[3] * 255.0).clamp(0.0, 255.0) as u8
                } else {
                    255
                };
            }
        }
    }

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
