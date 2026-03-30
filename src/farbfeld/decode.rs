//! Farbfeld decoder.
//!
//! Forked from zune-farbfeld 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use alloc::vec::Vec;
use enough::Stop;

use crate::error::BitmapError;

/// Parse farbfeld header, returning (width, height).
pub(crate) fn parse_header(data: &[u8]) -> Result<(u32, u32), BitmapError> {
    if data.len() < 16 {
        return Err(BitmapError::UnexpectedEof);
    }
    if &data[0..8] != b"farbfeld" {
        return Err(BitmapError::UnrecognizedFormat);
    }
    let width = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let height = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);

    if width == 0 {
        return Err(BitmapError::InvalidHeader("farbfeld width is zero".into()));
    }
    if height == 0 {
        return Err(BitmapError::InvalidHeader("farbfeld height is zero".into()));
    }
    Ok((width, height))
}

/// Decode farbfeld pixel data from big-endian to native endian u16 (as bytes).
pub(crate) fn decode_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let sample_count = pixel_count
        .checked_mul(4)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let input_bytes = sample_count
        .checked_mul(2)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;

    let pixel_data = data
        .get(16..16 + input_bytes)
        .ok_or(BitmapError::UnexpectedEof)?;

    let mut out = Vec::with_capacity(input_bytes);

    // Convert each u16 from big-endian to native endian
    let samples_per_row = width as usize * 4;
    let row_bytes = samples_per_row * 2;
    for (row_idx, chunk) in pixel_data.chunks_exact(row_bytes).enumerate() {
        if row_idx % 16 == 0 {
            stop.check()?;
        }
        // Process 4 u16s (8 bytes) at a time for better throughput
        let mut remaining = chunk;
        while remaining.len() >= 8 {
            let a = u16::from_be_bytes([remaining[0], remaining[1]]);
            let b = u16::from_be_bytes([remaining[2], remaining[3]]);
            let c = u16::from_be_bytes([remaining[4], remaining[5]]);
            let d = u16::from_be_bytes([remaining[6], remaining[7]]);
            out.extend_from_slice(&a.to_ne_bytes());
            out.extend_from_slice(&b.to_ne_bytes());
            out.extend_from_slice(&c.to_ne_bytes());
            out.extend_from_slice(&d.to_ne_bytes());
            remaining = &remaining[8..];
        }
        for pair in remaining.chunks_exact(2) {
            let val = u16::from_be_bytes([pair[0], pair[1]]);
            out.extend_from_slice(&val.to_ne_bytes());
        }
    }

    if out.len() != input_bytes {
        return Err(BitmapError::UnexpectedEof);
    }

    Ok(out)
}
