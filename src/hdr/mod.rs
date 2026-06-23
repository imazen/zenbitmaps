//! Radiance HDR (.hdr / RGBE) image format decoder and encoder (internal).
//!
//! Radiance HDR stores high dynamic range images using RGBE encoding
//! (shared exponent). Output is always `RgbF32` (3 channels, 32-bit float).

pub(crate) mod decode;
mod encode;

use crate::alloc_util::AllocPref;
use crate::decode::DecodeOutput;
use crate::error::BitmapError;
use crate::limits::{self, Limits};
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;
use whereat::at;

/// Decode Radiance HDR data to RgbF32 pixels.
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

/// Decode Radiance HDR data, honoring an explicit [`AllocPref`] at the
/// output-buffer allocation.
pub(crate) fn decode_with_alloc_pref<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<DecodeOutput<'a>> {
    let (width, height, offset) = decode::parse_header(data)?;
    limits::check_dimensions(width, height, limits)?;
    let out_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(12)) // 3 channels × 4 bytes per f32
        .ok_or_else(|| {
            at!(BitmapError::LimitExceeded(
                "output size overflows usize".into()
            ))
        })?;
    limits::check_output_size(out_bytes, limits)?;
    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
    let pixels = decode::decode_pixels(data, offset, width, height, alloc_pref, stop)?;
    Ok(DecodeOutput::owned(
        pixels,
        width,
        height,
        PixelLayout::RgbF32,
    ))
}

/// Encode pixels as Radiance HDR (RGBE with new-style RLE).
pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    encode::encode_hdr(pixels, width, height, layout, stop)
}
