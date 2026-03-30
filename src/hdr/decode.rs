//! Radiance HDR (.hdr / RGBE) decoder.

use alloc::vec::Vec;
use enough::Stop;

use crate::error::BitmapError;

/// Parse a Radiance HDR header, returning (width, height, data_offset).
///
/// Validates the magic (`#?RADIANCE` or `#?RGBE`), skips key=value lines,
/// and parses the resolution line (`-Y <height> +X <width>`).
pub(crate) fn parse_header(data: &[u8]) -> Result<(u32, u32, usize), BitmapError> {
    if data.len() < 10 {
        return Err(BitmapError::UnexpectedEof);
    }
    if !data.starts_with(b"#?RADIANCE") && !data.starts_with(b"#?RGBE") {
        return Err(BitmapError::UnrecognizedFormat);
    }

    // Find end of header: look for the empty line separating header from resolution.
    // Header lines are terminated by \n. The empty line is \n\n.
    let mut pos = 0;
    let mut found_empty_line = false;
    while pos < data.len() {
        if let Some(nl) = memchr_newline(&data[pos..]) {
            let line_end = pos + nl;
            pos = line_end + 1; // skip past \n
            // Check if the next byte is also \n (empty line)
            if pos < data.len() && data[pos] == b'\n' {
                pos += 1;
                found_empty_line = true;
                break;
            }
            // Also handle the case where the line itself is empty
            if nl == 0 {
                found_empty_line = true;
                break;
            }
        } else {
            return Err(BitmapError::InvalidHeader(
                "HDR header: no newline found".into(),
            ));
        }
    }

    if !found_empty_line {
        return Err(BitmapError::InvalidHeader(
            "HDR header: missing empty line separator".into(),
        ));
    }

    // Now parse the resolution line: `-Y <height> +X <width>\n`
    let remaining = &data[pos..];
    let nl = memchr_newline(remaining)
        .ok_or_else(|| BitmapError::InvalidHeader("HDR: missing resolution line".into()))?;
    let res_line = core::str::from_utf8(&remaining[..nl])
        .map_err(|_| BitmapError::InvalidHeader("HDR: resolution line not UTF-8".into()))?;
    let res_offset = pos + nl + 1; // byte after the resolution line's \n

    let (width, height) = parse_resolution(res_line)?;

    if width == 0 || height == 0 {
        return Err(BitmapError::InvalidHeader(
            "HDR: width or height is zero".into(),
        ));
    }

    Ok((width, height, res_offset))
}

/// Parse a resolution string like `-Y 600 +X 800`.
fn parse_resolution(s: &str) -> Result<(u32, u32), BitmapError> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 4 {
        return Err(BitmapError::InvalidHeader(alloc::format!(
            "HDR: expected 4 tokens in resolution line, got {}: '{s}'",
            parts.len()
        )));
    }
    // Standard orientation: -Y height +X width
    if parts[0] == "-Y" && parts[2] == "+X" {
        let height: u32 = parts[1].parse().map_err(|_| {
            BitmapError::InvalidHeader(alloc::format!("HDR: invalid height '{}'", parts[1]))
        })?;
        let width: u32 = parts[3].parse().map_err(|_| {
            BitmapError::InvalidHeader(alloc::format!("HDR: invalid width '{}'", parts[3]))
        })?;
        Ok((width, height))
    } else {
        Err(BitmapError::UnsupportedVariant(alloc::format!(
            "HDR: unsupported orientation '{} {}'",
            parts[0],
            parts[2]
        )))
    }
}

/// Decode RGBE pixel data to f32 RGB bytes.
///
/// Returns a `Vec<u8>` containing `width * height * 12` bytes (3 f32 per pixel).
pub(crate) fn decode_pixels(
    data: &[u8],
    offset: usize,
    width: u32,
    height: u32,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let w = width as usize;
    let h = height as usize;
    let out_bytes = w
        .checked_mul(h)
        .and_then(|px| px.checked_mul(12)) // 3 channels × 4 bytes per f32
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;

    let mut out = alloc::vec![0u8; out_bytes];
    let mut out_pos = 0;
    let mut pos = offset;

    // Scanline buffer for new-style RLE (4 channels × width)
    let mut scanline_buf = alloc::vec![0u8; w * 4];

    for row in 0..h {
        if row % 16 == 0 {
            stop.check()?;
        }

        if pos + 4 > data.len() {
            return Err(BitmapError::UnexpectedEof);
        }

        // Check for new-style RLE marker
        if (8..=0x7FFF).contains(&w) && data[pos] == 2 && data[pos + 1] == 2 && data[pos + 2] < 128
        {
            // New-style RLE scanline
            let encoded_width = ((data[pos + 2] as usize) << 8) | (data[pos + 3] as usize);
            if encoded_width != w {
                return Err(BitmapError::InvalidData(alloc::format!(
                    "HDR RLE: scanline width mismatch (expected {w}, got {encoded_width})"
                )));
            }
            pos += 4;

            // Decode 4 channel runs (R, G, B, E)
            for ch in 0..4 {
                let mut col = 0;
                while col < w {
                    if pos >= data.len() {
                        return Err(BitmapError::UnexpectedEof);
                    }
                    let code = data[pos];
                    pos += 1;

                    if code > 128 {
                        // Run: (code - 128) copies of next byte
                        let count = (code - 128) as usize;
                        if col + count > w {
                            return Err(BitmapError::InvalidData(
                                "HDR RLE: run overflows scanline".into(),
                            ));
                        }
                        if pos >= data.len() {
                            return Err(BitmapError::UnexpectedEof);
                        }
                        let val = data[pos];
                        pos += 1;
                        for i in 0..count {
                            scanline_buf[(col + i) * 4 + ch] = val;
                        }
                        col += count;
                    } else {
                        // Literal: `code` distinct values
                        let count = code as usize;
                        if count == 0 {
                            return Err(BitmapError::InvalidData(
                                "HDR RLE: zero-length literal run".into(),
                            ));
                        }
                        if col + count > w {
                            return Err(BitmapError::InvalidData(
                                "HDR RLE: literal overflows scanline".into(),
                            ));
                        }
                        if pos + count > data.len() {
                            return Err(BitmapError::UnexpectedEof);
                        }
                        for i in 0..count {
                            scanline_buf[(col + i) * 4 + ch] = data[pos + i];
                        }
                        pos += count;
                        col += count;
                    }
                }
            }

            // Convert RGBE to f32 and write directly into output
            let row_out = &mut out[out_pos..out_pos + w * 12];
            rgbe_deinterleaved_to_f32(&scanline_buf[..w * 4], w, row_out);
            out_pos += w * 12;
        } else {
            // Uncompressed: read flat RGBE quads
            let needed = w * 4;
            if pos + needed > data.len() {
                return Err(BitmapError::UnexpectedEof);
            }
            let row_out = &mut out[out_pos..out_pos + w * 12];
            rgbe_scanline_to_f32(&data[pos..pos + needed], row_out);
            out_pos += w * 12;
            pos += needed;
        }
    }

    Ok(out)
}

/// Convert RGBE (4 bytes) to 3×f32 linear RGB.
///
/// Uses bit manipulation to compute `2^(e - 136)` without libm or unsafe.
pub(crate) fn rgbe_to_f32(r: u8, g: u8, b: u8, e: u8) -> (f32, f32, f32) {
    if e == 0 {
        return (0.0, 0.0, 0.0);
    }
    // ldexp(1.0, e - 128 - 8) = 2^(e - 136)
    // Construct the float via bit manipulation of the IEEE 754 exponent field.
    let exp_bits = ((e as u32).wrapping_add(127).wrapping_sub(136)) << 23;
    let scale = f32::from_bits(exp_bits);
    (
        (r as f32 + 0.5) * scale,
        (g as f32 + 0.5) * scale,
        (b as f32 + 0.5) * scale,
    )
}

/// Batch convert RGBE scanline to f32 RGB, writing directly into output buffer.
///
/// `rgbe` is interleaved [R,G,B,E, R,G,B,E, ...] data (4 bytes per pixel).
/// `out` must be exactly `pixel_count * 12` bytes. Writes 3×f32 per pixel.
#[inline]
pub(crate) fn rgbe_scanline_to_f32(rgbe: &[u8], out: &mut [u8]) {
    debug_assert_eq!(rgbe.len() % 4, 0);
    let pixel_count = rgbe.len() / 4;
    debug_assert_eq!(out.len(), pixel_count * 12);

    let mut out_pos = 0;
    for px in rgbe.chunks_exact(4) {
        let (rf, gf, bf) = rgbe_to_f32(px[0], px[1], px[2], px[3]);
        out[out_pos..out_pos + 4].copy_from_slice(&rf.to_le_bytes());
        out[out_pos + 4..out_pos + 8].copy_from_slice(&gf.to_le_bytes());
        out[out_pos + 8..out_pos + 12].copy_from_slice(&bf.to_le_bytes());
        out_pos += 12;
    }
}

/// Batch convert deinterleaved RGBE channels to f32 RGB.
///
/// `scanline_buf` is channel-interleaved [R0,G0,B0,E0, R1,G1,B1,E1, ...] (4 bytes per pixel).
/// `out` must be exactly `pixel_count * 12` bytes.
#[inline]
pub(crate) fn rgbe_deinterleaved_to_f32(scanline_buf: &[u8], width: usize, out: &mut [u8]) {
    debug_assert_eq!(scanline_buf.len(), width * 4);
    debug_assert_eq!(out.len(), width * 12);

    let mut out_pos = 0;
    for px in 0..width {
        let base = px * 4;
        let (rf, gf, bf) = rgbe_to_f32(
            scanline_buf[base],
            scanline_buf[base + 1],
            scanline_buf[base + 2],
            scanline_buf[base + 3],
        );
        out[out_pos..out_pos + 4].copy_from_slice(&rf.to_le_bytes());
        out[out_pos + 4..out_pos + 8].copy_from_slice(&gf.to_le_bytes());
        out[out_pos + 8..out_pos + 12].copy_from_slice(&bf.to_le_bytes());
        out_pos += 12;
    }
}

/// Find the position of the first `\n` in `data`, or `None`.
fn memchr_newline(data: &[u8]) -> Option<usize> {
    data.iter().position(|&b| b == b'\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgbe_to_f32_zero_exponent() {
        let (r, g, b) = rgbe_to_f32(128, 128, 128, 0);
        assert_eq!(r, 0.0);
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
    }

    #[test]
    fn rgbe_to_f32_known_value() {
        // e=128+8=136 → scale = 2^0 = 1.0
        // With mantissa 128: (128+0.5)*1.0 = 128.5
        let (r, _g, _b) = rgbe_to_f32(128, 0, 0, 136);
        assert!((r - 128.5).abs() < 0.01);
    }

    #[test]
    fn rgbe_to_f32_unit_range() {
        // e=128 → scale = 2^(128-136) = 2^(-8) = 1/256
        // With mantissa 128: (128+0.5)/256 ≈ 0.502
        let (r, _g, _b) = rgbe_to_f32(128, 0, 0, 128);
        assert!((r - 0.502).abs() < 0.01, "got {r}");
    }

    #[test]
    fn parse_resolution_standard() {
        let (w, h) = parse_resolution("-Y 600 +X 800").unwrap();
        assert_eq!(w, 800);
        assert_eq!(h, 600);
    }

    #[test]
    fn parse_resolution_bad() {
        assert!(parse_resolution("+Y 600 +X 800").is_err());
        assert!(parse_resolution("-Y abc +X 800").is_err());
        assert!(parse_resolution("-Y 600").is_err());
    }

    #[test]
    fn parse_header_minimal() {
        let mut hdr = Vec::new();
        hdr.extend_from_slice(b"#?RADIANCE\n");
        hdr.extend_from_slice(b"FORMAT=32-bit_rle_rgbe\n");
        hdr.extend_from_slice(b"\n");
        hdr.extend_from_slice(b"-Y 2 +X 3\n");
        let (w, h, offset) = parse_header(&hdr).unwrap();
        assert_eq!(w, 3);
        assert_eq!(h, 2);
        assert_eq!(offset, hdr.len());
    }

    #[test]
    fn parse_header_rgbe_magic() {
        let mut hdr = Vec::new();
        hdr.extend_from_slice(b"#?RGBE\n");
        hdr.extend_from_slice(b"\n");
        hdr.extend_from_slice(b"-Y 1 +X 1\n");
        let (w, h, _) = parse_header(&hdr).unwrap();
        assert_eq!(w, 1);
        assert_eq!(h, 1);
    }
}
