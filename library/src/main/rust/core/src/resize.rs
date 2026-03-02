use crate::decode::DecodeError;
use crate::types::Rect;
use fast_image_resize as fir;

/// Extract the region `in_rect` from `src_pixels` (full image of `src_width`
/// pixels, `components` bytes per pixel), then downsample to `out_rect`
/// dimensions, writing the result into `out_pixels`.
///
/// When `sample_size == 1` this is a simple blit of the region.
pub fn downsample_region(
    src_pixels: &[u8],
    src_width: u32,
    components: u32,
    in_rect: Rect,
    out_rect: Rect,
    sample_size: u32,
    out_pixels: &mut [u8],
) -> Result<(), DecodeError> {
    let stride = src_width * components;
    let crop_w = in_rect.width;
    let crop_h = in_rect.height;
    let crop_stride = crop_w * components;

    // Check if the source rows are already contiguous in memory
    // (no x-offset and region width == source width).
    let is_contiguous = in_rect.x == 0 && in_rect.width == src_width;

    // Get a contiguous view of the cropped region, borrowing when possible.
    let cropped_owned: Vec<u8>;
    let cropped: &[u8] = if is_contiguous {
        let start = (in_rect.y * stride) as usize;
        let end = start + (crop_h * crop_stride) as usize;
        if end > src_pixels.len() {
            return Err(DecodeError::InvalidRegion(
                "source region out of bounds".into(),
            ));
        }
        &src_pixels[start..end]
    } else {
        cropped_owned = {
            let mut buf = vec![0u8; (crop_stride * crop_h) as usize];
            for row in 0..crop_h {
                let src_y = in_rect.y + row;
                let src_start = (src_y * stride + in_rect.x * components) as usize;
                let src_end = src_start + crop_stride as usize;
                let dst_start = (row * crop_stride) as usize;
                let dst_end = dst_start + crop_stride as usize;
                if src_end > src_pixels.len() {
                    return Err(DecodeError::InvalidRegion(format!(
                        "source row {} out of bounds",
                        src_y
                    )));
                }
                buf[dst_start..dst_end].copy_from_slice(&src_pixels[src_start..src_end]);
            }
            buf
        };
        &cropped_owned
    };

    // If no downsampling needed, just copy the cropped pixels directly.
    if sample_size <= 1 || (out_rect.width == crop_w && out_rect.height == crop_h) {
        let len = (out_rect.width * out_rect.height * components) as usize;
        out_pixels[..len].copy_from_slice(&cropped[..len]);
        return Ok(());
    }

    // Use fast_image_resize to scale down.
    if crop_w == 0 || crop_h == 0 || out_rect.width == 0 || out_rect.height == 0 {
        return Err(DecodeError::InvalidRegion("zero dimension".into()));
    }
    let dst_w = out_rect.width;
    let dst_h = out_rect.height;

    let pixel_type = match components {
        1 => fir::PixelType::U8,
        2 => fir::PixelType::U8x2,
        4 => fir::PixelType::U8x4,
        _ => {
            return Err(DecodeError::DecodingFailed(format!(
                "unsupported component count {components}"
            )));
        }
    };

    // Use ImageRef to borrow the cropped slice, avoids cloning into a Vec.
    let src_image = fir::images::ImageRef::new(crop_w, crop_h, cropped, pixel_type)
        .map_err(|e| DecodeError::DecodingFailed(format!("resize src: {e}")))?;

    let mut dst_image = fir::images::Image::new(dst_w, dst_h, pixel_type);

    let mut resizer = fir::Resizer::new();
    resizer
        .resize(&src_image, &mut dst_image, None)
        .map_err(|e| DecodeError::DecodingFailed(format!("resize: {e}")))?;

    let result = dst_image.into_vec();
    let len = (dst_w * dst_h * components) as usize;
    out_pixels[..len].copy_from_slice(&result[..len]);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_blit() {
        // 4x4 RGBA image, extract full region at sample_size 1
        let w = 4u32;
        let h = 4u32;
        let src: Vec<u8> = (0..(w * h * 4) as u8).collect();
        let mut out = vec![0u8; src.len()];
        downsample_region(&src, w, 4, Rect::full(w, h), Rect::full(w, h), 1, &mut out).unwrap();
        assert_eq!(src, out);
    }

    #[test]
    fn crop_subregion() {
        // 4x4 RGBA, extract center 2x2
        let w = 4u32;
        let h = 4u32;
        let mut src = vec![0u8; (w * h * 4) as usize];
        // Fill each pixel with its linear index
        for i in 0..(w * h) as usize {
            src[i * 4] = i as u8;
            src[i * 4 + 1] = i as u8;
            src[i * 4 + 2] = i as u8;
            src[i * 4 + 3] = 255;
        }

        let mut out = vec![0u8; (2 * 2 * 4) as usize];
        let in_rect = Rect::new(1, 1, 2, 2);
        let out_rect = Rect::new(0, 0, 2, 2);
        downsample_region(&src, w, 4, in_rect, out_rect, 1, &mut out).unwrap();

        // Pixel at (1,1) in src = index 5
        assert_eq!(out[0], 5);
        // Pixel at (2,1) in src = index 6
        assert_eq!(out[4], 6);
    }

    #[test]
    fn downsample_halves() {
        // 4x4 single-channel, downsample to 2x2
        let w = 4u32;
        let h = 4u32;
        let src = vec![128u8; (w * h) as usize];
        let mut out = vec![0u8; (2 * 2) as usize];
        downsample_region(
            &src,
            w,
            1,
            Rect::full(w, h),
            Rect::new(0, 0, 2, 2),
            2,
            &mut out,
        )
        .unwrap();

        // All output pixels should be close to 128
        for &v in &out {
            assert!((v as i32 - 128).unsigned_abs() < 4);
        }
    }
}
