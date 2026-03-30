//! QOI decoder.
//!
//! Uses [rapid-qoi](https://github.com/zakarumych/rapid-qoi) for decoding.

use alloc::vec;
use alloc::vec::Vec;
use enough::Stop;

use crate::error::BitmapError;

/// Parsed QOI header info.
pub(crate) struct QoiHeaderInfo {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    /// True if the QOI colorspace field signals linear (not sRGB).
    pub is_linear: bool,
}

/// Parse QOI header, returning dimensions, alpha, and colorspace.
pub(crate) fn parse_header(data: &[u8]) -> Result<QoiHeaderInfo, BitmapError> {
    let qoi = rapid_qoi::Qoi::decode_header(data)
        .map_err(|e| BitmapError::InvalidHeader(alloc::format!("{e:?}")))?;

    if qoi.width == 0 {
        return Err(BitmapError::InvalidHeader("QOI width is zero".into()));
    }
    if qoi.height == 0 {
        return Err(BitmapError::InvalidHeader("QOI height is zero".into()));
    }

    let is_linear = matches!(qoi.colors, rapid_qoi::Colors::Rgb | rapid_qoi::Colors::Rgba);

    Ok(QoiHeaderInfo {
        width: qoi.width,
        height: qoi.height,
        has_alpha: qoi.colors.has_alpha(),
        is_linear,
    })
}

/// Decode QOI pixel data with row-level cancellation.
pub(crate) fn decode_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    has_alpha: bool,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let channels: usize = if has_alpha { 4 } else { 3 };
    let row_bytes = (width as usize)
        .checked_mul(channels)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let total_bytes = row_bytes
        .checked_mul(height as usize)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;

    let mut output = vec![0u8; total_bytes];

    // Use decode_range for row-level streaming with cancellation checks
    let encoded = data.get(14..).ok_or(BitmapError::UnexpectedEof)?;

    if has_alpha {
        let mut index = [<[u8; 4]>::new_opaque(); 64];
        let mut px = <[u8; 4]>::new_opaque();
        let mut run = 0usize;
        let mut offset = 0;

        for row_idx in 0..height as usize {
            if row_idx % 16 == 0 {
                stop.check()?;
            }
            let row_start = row_idx * row_bytes;
            let row_end = row_start + row_bytes;
            let consumed = rapid_qoi::Qoi::decode_range::<4>(
                &mut index,
                &mut px,
                &mut run,
                &encoded[offset..],
                &mut output[row_start..row_end],
            )
            .map_err(|e| BitmapError::InvalidData(alloc::format!("{e:?}")))?;
            offset += consumed;
        }
    } else {
        let mut index = [<[u8; 3]>::new(); 64];
        let mut px = <[u8; 3]>::new();
        let mut run = 0usize;
        let mut offset = 0;

        for row_idx in 0..height as usize {
            if row_idx % 16 == 0 {
                stop.check()?;
            }
            let row_start = row_idx * row_bytes;
            let row_end = row_start + row_bytes;
            let consumed = rapid_qoi::Qoi::decode_range::<3>(
                &mut index,
                &mut px,
                &mut run,
                &encoded[offset..],
                &mut output[row_start..row_end],
            )
            .map_err(|e| BitmapError::InvalidData(alloc::format!("{e:?}")))?;
            offset += consumed;
        }
    }

    Ok(output)
}

use rapid_qoi::Pixel;
