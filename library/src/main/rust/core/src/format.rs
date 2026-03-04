use crate::types::{Format, ImageType};
use std::cmp::min;

/// Detect the [`ImageType`] from the first bytes of a file.
///
/// Requires at least 32 bytes.
pub fn detect(header: &[u8]) -> Option<ImageType> {
    if header.len() < 12 {
        return None;
    }

    if is_jpeg(header) {
        return Some(ImageType {
            format: Format::Jpeg,
            is_animated: false,
        });
    }
    if is_png(header) {
        return Some(ImageType {
            format: Format::Png,
            is_animated: false,
        });
    }
    if is_webp(header) {
        let is_animated = webp_is_animated(header);
        return Some(ImageType {
            format: Format::Webp,
            is_animated,
        });
    }
    if is_gif(header) {
        return Some(ImageType {
            format: Format::Gif,
            is_animated: true,
        });
    }
    if is_jxl(header) {
        return Some(ImageType {
            format: Format::Jxl,
            is_animated: false,
        });
    }

    match get_ftyp_image_type(header) {
        FtypType::Heif => Some(ImageType {
            format: Format::Heif,
            is_animated: false,
        }),
        FtypType::Avif => Some(ImageType {
            format: Format::Avif,
            is_animated: false,
        }),
        FtypType::No => None,
    }
}

/// Detect the [`Format`] from the full file data (uses first 32 bytes).
pub fn detect_format(data: &[u8]) -> Option<Format> {
    detect(data).map(|t| t.format)
}

// ---------------------------------------------------------------------------
// Magic-number checks matching the C++ decoder_headers.h
// ---------------------------------------------------------------------------

fn is_jpeg(data: &[u8]) -> bool {
    data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF
}

fn is_png(data: &[u8]) -> bool {
    data.len() >= 4 && data[0] == 0x89 && data[1] == b'P' && data[2] == b'N' && data[3] == b'G'
}

fn is_webp(data: &[u8]) -> bool {
    data.len() >= 4 && data[0] == b'R' && data[1] == b'I' && data[2] == b'F' && data[3] == b'F'
}

/// Check if a WebP file is animated by inspecting the VP8X chunk.
///
/// The VP8X extended header (if present) starts at byte 12 of the RIFF
/// container: `"VP8X"` fourcc + 4-byte little-endian size + flags byte.
/// Bit 1 of the flags byte indicates animation.
fn webp_is_animated(data: &[u8]) -> bool {
    // Need at least: RIFF(4) + size(4) + WEBP(4) + VP8X(4) + size(4) + flags(1) = 21 bytes
    if data.len() < 21 {
        return false;
    }
    // First sub-chunk must be VP8X for the animation flag to exist.
    if &data[12..16] != b"VP8X" {
        return false;
    }
    // Flags byte is at offset 20 (after VP8X fourcc + 4-byte chunk size).
    // Bit 1 (0x02) = animation flag.
    data[20] & 0x02 != 0
}

fn is_gif(data: &[u8]) -> bool {
    data.len() >= 4 && data[0] == b'G' && data[1] == b'I' && data[2] == b'F' && data[3] == b'8'
}

fn is_jxl(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    // Container format
    let container = data[0] == 0x00
        && data[1] == 0x00
        && data[2] == 0x00
        && data[3] == 0x0C
        && data[4] == b'J'
        && data[5] == b'X'
        && data[6] == b'L'
        && data[7] == b' '
        && data[8] == 0x0D
        && data[9] == 0x0A
        && data[10] == 0x87
        && data[11] == 0x0A;
    // Bare codestream
    let codestream = data[0] == 0xFF && data[1] == 0x0A;
    container || codestream
}

#[derive(Debug, PartialEq)]
enum FtypType {
    No,
    Heif,
    Avif,
}

fn get_ftyp_image_type(data: &[u8]) -> FtypType {
    if data.len() < 12 {
        return FtypType::No;
    }
    if data[4] != b'f' || data[5] != b't' || data[6] != b'y' || data[7] != b'p' {
        return FtypType::No;
    }

    let header_size =
        (data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32;
    let max_offset = (min(header_size, data.len() as u32) as usize).saturating_sub(4);
    let mut offset = 8usize;
    while offset <= max_offset {
        if offset + 3 >= data.len() {
            break;
        }
        let brand = &data[offset..];
        if brand[0] == b'h' && brand[1] == b'e' && (brand[2] == b'i' || brand[2] == b'v') {
            return FtypType::Heif;
        } else if brand[0] == b'a' && brand[1] == b'v' && brand[2] == b'i' {
            return FtypType::Avif;
        }
        offset += 4;
    }

    FtypType::No
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_jpeg() {
        let mut header = vec![0u8; 32];
        header[0] = 0xFF;
        header[1] = 0xD8;
        header[2] = 0xFF;
        let t = detect(&header).unwrap();
        assert_eq!(t.format, Format::Jpeg);
        assert!(!t.is_animated);
    }

    #[test]
    fn detect_png() {
        let mut header = vec![0u8; 32];
        header[0] = 0x89;
        header[1] = b'P';
        header[2] = b'N';
        header[3] = b'G';
        let t = detect(&header).unwrap();
        assert_eq!(t.format, Format::Png);
    }

    #[test]
    fn detect_webp() {
        let mut header = vec![0u8; 32];
        header[0] = b'R';
        header[1] = b'I';
        header[2] = b'F';
        header[3] = b'F';
        let t = detect(&header).unwrap();
        assert_eq!(t.format, Format::Webp);
    }

    #[test]
    fn detect_gif() {
        let mut header = vec![0u8; 32];
        header[0] = b'G';
        header[1] = b'I';
        header[2] = b'F';
        header[3] = b'8';
        let t = detect(&header).unwrap();
        assert_eq!(t.format, Format::Gif);
        assert!(t.is_animated);
    }

    #[test]
    fn detect_jxl_container() {
        let header: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let t = detect(&header).unwrap();
        assert_eq!(t.format, Format::Jxl);
    }

    #[test]
    fn detect_jxl_codestream() {
        let mut header = vec![0u8; 32];
        header[0] = 0xFF;
        header[1] = 0x0A;
        let t = detect(&header).unwrap();
        assert_eq!(t.format, Format::Jxl);
    }

    #[test]
    fn detect_avif() {
        // Construct a minimal ftyp box with 'avis' brand.
        let mut header = vec![0u8; 32];
        // header size = 20 (big-endian)
        header[0] = 0;
        header[1] = 0;
        header[2] = 0;
        header[3] = 20;
        // 'ftyp'
        header[4] = b'f';
        header[5] = b't';
        header[6] = b'y';
        header[7] = b'p';
        // major brand 'avis'
        header[8] = b'a';
        header[9] = b'v';
        header[10] = b'i';
        header[11] = b's';
        let t = detect(&header).unwrap();
        assert_eq!(t.format, Format::Avif);
    }

    #[test]
    fn detect_heif() {
        let mut header = vec![0u8; 32];
        header[0] = 0;
        header[1] = 0;
        header[2] = 0;
        header[3] = 20;
        header[4] = b'f';
        header[5] = b't';
        header[6] = b'y';
        header[7] = b'p';
        // major brand 'heic'
        header[8] = b'h';
        header[9] = b'e';
        header[10] = b'i';
        header[11] = b'c';
        let t = detect(&header).unwrap();
        assert_eq!(t.format, Format::Heif);
    }

    #[test]
    fn detect_unknown() {
        let header = vec![0u8; 32];
        assert!(detect(&header).is_none());
    }

    #[test]
    fn detect_too_short() {
        let header = vec![0xFF, 0xD8];
        assert!(detect(&header).is_none());
    }
}
