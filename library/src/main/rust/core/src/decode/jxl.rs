use crate::borders::find_borders;
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
                .map(|px| ((px[0] as u16 * 299 + px[1] as u16 * 587 + px[2] as u16 * 114) / 1000) as u8)
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
    let buf = fb.buf(); // interleaved f32 samples: [R, G, B, A, R, G, B, A, ...]

    let mut rgba = vec![255u8; width * height * 4];

    for i in 0..(width * height) {
        let base = i * num_channels;
        let r = (buf[base] * 255.0).clamp(0.0, 255.0) as u8;
        let g = if num_channels > 1 {
            (buf[base + 1] * 255.0).clamp(0.0, 255.0) as u8
        } else {
            r
        };
        let b = if num_channels > 2 {
            (buf[base + 2] * 255.0).clamp(0.0, 255.0) as u8
        } else {
            r
        };
        let a = if num_channels > 3 {
            (buf[base + 3] * 255.0).clamp(0.0, 255.0) as u8
        } else {
            255u8
        };
        rgba[i * 4] = r;
        rgba[i * 4 + 1] = g;
        rgba[i * 4 + 2] = b;
        rgba[i * 4 + 3] = a;
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

        downsample_region(&rgba, full_width, 4, in_rect, out_rect, sample_size, out_pixels)
    }

    fn use_transform(&self) -> bool {
        false
    }

    fn lcms_in_type(&self) -> u32 {
        0
    }
}
