//! Farbfeld decoder.
//!
//! Forked from zune-farbfeld 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use alloc::vec;
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
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
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

    // Pre-allocate output and write directly — no Vec growth
    let mut out = vec![0u8; input_bytes];
    let row_bytes = width as usize * 8; // 4 channels × 2 bytes

    for (row_idx, (src_row, dst_row)) in pixel_data
        .chunks_exact(row_bytes)
        .zip(out.chunks_exact_mut(row_bytes))
        .enumerate()
    {
        if row_idx % 16 == 0 {
            stop.check()?;
        }
        be16_to_ne_bulk(src_row, dst_row);
    }

    Ok(out)
}

/// Batch big-endian u16 → native endian u16, writing directly into output.
///
/// Processes 8 u16s (16 bytes) per iteration for pipeline-friendly throughput.
/// src and dst must have equal length and be a multiple of 2.
#[inline]
fn be16_to_ne_bulk(src: &[u8], dst: &mut [u8]) {
    debug_assert_eq!(src.len(), dst.len());
    debug_assert_eq!(src.len() % 2, 0);

    let mut i = 0;
    let len = src.len();

    // Process 16 bytes (8 u16s) at a time
    while i + 16 <= len {
        let s = &src[i..i + 16];
        let d = &mut dst[i..i + 16];
        let a = u16::from_be_bytes([s[0], s[1]]);
        let b = u16::from_be_bytes([s[2], s[3]]);
        let c = u16::from_be_bytes([s[4], s[5]]);
        let d_val = u16::from_be_bytes([s[6], s[7]]);
        let e = u16::from_be_bytes([s[8], s[9]]);
        let f = u16::from_be_bytes([s[10], s[11]]);
        let g = u16::from_be_bytes([s[12], s[13]]);
        let h = u16::from_be_bytes([s[14], s[15]]);
        d[0..2].copy_from_slice(&a.to_ne_bytes());
        d[2..4].copy_from_slice(&b.to_ne_bytes());
        d[4..6].copy_from_slice(&c.to_ne_bytes());
        d[6..8].copy_from_slice(&d_val.to_ne_bytes());
        d[8..10].copy_from_slice(&e.to_ne_bytes());
        d[10..12].copy_from_slice(&f.to_ne_bytes());
        d[12..14].copy_from_slice(&g.to_ne_bytes());
        d[14..16].copy_from_slice(&h.to_ne_bytes());
        i += 16;
    }

    // Handle remaining pairs
    while i + 2 <= len {
        let val = u16::from_be_bytes([src[i], src[i + 1]]);
        dst[i..i + 2].copy_from_slice(&val.to_ne_bytes());
        i += 2;
    }
}
