//! PNM decoder: P1-P7, PFM.
//!
//! P1 (ASCII PBM), P2 (ASCII PGM), P3 (ASCII PPM) — text pixel data.
//! P4 (binary PBM) — bit-packed, 8 pixels per byte, MSB first.
//! P5 (binary PGM), P6 (binary PPM) — raw binary pixel data.
//! P7 (PAM) — arbitrary channels, binary. PFM — float, binary.
//!
//! Credits: Draws from zune-ppm by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use super::PnmHeader;
use crate::alloc_util::{self, AllocPref};
use crate::error::BitmapError;
use crate::pixel::PixelLayout;
use crate::pnm::PnmFormat;
use alloc::string::String;
use alloc::vec::Vec;
use enough::Stop;

/// Parse header from raw data.
pub(crate) fn parse_header(data: &[u8]) -> crate::Result<PnmHeader> {
    if data.len() < 3 {
        return Err(whereat::at!(BitmapError::UnexpectedEof));
    }

    match &data[..2] {
        b"P5" => parse_p5_p6_header(data, PnmFormat::Pgm),
        b"P6" => parse_p5_p6_header(data, PnmFormat::Ppm),
        b"P7" => parse_p7_header(data),
        b"Pf" | b"PF" => parse_pfm_header(data),
        b"P1" | b"P4" => parse_pbm_header(data),
        b"P2" => parse_p5_p6_header(data, PnmFormat::Pgm),
        b"P3" => parse_p5_p6_header(data, PnmFormat::Ppm),
        _ => Err(whereat::at!(BitmapError::UnrecognizedFormat)),
    }
}

fn parse_p5_p6_header(data: &[u8], format: PnmFormat) -> crate::Result<PnmHeader> {
    let mut pos = 2;

    pos = skip_whitespace_and_comments(data, pos)?;
    let (width, new_pos) = parse_u32(data, pos)?;
    pos = skip_whitespace_and_comments(data, new_pos)?;
    let (height, new_pos) = parse_u32(data, pos)?;
    pos = skip_whitespace_and_comments(data, new_pos)?;
    let (maxval, new_pos) = parse_u32(data, pos)?;

    if width == 0 || height == 0 {
        return Err(whereat::at!(BitmapError::InvalidHeader(
            "width and height must be non-zero".into(),
        )));
    }
    if maxval == 0 || maxval > 65535 {
        return Err(whereat::at!(BitmapError::InvalidHeader(alloc::format!(
            "maxval must be 1-65535, got {maxval}"
        ))));
    }

    if new_pos >= data.len() {
        return Err(whereat::at!(BitmapError::UnexpectedEof));
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
            return Err(whereat::at!(BitmapError::UnsupportedVariant(
                alloc::format!("unexpected format {:?} in P5/P6 parser", format)
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
fn parse_pbm_header(data: &[u8]) -> crate::Result<PnmHeader> {
    let mut pos = 2;

    pos = skip_whitespace_and_comments(data, pos)?;
    let (width, new_pos) = parse_u32(data, pos)?;
    pos = skip_whitespace_and_comments(data, new_pos)?;
    let (height, new_pos) = parse_u32(data, pos)?;

    if width == 0 || height == 0 {
        return Err(whereat::at!(BitmapError::InvalidHeader(
            "width and height must be non-zero".into(),
        )));
    }

    // P1: single whitespace separates header from ASCII data
    // P4: single whitespace byte separates header from binary data
    if new_pos >= data.len() {
        return Err(whereat::at!(BitmapError::UnexpectedEof));
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

fn parse_p7_header(data: &[u8]) -> crate::Result<PnmHeader> {
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
            .map_err(|_| whereat::at!(BitmapError::InvalidHeader("non-UTF8 in PAM header".into())))?
            .trim();

        if line == "ENDHDR" {
            pos = line_end + 1;
            break;
        }

        if let Some(rest) = line.strip_prefix("WIDTH ") {
            width = Some(
                rest.trim()
                    .parse()
                    .map_err(|_| whereat::at!(BitmapError::InvalidHeader("bad WIDTH".into())))?,
            );
        } else if let Some(rest) = line.strip_prefix("HEIGHT ") {
            height = Some(
                rest.trim()
                    .parse()
                    .map_err(|_| whereat::at!(BitmapError::InvalidHeader("bad HEIGHT".into())))?,
            );
        } else if let Some(rest) = line.strip_prefix("DEPTH ") {
            depth = Some(
                rest.trim()
                    .parse()
                    .map_err(|_| whereat::at!(BitmapError::InvalidHeader("bad DEPTH".into())))?,
            );
        } else if let Some(rest) = line.strip_prefix("MAXVAL ") {
            maxval = Some(
                rest.trim()
                    .parse()
                    .map_err(|_| whereat::at!(BitmapError::InvalidHeader("bad MAXVAL".into())))?,
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
            return Err(whereat::at!(BitmapError::InvalidHeader(
                "no ENDHDR found".into()
            )));
        }
    }

    let width =
        width.ok_or_else(|| whereat::at!(BitmapError::InvalidHeader("missing WIDTH".into())))?;
    let height =
        height.ok_or_else(|| whereat::at!(BitmapError::InvalidHeader("missing HEIGHT".into())))?;
    let depth =
        depth.ok_or_else(|| whereat::at!(BitmapError::InvalidHeader("missing DEPTH".into())))?;
    let maxval =
        maxval.ok_or_else(|| whereat::at!(BitmapError::InvalidHeader("missing MAXVAL".into())))?;

    if width == 0 || height == 0 {
        return Err(whereat::at!(BitmapError::InvalidHeader(
            "width and height must be non-zero".into(),
        )));
    }
    if depth == 0 {
        return Err(whereat::at!(BitmapError::InvalidHeader(
            "DEPTH must be non-zero".into()
        )));
    }

    let layout = match (depth, maxval > 255) {
        (1, false) => PixelLayout::Gray8,
        (1, true) => PixelLayout::Gray16,
        (3, false) => PixelLayout::Rgb8,
        (3, true) => PixelLayout::Rgb8,
        (4, false) => PixelLayout::Rgba8,
        (4, true) => PixelLayout::Rgba8,
        _ => {
            return Err(whereat::at!(BitmapError::UnsupportedVariant(
                alloc::format!("PAM DEPTH={depth} not supported")
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

fn parse_pfm_header(data: &[u8]) -> crate::Result<PnmHeader> {
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
        .map_err(|_| whereat::at!(BitmapError::InvalidHeader("non-UTF8 scale".into())))?
        .trim();
    let scale: f32 = scale_str.parse().map_err(|_| {
        whereat::at!(BitmapError::InvalidHeader(alloc::format!(
            "bad scale: {scale_str}"
        )))
    })?;

    // The PFM specification (Pat Hanrahan, "PFM image format") defines the
    // scale as a non-zero finite number whose sign communicates byte order
    // and whose magnitude is a linear scale factor. Reject NaN, +Inf, -Inf,
    // and zero — any of which would corrupt downstream pixel math (NaN
    // poisons subsequent products; ±Inf saturates; zero is meaningless as
    // a scale factor and is excluded by spec).
    if !scale.is_finite() || scale == 0.0 {
        return Err(whereat::at!(BitmapError::InvalidHeader(alloc::format!(
            "PFM scale must be a non-zero finite value, got: {scale_str}"
        ))));
    }

    if width == 0 || height == 0 {
        return Err(whereat::at!(BitmapError::InvalidHeader(
            "width and height must be non-zero".into(),
        )));
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
///
/// The output buffer is sized from the (untrusted) header dimensions →
/// `alloc_pref` with site default `true` (fallible).
pub(crate) fn decode_integer_transform(
    pixel_data: &[u8],
    header: &PnmHeader,
    expected_src: usize,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    let w = header.width as usize;
    let h = header.height as usize;
    let depth = header.depth as usize;
    let is_16bit = header.maxval > 255;

    if !is_16bit {
        // Scale from maxval to 255
        let scale = 255.0 / header.maxval as f32;
        let mut out = alloc_util::vec_with_capacity(alloc_pref, true, expected_src)?;
        let stop_interval = w.saturating_mul(depth).saturating_mul(16).max(1);
        for (i, &b) in pixel_data[..expected_src].iter().enumerate() {
            if i % stop_interval == 0 {
                stop.check()
                    .map_err(|r| whereat::at!(BitmapError::from(r)))?;
            }
            out.push((b as f32 * scale + 0.5) as u8);
        }
        Ok(out)
    } else {
        match header.layout {
            PixelLayout::Gray16 => {
                // PNM binary 16-bit samples are big-endian on disk (PGM/PAM
                // spec: "the most significant byte is first"). `Gray16` is
                // documented native-endian, and the ASCII P2 path
                // (`decode_ascii_samples`) emits native-endian `u16`, so convert
                // here to keep the binary and ASCII paths byte-identical for the
                // same logical image (issue #12). Mirrors farbfeld's BE→native
                // decode; a no-op on big-endian hosts, a byte-swap on LE.
                let mut out = alloc_util::vec_with_capacity(alloc_pref, true, expected_src)?;
                let stop_interval = w.saturating_mul(depth).saturating_mul(16).max(1);
                for (i, pair) in pixel_data[..expected_src].chunks_exact(2).enumerate() {
                    if i % stop_interval == 0 {
                        stop.check()
                            .map_err(|r| whereat::at!(BitmapError::from(r)))?;
                    }
                    let val = u16::from_be_bytes([pair[0], pair[1]]);
                    out.extend_from_slice(&val.to_ne_bytes());
                }
                Ok(out)
            }
            _ => {
                let num_samples = w
                    .checked_mul(h)
                    .and_then(|wh| wh.checked_mul(depth))
                    .ok_or_else(|| {
                        whereat::at!(BitmapError::DimensionsTooLarge {
                            width: header.width,
                            height: header.height,
                        })
                    })?;
                // Verify 2*num_samples fits and data is sufficient
                let needed_bytes = num_samples.checked_mul(2).ok_or_else(|| {
                    whereat::at!(BitmapError::DimensionsTooLarge {
                        width: header.width,
                        height: header.height,
                    })
                })?;
                if pixel_data.len() < needed_bytes {
                    return Err(whereat::at!(BitmapError::UnexpectedEof));
                }
                let scale = 255.0 / header.maxval as f32;
                let stop_interval = w.saturating_mul(depth).saturating_mul(16).max(1);
                let mut out = alloc_util::vec_with_capacity(alloc_pref, true, num_samples)?;
                for i in 0..num_samples {
                    if i % stop_interval == 0 {
                        stop.check()
                            .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
///
/// The output buffer is sized from the (untrusted) header dimensions →
/// `alloc_pref` with site default `true` (fallible).
pub(crate) fn decode_pfm(
    pixel_data: &[u8],
    header: &PnmHeader,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    let w = header.width as usize;
    let h = header.height as usize;
    let depth = header.depth as usize;
    let num_floats = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(depth))
        .ok_or_else(|| {
            whereat::at!(BitmapError::DimensionsTooLarge {
                width: header.width,
                height: header.height,
            })
        })?;
    let expected_bytes = num_floats.checked_mul(4).ok_or_else(|| {
        whereat::at!(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })
    })?;

    if pixel_data.len() < expected_bytes {
        return Err(whereat::at!(BitmapError::UnexpectedEof));
    }

    let is_little_endian = header.pfm_scale < 0.0;
    let scale = header.pfm_scale.abs();

    let mut out = alloc_util::vec_with_capacity(alloc_pref, true, expected_bytes)?;
    let row_floats = w.checked_mul(depth).ok_or_else(|| {
        whereat::at!(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })
    })?;
    let row_bytes = row_floats.checked_mul(4).ok_or_else(|| {
        whereat::at!(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })
    })?;

    // PFM stores rows bottom-to-top
    for row in (0..h).rev() {
        if row % 16 == 0 {
            stop.check()
                .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
///
/// The output buffer is sized from the (untrusted) header dimensions →
/// `alloc_pref` with site default `true` (fallible).
pub(crate) fn decode_ascii_pbm(
    pixel_data: &[u8],
    header: &PnmHeader,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    let total = (header.width as usize)
        .checked_mul(header.height as usize)
        .ok_or_else(|| {
            whereat::at!(BitmapError::DimensionsTooLarge {
                width: header.width,
                height: header.height,
            })
        })?;

    let mut out = alloc_util::vec_with_capacity(alloc_pref, true, total)?;
    let mut pos = 0;

    for i in 0..total {
        if i % (header.width as usize * 16) == 0 {
            stop.check()
                .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
            return Err(whereat::at!(BitmapError::UnexpectedEof));
        }
        // PBM: 1 = black (0), 0 = white (255)
        let val = match pixel_data[pos] {
            b'0' => 255,
            b'1' => 0,
            c => {
                return Err(whereat::at!(BitmapError::InvalidData(alloc::format!(
                    "P1: expected '0' or '1', got '{}'",
                    c as char
                ))));
            }
        };
        out.push(val);
        pos += 1;
    }

    Ok(out)
}

/// Decode binary PBM (P4): 8 pixels per byte, MSB first.
/// 1 = black (0), 0 = white (255). Rows are padded to byte boundaries.
///
/// The output buffer is sized from the (untrusted) header dimensions →
/// `alloc_pref` with site default `true` (fallible).
pub(crate) fn decode_p4_bitpacked(
    pixel_data: &[u8],
    header: &PnmHeader,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    let w = header.width as usize;
    let h = header.height as usize;
    let row_bytes = w.div_ceil(8);
    let total_bytes = row_bytes.checked_mul(h).ok_or_else(|| {
        whereat::at!(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })
    })?;

    if pixel_data.len() < total_bytes {
        return Err(whereat::at!(BitmapError::UnexpectedEof));
    }

    let out_size = w.checked_mul(h).ok_or_else(|| {
        whereat::at!(BitmapError::DimensionsTooLarge {
            width: header.width,
            height: header.height,
        })
    })?;
    let mut out = alloc_util::vec_with_capacity(alloc_pref, true, out_size)?;

    for row in 0..h {
        if row % 16 == 0 {
            stop.check()
                .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
///
/// The output buffer is sized from the (untrusted) header dimensions →
/// `alloc_pref` with site default `true` (fallible).
pub(crate) fn decode_ascii_samples(
    pixel_data: &[u8],
    header: &PnmHeader,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    let w = header.width as usize;
    let h = header.height as usize;
    let depth = header.depth as usize;
    let total = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(depth))
        .ok_or_else(|| {
            whereat::at!(BitmapError::DimensionsTooLarge {
                width: header.width,
                height: header.height,
            })
        })?;

    // The OUTPUT byte width is decided by the *layout*, not the source maxval,
    // so the ASCII path produces byte-for-byte the same buffer the binary path
    // (`decode_integer_transform`) does for the same logical image:
    //
    // * A genuinely 16-bit-per-channel layout (Gray16 — the only one the P2/P3
    //   ASCII path produces; Rgba16 listed for completeness) keeps 2 raw
    //   native-endian bytes per sample. Emitting a single downscaled u8 here
    //   produced HALF the declared bytes — an OOB panic in
    //   `PixelBuffer::as_slice` and silent 16-bit precision loss (fuzz zenpipe#51).
    // * An 8-bit layout (Rgb8 — produced by 16-bit *P3 PPM*, since there is no
    //   Rgb16 layout) downscales 16-bit samples to one u8 via `val·255/maxval`,
    //   exactly like the binary P6 16-bit path. The pre-fix code keyed the byte
    //   width on `maxval > 255` alone, so 16-bit P3 emitted 2 bytes/sample while
    //   tagging the buffer Rgb8 (1 byte/channel) — a 6-byte 1×1 "Rgb8" image.
    //   `encode_pam` then truncated it back to 3 bytes, breaking the roundtrip
    //   (fuzz zenbitmaps#10).
    let layout_is_16bit = header.layout.bytes_per_pixel() == 2 * header.layout.channels();
    let bytes_per_sample = if layout_is_16bit { 2 } else { 1 };
    // Downscale when the source is wider than the 8-bit target (16-bit samples
    // into an 8-bit layout, or any sub-255 maxval into an 8-bit layout).
    let scale8 = (!layout_is_16bit && header.maxval != 255).then(|| 255.0 / header.maxval as f32);

    let mut out =
        alloc_util::vec_with_capacity(alloc_pref, true, total.saturating_mul(bytes_per_sample))?;
    let mut pos = 0;

    for i in 0..total {
        if i % (w * depth * 16) == 0 {
            stop.check()
                .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
            return Err(whereat::at!(BitmapError::UnexpectedEof));
        }
        let s = core::str::from_utf8(&pixel_data[start..pos])
            .map_err(|_| whereat::at!(BitmapError::InvalidData("non-UTF8 in ASCII PNM".into())))?;
        let val: u32 = s.parse().map_err(|_| {
            whereat::at!(BitmapError::InvalidData(alloc::format!(
                "bad sample value: {s}"
            )))
        })?;

        // Clamp out-of-range samples (a malformed ASCII value may exceed maxval).
        let val = val.min(header.maxval);
        if layout_is_16bit {
            // 16-bit-per-channel layout (Gray16): raw native-endian u16.
            out.extend_from_slice(&(val as u16).to_ne_bytes());
        } else if let Some(s) = scale8 {
            // 8-bit layout fed by a wider maxval (incl. 16-bit P3 PPM →
            // Rgb8): downscale to 0..=255, matching the binary 16-bit path.
            out.push((val as f32 * s + 0.5) as u8);
        } else {
            out.push(val as u8);
        }
    }

    Ok(out)
}

fn skip_whitespace_and_comments(data: &[u8], mut pos: usize) -> crate::Result<usize> {
    loop {
        if pos >= data.len() {
            return Err(whereat::at!(BitmapError::UnexpectedEof));
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

fn parse_u32(data: &[u8], pos: usize) -> crate::Result<(u32, usize)> {
    let mut end = pos;
    // Limit to 10 digits (u32::MAX = 4294967295, 10 digits)
    let max_end = core::cmp::min(pos + 11, data.len());
    while end < max_end && data[end].is_ascii_digit() {
        end += 1;
    }
    if end == pos {
        return Err(whereat::at!(BitmapError::InvalidHeader(
            "expected number".into()
        )));
    }
    let s = core::str::from_utf8(&data[pos..end])
        .map_err(|_| whereat::at!(BitmapError::InvalidHeader("non-UTF8 number".into())))?;
    let val: u32 = s.parse().map_err(|_| {
        whereat::at!(BitmapError::InvalidHeader(alloc::format!(
            "number too large: {s}"
        )))
    })?;
    Ok((val, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pfm_header_with_scale(scale_text: &str) -> Vec<u8> {
        // Minimal PF (color) header: "PF\n2 2\n<scale>\n"
        let mut v = Vec::new();
        v.extend_from_slice(b"PF\n2 2\n");
        v.extend_from_slice(scale_text.as_bytes());
        v.push(b'\n');
        v
    }

    fn assert_pfm_invalid(scale_text: &str) {
        let data = pfm_header_with_scale(scale_text);
        match parse_pfm_header(&data) {
            Ok(_) => panic!("expected InvalidHeader for scale {scale_text:?}"),
            Err(e) => match e.error() {
                BitmapError::InvalidHeader(_) => {}
                _ => panic!("expected InvalidHeader for scale {scale_text:?}, got {e}"),
            },
        }
    }

    #[test]
    fn pfm_rejects_nan_scale() {
        assert_pfm_invalid("NaN");
        assert_pfm_invalid("nan");
    }

    #[test]
    fn pfm_rejects_inf_scale() {
        for s in ["inf", "Infinity", "-inf", "-Infinity"] {
            assert_pfm_invalid(s);
        }
    }

    #[test]
    fn pfm_rejects_zero_scale() {
        for s in ["0", "0.0", "-0", "-0.0"] {
            assert_pfm_invalid(s);
        }
    }

    #[test]
    fn pfm_accepts_normal_scale() {
        let data = pfm_header_with_scale("-1.0");
        let h = match parse_pfm_header(&data) {
            Ok(h) => h,
            Err(e) => panic!("expected valid PFM header, got {e}"),
        };
        assert_eq!(h.width, 2);
        assert_eq!(h.height, 2);
        assert_eq!(h.pfm_scale, -1.0);
    }
}
