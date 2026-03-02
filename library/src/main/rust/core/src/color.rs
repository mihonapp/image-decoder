use crate::decode::DecodeError;

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

    // Save alpha channel (lcms2 may clobber it)
    let alphas: Vec<u8> = pixels[..pixel_count * 4]
        .chunks_exact(4)
        .map(|px| px[3])
        .collect();

    // Transform in-place: reinterpret &[u8] as &[[u8; 4]]
    let src_slice: &[[u8; 4]] = bytemuck::cast_slice(&pixels[..pixel_count * 4]);
    let mut output: Vec<[u8; 4]> = vec![[0u8; 4]; pixel_count];
    t.transform_pixels(src_slice, &mut output);
    pixels[..pixel_count * 4].copy_from_slice(bytemuck::cast_slice(&output));

    // Restore alpha
    for (i, &a) in alphas.iter().enumerate() {
        pixels[i * 4 + 3] = a;
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
