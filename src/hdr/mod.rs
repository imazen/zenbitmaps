//! Radiance HDR (.hdr / RGBE) image format decoder and encoder (internal).
//!
//! Radiance HDR stores high dynamic range images using RGBE encoding
//! (shared exponent). Output is always `RgbF32` (3 channels, 32-bit float).

pub(crate) mod decode;
mod encode;

use crate::decode::DecodeOutput;
use crate::error::BitmapError;
use crate::limits::{self, Limits};
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Decode Radiance HDR data to RgbF32 pixels.
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    let (width, height, offset) = decode::parse_header(data)?;
    if let Some(limits) = limits {
        limits.check(width, height)?;
    }
    let out_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(12)) // 3 channels × 4 bytes per f32
        .ok_or_else(|| BitmapError::LimitExceeded("output size overflows usize".into()))?;
    limits::check_output_size(out_bytes, limits)?;
    stop.check()?;
    let pixels = decode::decode_pixels(data, offset, width, height, stop)?;
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
) -> Result<Vec<u8>, BitmapError> {
    encode::encode_hdr(pixels, width, height, layout, stop)
}
