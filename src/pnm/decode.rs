//! PNM decoder: P1-P7, PFM.
//!
//! P1 (ASCII PBM), P2 (ASCII PGM), P3 (ASCII PPM) — text pixel data.
//! P4 (binary PBM) — bit-packed, 8 pixels per byte, MSB first.
//! P5 (binary PGM), P6 (binary PPM) — raw binary pixel data.
//! P7 (PAM) — arbitrary channels, binary. PFM — float, binary.
//!
//! Credits: Draws from zune-ppm by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use super::PnmHeader;
use crate::error::BitmapError;
use crate::pixel::PixelLayout;
use crate::pnm::PnmFormat;
use alloc::string::String;
use alloc::vec::Vec;
use enough::Stop;

/// Parse header from raw data.
pub(crate) fn parse_header(data: &[u8]) -> Result<PnmHeader, BitmapError> {
    if data.len() < 3 {
        return Err(BitmapError::UnexpectedEof);
    }

    match &data[..2] {
        b"P5" => parse_p5_p6_header(data, PnmFormat::Pgm),
        b"P6" => parse_p5_p6_header(data, PnmFormat::Ppm),
        b"P7" => parse_p7_header(data),
        b"Pf" | b"PF" => parse_pfm_header(data),
        b"P1" | b"P4" => parse_pbm_header(data),
        b"P2" => parse_p5_p6_header(data, PnmFormat::Pgm),
        b"P3" => parse_p5_p6_header(data, PnmFormat::Ppm),
        _ => Err(BitmapError::UnrecognizedFormat),
    }
}

fn parse_p5_p6_header(data: &[u8], format: PnmFormat) -> Result<PnmHeader, BitmapError> {
    let mut pos = 2;

    pos = skip_whitespace_and_comments(data, pos)?;
    let (width, new_pos) = parse_u32(data, pos)?;
    pos = skip_whitespace_and_comments(data, new_pos)?;
    let (height, new_pos) = parse_u32(data, pos)?;
    pos = skip_whitespace_and_comments(data, new_pos)?;
    let (maxval, new_pos) = parse_u32(data, pos)?;

    if width == 0 || height == 0 {
        return Err(BitmapError::InvalidHeader(
            "width and height must be non-zero".into(),
        ));
    }
    if maxval == 0 || maxval > 65535 {
        return Err(BitmapError::InvalidHeader(alloc::format!(
            "maxval must be 1-65535, got {maxval}"
        )));
    }

    if new_pos >= data.len() {
        return Err(BitmapError::UnexpectedEof);
    }
    let data_offset = new_pos + 1;

    let (depth, layout) = match format {
        PnmFormat::Pgm => {
            if maxval <= 255 {
                (1, PixelLayout::Gray8)
            } else {
                (1, PixelLayout::Gray16)
            }
        }
        PnmFormat::Ppm => (3, PixelLayout::Rgb8),
        _ => {
            return Err(BitmapError::UnsupportedVariant(alloc::format!(
                "unexpected format {:?} in P5/P6 parser",
                format
            )));
        }
    };

    Ok(PnmHeader {
        format,
        width,
        height,
        maxval,
        depth,
        layout,
        pfm_scale: 0.0,
        data_offset,
    })
}

/// Parse P1/P4 (PBM) header. PBM has width and height but no maxval.
fn parse_pbm_header(data: &[u8]) -> Result<PnmHeader, BitmapError> {
    let mut pos = 2;

    pos = skip_whitespace_and_comments(data, pos)?;
    let (width, new_pos) = parse_u32(data, pos)?;
    pos = skip_whitespace_and_comments(data, new_pos)?;
    let (height, new_pos) = parse_u32(data, pos)?;

    if width == 0 || height == 0 {
        return Err(BitmapError::InvalidHeader(
            "width and height must be non-zero".into(),
        ));
    }

    // P1: single whitespace separates header from ASCII data
    // P4: single whitespace byte separates header from binary data
    if new_pos >= data.len() {
        return Err(BitmapError::UnexpectedEof);
    }
    let data_offset = new_pos + 1;

    Ok(PnmHeader {
        format: PnmFormat::Pbm,
        width,
        height,
        maxval: 1,
        depth: 1,
        layout: PixelLayout::Gray8,
        pfm_scale: 0.0,
        data_offset,
    })
}

fn parse_p7_header(data: &[u8]) -> Result<PnmHeader, BitmapError> {
    let mut pos = 2;
    pos = skip_whitespace_and_comments(data, pos)?;

    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut depth: Option<u32> = None;
    let mut maxval: Option<u32> = None;
    let mut tupltype: Option<String> = None;

    loop {
        let line_end = data[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| pos + i)
            .unwrap_or(data.len());
        let line = core::str::from_utf8(&data[pos..line_end])
            .map_err(|_| BitmapError::InvalidHeader("non-UTF8 in PAM header".into()))?
            .trim();

        if line == "ENDHDR" {
            pos = line_end + 1;
            break;
        }

        if let Some(rest) = line.strip_prefix("WIDTH ") {
            width = Some(
                rest.trim()
                    .parse()
                    .map_err(|_| BitmapError::InvalidHeader("bad WIDTH".into()))?,
            );
        } else if let Some(rest) = line.strip_prefix("HEIGHT ") {
            height = Some(
                rest.trim()
                    .parse()
                    .map_err(|_| BitmapError::InvalidHeader("bad HEIGHT".into()))?,
            );
        } else if let Some(rest) = line.strip_prefix("DEPTH ") {
            depth = Some(
                rest.trim()
                    .parse()
                    .map_err(|_| BitmapError::InvalidHeader("bad DEPTH".into()))?,
            );
        } else if let Some(rest) = line.strip_prefix("MAXVAL ") {
            maxval = Some(
                rest.trim()
                    .parse()
                    .map_err(|_| BitmapError::InvalidHeader("bad MAXVAL".into()))?,
            );
        } else if let Some(rest) = line.strip_prefix("TUPLTYPE ") {
            tupltype = Some(rest.trim().into());
        } else if line.starts_with('#') {
            // comment, skip
        }

        pos = if line_end < data.len() {
            line_end + 1
        } else {
            data.len()
        };
        if pos >= data.len() {
            return Err(BitmapError::InvalidHeader("no ENDHDR found".into()));
        }
    }

    let width = width.ok_or_else(|| BitmapError::InvalidHeader("missing WIDTH".into()))?;
    let height = height.ok_or_else(|| BitmapError::InvalidHeader("missing HEIGHT".into()))?;
    let depth = depth.ok_or_else(|| BitmapError::InvalidHeader("missing DEPTH".into()))?;
    let maxval = maxval.ok_or_else(|| BitmapError::InvalidHeader("missing MAXVAL".into()))?;

    if width == 0 || height == 0 {
        return Err(BitmapError::InvalidHeader(
            "width and height must be non-zero".into(),
        ));
    }
    if depth == 0 {
        return Err(BitmapError::InvalidHeader("DEPTH must be non-zero".into()));
    }

    let layout = match (depth, maxval > 255) {
        (1, false) => PixelLayout::Gray8,
        (1, true) => PixelLayout::Gray16,
        (3, false) => PixelLayout::Rgb8,
        (3, true) => PixelLayout::Rgb8,
        (4, false) => PixelLayout::Rgba8,
        (4, true) => PixelLayout::Rgba8,
        _ => {
            return Err(BitmapError::UnsupportedVariant(alloc::format!(
                "PAM DEPTH={depth} not supported"
            )));
        }
    };

    let _ = tupltype;

    Ok(PnmHeader {
        format: PnmFormat::Pam,
        width,
        height,
        maxval,
        depth,
        layout,
        pfm_scale: 0.0,
        data_offset: pos,
    })
}

fn parse_pfm_header(data: &[u8]) -> Result<PnmHeader, BitmapError> {
    let is_color = data[1] == b'F';
    let mut pos = 2;

    pos = skip_whitespace_and_comments(data, pos)?;
    let (width, new_pos) = parse_u32(data, pos)?;
    pos = skip_whitespace_and_comments(data, new_pos)?;
    let (height, new_pos) = parse_u32(data, pos)?;
    pos = skip_whitespace_and_comments(data, new_pos)?;

    let line_end = data[pos..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| pos + i)
        .unwrap_or(data.len());
    let scale_str = core::str::from_utf8(&data[pos..line_end])
        .map_err(|_| BitmapError::InvalidHeader("non-UTF8 scale".into()))?
        .trim();
    let scale: f32 = scale_str
        .parse()
        .map_err(|_| BitmapError::InvalidHeader(alloc::format!("bad scale: {scale_str}")))?;

    if width == 0 || height == 0 {
        return Err(BitmapError::InvalidHeader(
            "width and height must be non-zero".into(),
        ));
    }

    let data_offset = line_end + 1;

    let (depth, layout) = if is_color {
        (3, PixelLayout::RgbF32)
    } else {
        (1, PixelLayout::GrayF32)
    };

    Ok(PnmHeader {
        format: PnmFormat::Pfm,
        width,
        height,
        maxval: 0,
        depth,
        layout,
        pfm_scale: scale,
        data_offset,
    })
}

/// Decode integer data that needs transformation (non-255 maxval or 16-bit).
pub(crate) fn decode_integer_transform(
    pixel_data: &[u8],
    header: &PnmHeader,
    expected_src: usize,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let w = header.width as usize;
    let h = header.height as usize;
    let depth = header.depth as usize;
    let is_16bit = header.maxval > 255;

    if !is_16bit {
        // Scale from maxval to 255
        let scale = 255.0 / header.maxval as f32;
        let mut out = Vec::with_capacity(expected_src);
        let stop_interval = w.saturating_mul(depth).saturating_mul(16).max(1);
        for (i, &b) in pixel_data[..expected_src].iter().enumerate() {
            if i % stop_interval == 0 {
                stop.check()?;
            }
            out.push((b as f32 * scale + 0.5) as u8);
        }
        Ok(out)
    } else {
        match header.layout {
            PixelLayout::Gray16 => Ok(pixel_data[..expected_src].to_vec()),
            _ => {
                let num_samples = w
                    .checked_mul(h)
                    .and_then(|wh| wh.checked_mul(depth))
                    .ok_or(BitmapError::DimensionsTooLarge {
                        width: header.width,
                        height: header.height,
                    })?;
                // Verify 2*num_samples fits and data is sufficient
                let needed_bytes =
                    num_samples
                        .checked_mul(2)
                        .ok_or(BitmapError::DimensionsTooLarge {
                            width: header.width,
                            height: header.height,
                        })?;
                if pixel_data.len() < needed_bytes {
                    return Err(BitmapError::UnexpectedEof);
                }
                let scale = 255.0 / header.maxval as f32;
                let stop_interval = w.saturating_mul(depth).saturating_mul(16).max(1);
                let mut out = Vec::with_capacity(num_samples);
                for i in 0..num_samples {
                    if i % stop_interval == 0 {
                        stop.check()?;
                    }
                    let hi = pixel_data[i * 2] as u16;
                    let lo = pixel_data[i * 2 + 1] as u16;
                    let val = (hi << 8) | lo;
                    out.push((val as f32 * scale + 0.5) as u8);
                }
                Ok(out)
            }
        }
    }
}

/// Decode PFM float pixel data.
pub(crate) fn decode_pfm(
    pixel_data: &[u8],
    header: &PnmHeader,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let w = header.width as usize;
    let h = header.height as usize;
    let depth = header.depth as usize;
    let num_floats = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(depth))
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })?;
    let expected_bytes = num_floats
        .checked_mul(4)
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })?;

    if pixel_data.len() < expected_bytes {
        return Err(BitmapError::UnexpectedEof);
    }

    let is_little_endian = header.pfm_scale < 0.0;
    let scale = header.pfm_scale.abs();

    let mut out = Vec::with_capacity(expected_bytes);
    let row_floats = w
        .checked_mul(depth)
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })?;
    let row_bytes = row_floats
        .checked_mul(4)
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })?;

    // PFM stores rows bottom-to-top
    for row in (0..h).rev() {
        if row % 16 == 0 {
            stop.check()?;
        }
        let row_start = row * row_bytes;
        for i in 0..row_floats {
            let offset = row_start + i * 4;
            let raw = if is_little_endian {
                f32::from_le_bytes([
                    pixel_data[offset],
                    pixel_data[offset + 1],
                    pixel_data[offset + 2],
                    pixel_data[offset + 3],
                ])
            } else {
                f32::from_be_bytes([
                    pixel_data[offset],
                    pixel_data[offset + 1],
                    pixel_data[offset + 2],
                    pixel_data[offset + 3],
                ])
            };
            let val = raw * scale;
            out.extend_from_slice(&val.to_ne_bytes());
        }
    }

    Ok(out)
}

/// Decode ASCII PBM (P1): `0` = white (255), `1` = black (0).
pub(crate) fn decode_ascii_pbm(
    pixel_data: &[u8],
    header: &PnmHeader,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let total = (header.width as usize)
        .checked_mul(header.height as usize)
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })?;

    let mut out = Vec::with_capacity(total);
    let mut pos = 0;

    for i in 0..total {
        if i % (header.width as usize * 16) == 0 {
            stop.check()?;
        }
        // Skip whitespace and comments
        while pos < pixel_data.len() {
            match pixel_data[pos] {
                b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
                b'#' => {
                    while pos < pixel_data.len() && pixel_data[pos] != b'\n' {
                        pos += 1;
                    }
                }
                _ => break,
            }
        }
        if pos >= pixel_data.len() {
            return Err(BitmapError::UnexpectedEof);
        }
        // PBM: 1 = black (0), 0 = white (255)
        let val = match pixel_data[pos] {
            b'0' => 255,
            b'1' => 0,
            c => {
                return Err(BitmapError::InvalidData(alloc::format!(
                    "P1: expected '0' or '1', got '{}'",
                    c as char
                )));
            }
        };
        out.push(val);
        pos += 1;
    }

    Ok(out)
}

/// Decode binary PBM (P4): 8 pixels per byte, MSB first.
/// 1 = black (0), 0 = white (255). Rows are padded to byte boundaries.
pub(crate) fn decode_p4_bitpacked(
    pixel_data: &[u8],
    header: &PnmHeader,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let w = header.width as usize;
    let h = header.height as usize;
    let row_bytes = w.div_ceil(8);
    let total_bytes = row_bytes
        .checked_mul(h)
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })?;

    if pixel_data.len() < total_bytes {
        return Err(BitmapError::UnexpectedEof);
    }

    let out_size = w.checked_mul(h).ok_or(BitmapError::DimensionsTooLarge {
        width: header.width,
        height: header.height,
    })?;
    let mut out = Vec::with_capacity(out_size);

    for row in 0..h {
        if row % 16 == 0 {
            stop.check()?;
        }
        let row_start = row * row_bytes;
        for col in 0..w {
            let byte_idx = row_start + col / 8;
            let bit_idx = 7 - (col % 8); // MSB first
            let bit = (pixel_data[byte_idx] >> bit_idx) & 1;
            // 1 = black (0), 0 = white (255)
            out.push(if bit == 1 { 0 } else { 255 });
        }
    }

    Ok(out)
}

/// Decode ASCII PGM/PPM (P2/P3): whitespace-separated decimal values.
/// Scales to 0-255 if maxval != 255.
pub(crate) fn decode_ascii_samples(
    pixel_data: &[u8],
    header: &PnmHeader,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let w = header.width as usize;
    let h = header.height as usize;
    let depth = header.depth as usize;
    let total = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(depth))
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })?;

    let scale = if header.maxval == 255 {
        None
    } else {
        Some(255.0 / header.maxval as f32)
    };

    let mut out = Vec::with_capacity(total);
    let mut pos = 0;

    for i in 0..total {
        if i % (w * depth * 16) == 0 {
            stop.check()?;
        }
        // Skip whitespace and comments
        while pos < pixel_data.len() {
            match pixel_data[pos] {
                b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
                b'#' => {
                    while pos < pixel_data.len() && pixel_data[pos] != b'\n' {
                        pos += 1;
                    }
                }
                _ => break,
            }
        }
        // Parse decimal number
        let start = pos;
        while pos < pixel_data.len() && pixel_data[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == start {
            return Err(BitmapError::UnexpectedEof);
        }
        let s = core::str::from_utf8(&pixel_data[start..pos])
            .map_err(|_| BitmapError::InvalidData("non-UTF8 in ASCII PNM".into()))?;
        let val: u32 = s
            .parse()
            .map_err(|_| BitmapError::InvalidData(alloc::format!("bad sample value: {s}")))?;

        let byte = if let Some(s) = scale {
            (val as f32 * s + 0.5) as u8
        } else {
            val as u8
        };
        out.push(byte);
    }

    Ok(out)
}

fn skip_whitespace_and_comments(data: &[u8], mut pos: usize) -> Result<usize, BitmapError> {
    loop {
        if pos >= data.len() {
            return Err(BitmapError::UnexpectedEof);
        }
        match data[pos] {
            b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
            b'#' => {
                while pos < data.len() && data[pos] != b'\n' {
                    pos += 1;
                }
                if pos < data.len() {
                    pos += 1;
                }
            }
            _ => return Ok(pos),
        }
    }
}

fn parse_u32(data: &[u8], pos: usize) -> Result<(u32, usize), BitmapError> {
    let mut end = pos;
    // Limit to 10 digits (u32::MAX = 4294967295, 10 digits)
    let max_end = core::cmp::min(pos + 11, data.len());
    while end < max_end && data[end].is_ascii_digit() {
        end += 1;
    }
    if end == pos {
        return Err(BitmapError::InvalidHeader("expected number".into()));
    }
    let s = core::str::from_utf8(&data[pos..end])
        .map_err(|_| BitmapError::InvalidHeader("non-UTF8 number".into()))?;
    let val: u32 = s
        .parse()
        .map_err(|_| BitmapError::InvalidHeader(alloc::format!("number too large: {s}")))?;
    Ok((val, end))
}
