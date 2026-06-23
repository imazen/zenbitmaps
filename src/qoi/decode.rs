//! QOI decoder.
//!
//! Headers are parsed via the vendored QOI core ([`crate::qoi::rapid_qoi`]);
//! pixel chunks are decoded by the same vendored `decode_range` kernel (runs
//! are clamped to the output and carried across rows via [`QoiDecodeState`]).

use alloc::vec::Vec;
use enough::Stop;

use crate::alloc_util::{self, AllocPref};
use crate::error::BitmapError;
use crate::qoi::rapid_qoi;

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
pub(crate) fn parse_header(data: &[u8]) -> crate::Result<QoiHeaderInfo> {
    let qoi = rapid_qoi::Qoi::decode_header(data)
        .map_err(|e| whereat::at!(BitmapError::InvalidHeader(alloc::format!("{e:?}"))))?;

    if qoi.width == 0 {
        return Err(whereat::at!(BitmapError::InvalidHeader(
            "QOI width is zero".into()
        )));
    }
    if qoi.height == 0 {
        return Err(whereat::at!(BitmapError::InvalidHeader(
            "QOI height is zero".into()
        )));
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
///
/// The output buffer is sized from the (untrusted) header dimensions →
/// `alloc_pref` with site default `true` (fallible).
pub(crate) fn decode_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    has_alpha: bool,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    let channels: usize = if has_alpha { 4 } else { 3 };
    let row_bytes = (width as usize)
        .checked_mul(channels)
        .ok_or_else(|| whereat::at!(BitmapError::DimensionsTooLarge { width, height }))?;
    let total_bytes = row_bytes
        .checked_mul(height as usize)
        .ok_or_else(|| whereat::at!(BitmapError::DimensionsTooLarge { width, height }))?;

    let mut output = alloc_util::alloc_zeroed(alloc_pref, true, total_bytes)?;

    // Row-level streaming decode with cancellation checks, using the vendored
    // QOI kernel via `QoiDecodeState` (runs are clamped to the remaining output
    // and carried across rows).
    let encoded = data
        .get(14..)
        .ok_or_else(|| whereat::at!(BitmapError::UnexpectedEof))?;

    if has_alpha {
        let mut state = QoiDecodeState::<4>::new();
        let mut offset = 0;

        for row_idx in 0..height as usize {
            if row_idx % 16 == 0 {
                stop.check()
                    .map_err(|r| whereat::at!(BitmapError::from(r)))?;
            }
            let row_start = row_idx * row_bytes;
            let row_end = row_start + row_bytes;
            let consumed = state
                .decode_into(&encoded[offset..], &mut output[row_start..row_end])
                .map_err(|()| whereat::at!(BitmapError::UnexpectedEof))?;
            offset += consumed;
        }
    } else {
        let mut state = QoiDecodeState::<3>::new();
        let mut offset = 0;

        for row_idx in 0..height as usize {
            if row_idx % 16 == 0 {
                stop.check()
                    .map_err(|r| whereat::at!(BitmapError::from(r)))?;
            }
            let row_start = row_idx * row_bytes;
            let row_end = row_start + row_bytes;
            let consumed = state
                .decode_into(&encoded[offset..], &mut output[row_start..row_end])
                .map_err(|()| whereat::at!(BitmapError::UnexpectedEof))?;
            offset += consumed;
        }
    }

    Ok(output)
}

/// Streaming QOI decode state carried across output chunks (rows).
///
/// `N` is the number of channels (3 for RGB, 4 for RGBA). This is a thin
/// wrapper around the vendored [`rapid_qoi::Qoi::decode_range`] kernel: it owns
/// the running index array, the previous pixel, and any run-length left over
/// from a `QOI_OP_RUN` that did not fit in the previous output chunk. The
/// vendored kernel clamps runs to the remaining output and reports the unfilled
/// remainder, so a run that spans multiple rows decodes correctly.
pub(crate) struct QoiDecodeState<const N: usize> {
    /// Running array of previously seen pixels, indexed by the QOI hash.
    index: [[u8; N]; 64],
    /// Previous pixel value.
    px: [u8; N],
    /// Run-length remaining from a `QOI_OP_RUN` that did not fit in the
    /// previous output chunk.
    run: usize,
}

impl<const N: usize> QoiDecodeState<N>
where
    [u8; N]: rapid_qoi::Pixel,
{
    /// Create fresh decode state per the QOI spec: the running index array is
    /// zero-initialized and the previous pixel starts at opaque black
    /// `{0,0,0,255}` (alpha implicit for RGB).
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            index: [<[u8; N] as rapid_qoi::Pixel>::new(); 64],
            px: <[u8; N] as rapid_qoi::Pixel>::new_opaque(),
            run: 0,
        }
    }

    /// Decode QOI chunks from `bytes` into `out`, filling exactly `out.len()`
    /// bytes (which must be a whole multiple of `N`).
    ///
    /// Returns the number of bytes consumed from `bytes`. Decode state
    /// (`index`, `px`, `run`) is carried in `self` so the next call resumes
    /// where this one left off.
    ///
    /// Returns `Err(())` on truncated input (a chunk that needs more bytes than
    /// are available).
    pub(crate) fn decode_into(&mut self, bytes: &[u8], out: &mut [u8]) -> Result<usize, ()> {
        rapid_qoi::Qoi::decode_range::<N>(&mut self.index, &mut self.px, &mut self.run, bytes, out)
            .map_err(|_| ())
    }
}
