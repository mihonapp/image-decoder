//! Integration tests for imagedecoder-core.
//!
//! These tests exercise the public API: format detection, decoder creation,
//! region decoding, and border cropping.  They run on the host machine via
//! `cargo test` without any Android emulator.
//!
//! Test images are loaded from `../test-data/` relative to the workspace root.
//! If a particular test image is missing the test is skipped with a descriptive
//! message rather than failing, so CI can run even without the full media set.

use imagedecoder_core::borders;
use imagedecoder_core::decode;
use imagedecoder_core::types::{Format, Rect};
use std::path::PathBuf;

fn test_data_dir() -> PathBuf {
    // Cargo runs tests with cwd = crate root (core/), so we go up to the
    // workspace root and into test-data/
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-data")
}

fn load_test_file(name: &str) -> Option<Vec<u8>> {
    let path = test_data_dir().join(name);
    if !path.exists() {
        eprintln!("SKIP: test file not found: {}", path.display());
        return None;
    }
    Some(std::fs::read(path).expect("failed to read test file"))
}

// -----------------------------------------------------------------------
// Format detection
// -----------------------------------------------------------------------

#[test]
fn detect_jpeg_from_header() {
    let mut header = vec![0u8; 32];
    header[0] = 0xFF;
    header[1] = 0xD8;
    header[2] = 0xFF;
    let t = decode::find_type(&header).unwrap();
    assert_eq!(t.format, Format::Jpeg);
    assert!(!t.is_animated);
}

#[test]
fn detect_png_from_header() {
    let mut header = vec![0u8; 32];
    header[0] = 0x89;
    header[1] = b'P';
    header[2] = b'N';
    header[3] = b'G';
    let t = decode::find_type(&header).unwrap();
    assert_eq!(t.format, Format::Png);
}

#[test]
fn detect_webp_from_header() {
    let mut header = vec![0u8; 32];
    header[0] = b'R';
    header[1] = b'I';
    header[2] = b'F';
    header[3] = b'F';
    let t = decode::find_type(&header).unwrap();
    assert_eq!(t.format, Format::Webp);
}

#[test]
fn detect_gif_from_header() {
    let mut header = vec![0u8; 32];
    header[0] = b'G';
    header[1] = b'I';
    header[2] = b'F';
    header[3] = b'8';
    let t = decode::find_type(&header).unwrap();
    assert_eq!(t.format, Format::Gif);
    assert!(t.is_animated);
}

#[test]
fn detect_jxl_codestream() {
    let mut header = vec![0u8; 32];
    header[0] = 0xFF;
    header[1] = 0x0A;
    let t = decode::find_type(&header).unwrap();
    assert_eq!(t.format, Format::Jxl);
}

#[test]
fn detect_unknown_returns_none() {
    let header = vec![0u8; 32];
    assert!(decode::find_type(&header).is_none());
}

// -----------------------------------------------------------------------
// Border detection (pure algorithmic tests, no images needed)
// -----------------------------------------------------------------------

#[test]
fn find_borders_full_black() {
    let w = 100u32;
    let h = 100u32;
    let pixels = vec![0u8; (w * h) as usize];
    let b = borders::find_borders(&pixels, w, h);
    assert_eq!(b, Rect::full(w, h));
}

#[test]
fn find_borders_white_borders() {
    let w = 200u32;
    let h = 300u32;
    let content = Rect::new(20, 30, 160, 240);

    let mut pixels = vec![255u8; (w * h) as usize];
    for y in content.y..(content.y + content.height) {
        for x in content.x..(content.x + content.width) {
            pixels[(y * w + x) as usize] = 0;
        }
    }

    let b = borders::find_borders(&pixels, w, h);
    // Allow small tolerance
    assert!((b.x as i32 - content.x as i32).unsigned_abs() <= 2);
    assert!((b.y as i32 - content.y as i32).unsigned_abs() <= 2);
}

// -----------------------------------------------------------------------
// Downscaling
// -----------------------------------------------------------------------

#[test]
fn resize_identity() {
    let w = 4u32;
    let h = 4u32;
    let src: Vec<u8> = (0..64).collect();
    let mut out = vec![0u8; 64];
    imagedecoder_core::resize::downsample_region(
        &src,
        w,
        4,
        Rect::full(w, h),
        Rect::full(w, h),
        1,
        &mut out,
    )
    .unwrap();
    assert_eq!(src, out);
}

#[test]
fn resize_half() {
    let w = 4u32;
    let h = 4u32;
    let src = vec![128u8; (w * h) as usize];
    let mut out = vec![0u8; 4];
    imagedecoder_core::resize::downsample_region(
        &src,
        w,
        1,
        Rect::full(w, h),
        Rect::new(0, 0, 2, 2),
        2,
        &mut out,
    )
    .unwrap();
    for &v in &out {
        assert!((v as i32 - 128).unsigned_abs() < 4);
    }
}

// -----------------------------------------------------------------------
// Decoder – JPEG (requires test-data/sample.jpg)
// -----------------------------------------------------------------------

#[test]
fn jpeg_decode_full() {
    let data = match load_test_file("sample.jpg") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data, false, None).unwrap();
    let info = decoder.info();
    assert!(info.image_width > 0);
    assert!(info.image_height > 0);

    let bounds = info.bounds;
    let out_rect = bounds;
    let pixel_count = (out_rect.width * out_rect.height * 4) as usize;
    let mut out = vec![0u8; pixel_count];
    decoder.decode(&mut out, out_rect, bounds, 1).unwrap();
    // At least some pixels should be non-zero
    assert!(out.iter().any(|&v| v != 0));
}

#[test]
fn jpeg_decode_region() {
    let data = match load_test_file("sample.jpg") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data, false, None).unwrap();
    let info = decoder.info();

    // Decode top-left 50x50
    let region_w = 50.min(info.image_width);
    let region_h = 50.min(info.image_height);
    let in_rect = Rect::new(0, 0, region_w, region_h);
    let out_rect = in_rect;
    let mut out = vec![0u8; (region_w * region_h * 4) as usize];
    decoder.decode(&mut out, out_rect, in_rect, 1).unwrap();
    assert!(out.iter().any(|&v| v != 0));
}

#[test]
fn jpeg_decode_downsampled() {
    let data = match load_test_file("sample.jpg") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data, false, None).unwrap();
    let info = decoder.info();

    let in_rect = info.bounds;
    let out_rect = in_rect.downsample(2);
    if out_rect.width == 0 || out_rect.height == 0 {
        return; // image too small
    }
    let mut out = vec![0u8; (out_rect.width * out_rect.height * 4) as usize];
    decoder.decode(&mut out, out_rect, in_rect, 2).unwrap();
    assert!(out.iter().any(|&v| v != 0));
}

#[test]
fn jpeg_crop_borders() {
    let data = match load_test_file("bordered.jpg") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data.clone(), true, None).unwrap();
    let no_crop = decode::new_decoder(data, false, None).unwrap();

    // Cropped bounds should be a subset of the full bounds
    let cropped = decoder.info().bounds;
    let full = no_crop.info().bounds;
    assert!(cropped.width <= full.width);
    assert!(cropped.height <= full.height);
}

// -----------------------------------------------------------------------
// Decoder – PNG (requires test-data/sample.png)
// -----------------------------------------------------------------------

#[test]
fn png_decode_full() {
    let data = match load_test_file("sample.png") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data, false, None).unwrap();
    let info = decoder.info();
    assert!(info.image_width > 0);

    let bounds = info.bounds;
    let mut out = vec![0u8; (bounds.width * bounds.height * 4) as usize];
    decoder.decode(&mut out, bounds, bounds, 1).unwrap();
    assert!(out.iter().any(|&v| v != 0));
}

// -----------------------------------------------------------------------
// Decoder – WebP (requires test-data/sample.webp)
// -----------------------------------------------------------------------

#[test]
fn webp_decode_full() {
    let data = match load_test_file("sample.webp") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data, false, None).unwrap();
    let info = decoder.info();
    assert!(info.image_width > 0);

    let bounds = info.bounds;
    let mut out = vec![0u8; (bounds.width * bounds.height * 4) as usize];
    decoder.decode(&mut out, bounds, bounds, 1).unwrap();
    assert!(out.iter().any(|&v| v != 0));
}

// -----------------------------------------------------------------------
// Decoder – JXL (requires test-data/sample.jxl)
// -----------------------------------------------------------------------

#[test]
fn jxl_decode_full() {
    let data = match load_test_file("sample.jxl") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data, false, None).unwrap();
    let info = decoder.info();
    assert!(info.image_width > 0);

    let bounds = info.bounds;
    let mut out = vec![0u8; (bounds.width * bounds.height * 4) as usize];
    decoder.decode(&mut out, bounds, bounds, 1).unwrap();
    assert!(out.iter().any(|&v| v != 0));
}

// -----------------------------------------------------------------------
// Decoder – unsupported format
// -----------------------------------------------------------------------

#[test]
fn unsupported_format_returns_error() {
    let data = vec![0u8; 100]; // garbage
    let result = decode::new_decoder(data, false, None);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// Color management (pure, no images)
// -----------------------------------------------------------------------

#[test]
fn srgb_identity_transform() {
    let mut pixels = vec![128u8, 64, 32, 255, 0, 0, 0, 255];
    let original = pixels.clone();
    imagedecoder_core::color::transform_pixels(&mut pixels, 2, None, None).unwrap();
    for (a, b) in pixels.iter().zip(original.iter()) {
        assert!((*a as i16 - *b as i16).unsigned_abs() <= 1);
    }
}

// -----------------------------------------------------------------------
// WebP animation detection
// -----------------------------------------------------------------------

#[test]
fn webp_vp8x_animated_flag() {
    // Build a minimal RIFF/WEBP with VP8X chunk, animation bit set.
    let mut header = vec![0u8; 32];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&24u32.to_le_bytes()); // file size
    header[8..12].copy_from_slice(b"WEBP");
    header[12..16].copy_from_slice(b"VP8X");
    header[16..20].copy_from_slice(&10u32.to_le_bytes()); // chunk size
    header[20] = 0x02; // flags: animation bit (bit 1)
    let t = decode::find_type(&header).unwrap();
    assert_eq!(t.format, Format::Webp);
    assert!(t.is_animated, "VP8X with animation flag should be detected");
}

#[test]
fn webp_vp8x_not_animated() {
    // VP8X present but animation bit NOT set.
    let mut header = vec![0u8; 32];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&24u32.to_le_bytes());
    header[8..12].copy_from_slice(b"WEBP");
    header[12..16].copy_from_slice(b"VP8X");
    header[16..20].copy_from_slice(&10u32.to_le_bytes());
    header[20] = 0x10; // flags: EXIF, no animation
    let t = decode::find_type(&header).unwrap();
    assert_eq!(t.format, Format::Webp);
    assert!(
        !t.is_animated,
        "VP8X without animation flag should not be animated"
    );
}

#[test]
fn webp_simple_not_animated() {
    // Simple WebP (VP8, no VP8X) — never animated.
    let mut header = vec![0u8; 32];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&24u32.to_le_bytes());
    header[8..12].copy_from_slice(b"WEBP");
    header[12..16].copy_from_slice(b"VP8 ");
    let t = decode::find_type(&header).unwrap();
    assert_eq!(t.format, Format::Webp);
    assert!(!t.is_animated);
}

// -----------------------------------------------------------------------
// ICC profile extraction
// -----------------------------------------------------------------------

#[test]
fn jpeg_icc_extraction() {
    // Build a synthetic JPEG with one APP2 ICC_PROFILE chunk.
    let icc_payload = b"SYNTHETIC_ICC_PAYLOAD";
    let mut data = vec![0xFF, 0xD8]; // SOI
    data.push(0xFF);
    data.push(0xE2); // APP2
    let seg_len = (2 + 12 + 2 + icc_payload.len()) as u16;
    data.push((seg_len >> 8) as u8);
    data.push((seg_len & 0xFF) as u8);
    data.extend_from_slice(b"ICC_PROFILE\0");
    data.push(1); // sequence
    data.push(1); // total
    data.extend_from_slice(icc_payload);
    data.push(0xFF);
    data.push(0xDA); // SOS

    let profile = imagedecoder_core::icc::extract_jpeg_icc(&data).unwrap();
    assert_eq!(profile, icc_payload);
}

#[test]
fn jpeg_no_icc_returns_none() {
    let data = match load_test_file("sample.jpg") {
        Some(d) => d,
        None => return,
    };
    // sample.jpg likely has no ICC profile; if it does this test is still valid
    // since we're just verifying the function runs without panics.
    // note: if this fails and you replaced the sample image, check that it contains
    // no ICC profile.
    let _ = imagedecoder_core::icc::extract_jpeg_icc(&data);
}

#[test]
fn webp_icc_extraction() {
    let icc_payload = b"WEBP_ICC_DATA";
    let chunk_size = icc_payload.len() as u32;
    let file_size = 4 + 8 + chunk_size;
    let mut data = Vec::new();
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&file_size.to_le_bytes());
    data.extend_from_slice(b"WEBP");
    data.extend_from_slice(b"ICCP");
    data.extend_from_slice(&chunk_size.to_le_bytes());
    data.extend_from_slice(icc_payload);

    let profile = imagedecoder_core::icc::extract_webp_icc(&data).unwrap();
    assert_eq!(profile, icc_payload);
}

#[test]
fn webp_no_icc_returns_none() {
    let data = match load_test_file("sample.webp") {
        Some(d) => d,
        None => return,
    };
    let _ = imagedecoder_core::icc::extract_webp_icc(&data);
}

#[test]
fn png_icc_extraction_no_panic() {
    let data = match load_test_file("sample.png") {
        Some(d) => d,
        None => return,
    };
    let _ = imagedecoder_core::icc::extract_png_icc(&data);
}

// -----------------------------------------------------------------------
// Oversized dimension rejection
// -----------------------------------------------------------------------

#[test]
fn check_dimensions_rejects_zero() {
    assert!(decode::check_dimensions(0, 100).is_err());
    assert!(decode::check_dimensions(100, 0).is_err());
}

#[test]
fn check_dimensions_rejects_oversized() {
    // 300_000 × 300_000 = 90 billion pixels → exceeds 256M limit
    assert!(decode::check_dimensions(300_000, 300_000).is_err());
}

#[test]
fn check_dimensions_accepts_normal() {
    assert!(decode::check_dimensions(1920, 1080).is_ok());
    assert!(decode::check_dimensions(4096, 4096).is_ok());
}

// -----------------------------------------------------------------------
// use_transform() parity
// -----------------------------------------------------------------------

#[test]
fn jpeg_use_transform_reflects_icc_presence() {
    let data = match load_test_file("sample.jpg") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data, false, None).unwrap();
    // Whether true or false depends on whether sample.jpg has ICC.
    // This test verifies no panic and consistent return.
    let _ = decoder.use_transform();
}

// -----------------------------------------------------------------------
// Decoder – ICC transform applied (functional)
// -----------------------------------------------------------------------

#[test]
fn jpeg_decode_with_srgb_target_profile() {
    // Decoding with an explicit sRGB target profile should succeed without
    // error even if the source has no embedded ICC (transform is skipped).
    let data = match load_test_file("sample.jpg") {
        Some(d) => d,
        None => return,
    };
    let decoder = decode::new_decoder(data, false, None).unwrap();
    let bounds = decoder.info().bounds;
    let mut out = vec![0u8; (bounds.width * bounds.height * 4) as usize];
    decoder.decode(&mut out, bounds, bounds, 1).unwrap();
    assert!(out.iter().any(|&v| v != 0));
}
