//! QOI (Quite OK Image) format decoder and encoder (internal).
//!
//! QOI is a fast, lossless image format supporting RGB and RGBA at 8-bit depth.
//! Both encoding and decoding use the vendored QOI core in [`rapid_qoi`]
//! (vendored from rapid-qoi, with the local `QOI_OP_RUN` clamp fix); decoding
//! wraps the vendored `decode_range` in a small streaming state
//! ([`decode::QoiDecodeState`]) for row-level cancellation and runs that cross
//! row boundaries.

pub(crate) mod decode;
mod encode;
pub(crate) mod rapid_qoi;

use crate::alloc_util::AllocPref;
use crate::decode::DecodeOutput;
use crate::error::BitmapError;
use crate::limits::{self, Limits};
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Decode QOI data to RGB8 or RGBA8 pixels.
///
/// Allocations use each site's default fallibility; for the zencodec path that
/// honors [`AllocPreference`](zencodec::AllocPreference), call
/// [`decode_with_alloc_pref`].
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> crate::Result<DecodeOutput<'a>> {
    decode_with_alloc_pref(data, limits, AllocPref::CodecDefault, stop)
}

/// Decode QOI data, honoring an explicit [`AllocPref`] at the output-buffer
/// allocation.
pub(crate) fn decode_with_alloc_pref<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<DecodeOutput<'a>> {
    let hdr = decode::parse_header(data)?;
    let (width, height, has_alpha) = (hdr.width, hdr.height, hdr.has_alpha);
    limits::check_dimensions(width, height, limits)?;
    let channels: usize = if has_alpha { 4 } else { 3 };
    let out_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(channels))
        .ok_or_else(|| {
            whereat::at!(BitmapError::LimitExceeded(
                "output size overflows usize".into()
            ))
        })?;
    limits::check_output_size(out_bytes, limits)?;
    stop.check()
        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
    let pixels = decode::decode_pixels(data, width, height, has_alpha, alloc_pref, stop)?;
    let layout = if has_alpha {
        PixelLayout::Rgba8
    } else {
        PixelLayout::Rgb8
    };
    Ok(DecodeOutput::owned(pixels, width, height, layout))
}

/// Encode pixels as QOI.
pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    encode::encode_qoi(pixels, width, height, layout, stop)
}
