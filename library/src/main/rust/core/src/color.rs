use crate::decode::DecodeError;

/// Convert an RGB pixel to a single luma (grayscale) value
/// using the BT.601 luminance formula.
#[inline]
pub fn rgb_to_luma(r: u8, g: u8, b: u8) -> u8 {
    ((r as u16 * 299 + g as u16 * 587 + b as u16 * 114) / 1000) as u8
}

/// Apply an ICC color transform from `src_profile_data` to `dst_profile_data`
/// on the RGBA pixels in `pixels`.
///
/// `pixel_count` is the number of pixels (length = pixel_count * 4).
/// If `dst_profile_data` is `None`, sRGB is used as the target.
pub fn transform_pixels(
    pixels: &mut [u8],
    pixel_count: usize,
    src_profile_data: Option<&[u8]>,
    dst_profile_data: Option<&[u8]>,
) -> Result<(), DecodeError> {
    let src_profile = match src_profile_data {
        Some(data) => lcms2::Profile::new_icc(data)
            .map_err(|e| DecodeError::ColorManagement(format!("src profile: {e}")))?,
        None => lcms2::Profile::new_srgb(),
    };

    let dst_profile = match dst_profile_data {
        Some(data) => lcms2::Profile::new_icc(data)
            .map_err(|e| DecodeError::ColorManagement(format!("dst profile: {e}")))?,
        None => lcms2::Profile::new_srgb(),
    };

    let t = lcms2::Transform::new(
        &src_profile,
        lcms2::PixelFormat::RGBA_8,
        &dst_profile,
        lcms2::PixelFormat::RGBA_8,
        lcms2::Intent::Perceptual,
    )
    .map_err(|e| DecodeError::ColorManagement(format!("transform: {e}")))?;

    // Save alpha channel values as lcms2 may overwrite them.
    let alphas: Vec<u8> = pixels[..pixel_count * 4]
        .chunks_exact(4)
        .map(|px| px[3])
        .collect();

    // In-place transform: avoids allocating a full intermediate buffer.
    t.transform_in_place(&mut pixels[..pixel_count * 4]);

    // Restore original alpha values.
    for (chunk, &alpha) in pixels[..pixel_count * 4]
        .chunks_exact_mut(4)
        .zip(alphas.iter())
    {
        chunk[3] = alpha;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_srgb_transform() {
        // sRGB -> sRGB should leave pixels unchanged (within rounding)
        let mut pixels = vec![128u8, 64, 32, 255, 0, 0, 0, 255];
        let original = pixels.clone();
        transform_pixels(&mut pixels, 2, None, None).unwrap();
        for (i, (a, b)) in pixels.iter().zip(original.iter()).enumerate() {
            assert!(
                (*a as i16 - *b as i16).unsigned_abs() <= 2,
                "pixel byte {i}: expected {b}, got {a}"
            );
        }
    }
}
