use crate::decode::DecodeError;
use crate::types::Rect;
use fast_image_resize as fir;

/// Extract the region `in_rect` from `src_pixels` (full image of `src_width`
/// pixels, `components` bytes per pixel), then downsample to `out_rect`
/// dimensions, writing the result into `out_pixels`.
///
/// When `sample_size == 1` this is a simple blit of the region.
///
/// Downsampling uses a SIMD-optimised bicubic (Catmull-Rom) filter via
/// `fast_image_resize`.
pub fn downsample_region(
    src_pixels: &[u8],
    src_width: u32,
    components: u32,
    in_rect: Rect,
    out_rect: Rect,
    sample_size: u32,
    out_pixels: &mut [u8],
) -> Result<(), DecodeError> {
    let stride = (src_width * components) as usize;
    let crop_stride = (in_rect.width * components) as usize;

    // sample_size == 1: simple blit
    if sample_size <= 1 || (out_rect.width == in_rect.width && out_rect.height == in_rect.height) {
        if in_rect.x == 0 && in_rect.width == src_width {
            let start = in_rect.y as usize * stride;
            let len = in_rect.height as usize * crop_stride;
            out_pixels[..len].copy_from_slice(&src_pixels[start..start + len]);
        } else {
            // Row-by-row copy.
            for row in 0..in_rect.height as usize {
                let src_start =
                    (in_rect.y as usize + row) * stride + in_rect.x as usize * components as usize;
                let dst_start = row * crop_stride;
                out_pixels[dst_start..dst_start + crop_stride]
                    .copy_from_slice(&src_pixels[src_start..src_start + crop_stride]);
            }
        }
        return Ok(());
    }

    // sample_size >= 2: bicubic downsample (SIMD-optimised)
    let crop_w = in_rect.width;
    let crop_h = in_rect.height;
    let dst_w = out_rect.width;
    let dst_h = out_rect.height;

    if crop_w == 0 || crop_h == 0 || dst_w == 0 || dst_h == 0 {
        return Err(DecodeError::InvalidRegion("zero dimension".into()));
    }

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

    // Borrow the cropped region without allocating when rows are contiguous.
    let is_contiguous = in_rect.x == 0 && in_rect.width == src_width;
    let cropped_owned: Vec<u8>;
    let cropped: &[u8] = if is_contiguous {
        let start = in_rect.y as usize * stride;
        &src_pixels[start..start + crop_h as usize * crop_stride]
    } else {
        cropped_owned = {
            let mut buf = vec![0u8; crop_h as usize * crop_stride];
            for row in 0..crop_h as usize {
                let src_start =
                    (in_rect.y as usize + row) * stride + in_rect.x as usize * components as usize;
                let dst_start = row * crop_stride;
                buf[dst_start..dst_start + crop_stride]
                    .copy_from_slice(&src_pixels[src_start..src_start + crop_stride]);
            }
            buf
        };
        &cropped_owned
    };

    let src_image = fir::images::ImageRef::new(crop_w, crop_h, cropped, pixel_type)
        .map_err(|e| DecodeError::DecodingFailed(format!("resize src: {e}")))?;

    // Write directly into the caller's output buffer 
    let mut dst_image = fir::images::Image::from_slice_u8(dst_w, dst_h, out_pixels, pixel_type)
        .map_err(|e| DecodeError::DecodingFailed(format!("resize dst: {e}")))?;

    let options = fir::ResizeOptions::new()
        .resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::CatmullRom));

    let mut resizer = fir::Resizer::new();
    resizer
        .resize(&src_image, &mut dst_image, Some(&options))
        .map_err(|e| DecodeError::DecodingFailed(format!("resize: {e}")))?;

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
