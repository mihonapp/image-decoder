//! ICC profile extraction helpers for each image format.
//!
//! These routines parse format-specific containers to locate the embedded ICC
//! colour profile (if any) and return it as raw bytes suitable for feeding to
//! lcms2.

/// Extract the ICC profile embedded in a JPEG file.
///
/// The profile is stored across one or more APP2 markers whose payload starts
/// with the ASCII string `"ICC_PROFILE\0"` followed by a 1-based sequence
/// number and a total-chunk count. This function reassembles the chunks in
/// order and returns the concatenated profile bytes.
pub fn extract_jpeg_icc(data: &[u8]) -> Option<Vec<u8>> {
    const ICC_MARKER: &[u8; 12] = b"ICC_PROFILE\0";

    // Collect (sequence_number, chunk_data) pairs.
    let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut pos = 2; // skip SOI (0xFF 0xD8)

    while pos + 1 < data.len() {
        if data[pos] != 0xFF {
            break;
        }

        let marker = data[pos + 1];

        // SOS (0xDA) means we've reached compressed data — stop scanning.
        if marker == 0xDA {
            break;
        }

        // Markers without a length payload (RST, SOI, EOI, TEM).
        if marker == 0x00
            || marker == 0x01
            || marker == 0xD8
            || marker == 0xD9
            || (0xD0..=0xD7).contains(&marker)
        {
            pos += 2;
            continue;
        }

        if pos + 3 >= data.len() {
            break;
        }

        let seg_len = ((data[pos + 2] as usize) << 8) | data[pos + 3] as usize;
        if seg_len < 2 {
            break;
        }

        let payload_start = pos + 4; // after marker + length
        let payload_len = seg_len - 2;
        let seg_end = pos + 2 + seg_len;

        if seg_end > data.len() {
            break;
        }

        // APP2 = 0xE2
        if marker == 0xE2 && payload_len > 14 {
            if &data[payload_start..payload_start + 12] == ICC_MARKER {
                let seq = data[payload_start + 12];
                // data[payload_start + 13] = total count (we derive it from max seq)
                let chunk = data[payload_start + 14..seg_end].to_vec();
                chunks.push((seq, chunk));
            }
        }

        pos = seg_end;
    }

    if chunks.is_empty() {
        return None;
    }

    chunks.sort_by_key(|(seq, _)| *seq);

    let total_len: usize = chunks.iter().map(|(_, c)| c.len()).sum();
    let mut profile = Vec::with_capacity(total_len);
    for (_, chunk) in &chunks {
        profile.extend_from_slice(chunk);
    }

    Some(profile)
}

/// Extract the ICC profile embedded in a WebP file.
///
/// WebP extended format (VP8X) stores the profile in an `"ICCP"` RIFF chunk
/// that follows the VP8X header. This function scans the RIFF chunks for it.
pub fn extract_webp_icc(data: &[u8]) -> Option<Vec<u8>> {
    // Minimum: "RIFF" (4) + size (4) + "WEBP" (4) + chunk header (8) = 20
    if data.len() < 20 {
        return None;
    }

    if &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }

    let file_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let end = (file_size + 8).min(data.len());

    let mut pos = 12; // start of first RIFF sub-chunk

    while pos + 8 <= end {
        let fourcc = &data[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
                as usize;
        let chunk_data_start = pos + 8;
        let chunk_data_end = (chunk_data_start + chunk_size).min(end);

        if fourcc == b"ICCP" && chunk_size > 0 && chunk_data_end <= end {
            return Some(data[chunk_data_start..chunk_data_end].to_vec());
        }

        // RIFF chunks are padded to even byte boundaries.
        pos = chunk_data_start + chunk_size + (chunk_size & 1);
    }

    None
}

/// Extract the ICC profile from a PNG file using the `png` crate's metadata.
pub fn extract_png_icc(data: &[u8]) -> Option<Vec<u8>> {
    let decoder = png::Decoder::new(std::io::Cursor::new(data));
    let reader = decoder.read_info().ok()?;
    reader.info().icc_profile.as_ref().map(|cow| cow.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_no_icc() {
        // Minimal JPEG without APP2 markers
        let data = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08,
        ];
        assert!(extract_jpeg_icc(&data).is_none());
    }

    #[test]
    fn jpeg_single_chunk_icc() {
        // Build a synthetic APP2 with a small ICC payload
        let mut data = vec![0xFF, 0xD8]; // SOI
        let icc_payload = b"FAKE_ICC_DATA_HERE";
        // APP2 marker
        data.push(0xFF);
        data.push(0xE2);
        // Length = 2 + 12 (identifier) + 2 (seq+count) + icc_payload.len()
        let seg_len = (2 + 12 + 2 + icc_payload.len()) as u16;
        data.push((seg_len >> 8) as u8);
        data.push((seg_len & 0xFF) as u8);
        data.extend_from_slice(b"ICC_PROFILE\0");
        data.push(1); // sequence number
        data.push(1); // total count
        data.extend_from_slice(icc_payload);
        // SOS to end scanning
        data.push(0xFF);
        data.push(0xDA);
        data.push(0x00);
        data.push(0x08);

        let profile = extract_jpeg_icc(&data).unwrap();
        assert_eq!(profile, icc_payload);
    }

    #[test]
    fn webp_no_iccp() {
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(b"RIFF");
        data[4..8].copy_from_slice(&24u32.to_le_bytes());
        data[8..12].copy_from_slice(b"WEBP");
        data[12..16].copy_from_slice(b"VP8 ");
        data[16..20].copy_from_slice(&12u32.to_le_bytes());
        assert!(extract_webp_icc(&data).is_none());
    }

    #[test]
    fn webp_with_iccp() {
        let icc_payload = b"FAKE_ICC";
        let iccp_chunk_size = icc_payload.len() as u32;
        let file_size = 4 + 8 + iccp_chunk_size; // "WEBP" + chunk header + data
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&file_size.to_le_bytes());
        data.extend_from_slice(b"WEBP");
        data.extend_from_slice(b"ICCP");
        data.extend_from_slice(&iccp_chunk_size.to_le_bytes());
        data.extend_from_slice(icc_payload);
        let profile = extract_webp_icc(&data).unwrap();
        assert_eq!(profile, icc_payload);
    }
}
