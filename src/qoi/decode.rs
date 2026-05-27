//! QOI decoder.
//!
//! Headers are parsed via [rapid-qoi](https://github.com/zakarumych/rapid-qoi);
//! pixel chunks are decoded by the native, spec-compliant
//! [`super::run_decode`] kernel (runs are clamped to the output and carried
//! across rows).

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
    #[allow(dead_code)]
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

    // Row-level streaming decode with cancellation checks, using our native
    // spec-compliant chunk decoder (runs are clamped to the remaining output
    // and carried across rows — see `super::run_decode`).
    let encoded = data.get(14..).ok_or(BitmapError::UnexpectedEof)?;

    if has_alpha {
        let mut state = QoiDecodeState::<4>::new();
        let mut offset = 0;

        for row_idx in 0..height as usize {
            if row_idx % 16 == 0 {
                stop.check()?;
            }
            let row_start = row_idx * row_bytes;
            let row_end = row_start + row_bytes;
            let consumed = state
                .decode_into(&encoded[offset..], &mut output[row_start..row_end])
                .map_err(|()| BitmapError::UnexpectedEof)?;
            offset += consumed;
        }
    } else {
        let mut state = QoiDecodeState::<3>::new();
        let mut offset = 0;

        for row_idx in 0..height as usize {
            if row_idx % 16 == 0 {
                stop.check()?;
            }
            let row_start = row_idx * row_bytes;
            let row_end = row_start + row_bytes;
            let consumed = state
                .decode_into(&encoded[offset..], &mut output[row_start..row_end])
                .map_err(|()| BitmapError::UnexpectedEof)?;
            offset += consumed;
        }
    }

    Ok(output)
}

use super::run_decode::QoiDecodeState;
