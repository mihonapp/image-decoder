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

    // Use stream() to convert directly from grid to u8, avoiding the
    // intermediate f32 FrameBuffer allocation that image_all_channels() creates.
    let mut stream = render.stream();
    let num_channels = stream.channels() as usize;

    if num_channels == 4 {
        // RGBA — write directly into the output buffer.
        let mut rgba = vec![0u8; width * height * 4];
        stream.write_to_buffer::<u8>(&mut rgba);
        Ok(rgba)
    } else {
        // Stream has fewer channels (e.g. RGB=3, Gray=1).
        // Read into a compact buffer, then expand to RGBA.
        let mut compact = vec![0u8; width * height * num_channels];
        stream.write_to_buffer::<u8>(&mut compact);

        let mut rgba = vec![255u8; width * height * 4];
        match num_channels {
            3 => {
                for (dst, src) in rgba.chunks_exact_mut(4).zip(compact.chunks_exact(3)) {
                    let [r, g, b] = [src[0], src[1], src[2]];
                    dst[0] = r;
                    dst[1] = g;
                    dst[2] = b;
                    // dst[3] already 255
                }
            }
            1 => {
                for (dst, &luma) in rgba.chunks_exact_mut(4).zip(compact.iter()) {
                    dst[0] = luma;
                    dst[1] = luma;
                    dst[2] = luma;
                    // dst[3] already 255
                }
            }
            _ => {
                for (dst, src) in rgba
                    .chunks_exact_mut(4)
                    .zip(compact.chunks_exact(num_channels))
                {
                    dst[0] = src[0];
                    dst[1] = src.get(1).copied().unwrap_or(src[0]);
                    dst[2] = src.get(2).copied().unwrap_or(src[0]);
                    dst[3] = src.get(3).copied().unwrap_or(255);
                }
            }
        }
        Ok(rgba)
    }
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
