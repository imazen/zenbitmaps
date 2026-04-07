//! QOI (Quite OK Image) format decoder and encoder (internal).
//!
//! QOI is a fast, lossless image format supporting RGB and RGBA at 8-bit depth.
//! Uses [rapid-qoi](https://github.com/zakarumych/rapid-qoi) for encoding
//! and decoding, with row-level cancellation via `decode_range`.

pub(crate) mod decode;
mod encode;

use crate::decode::DecodeOutput;
use crate::error::BitmapError;
use crate::limits::{self, Limits};
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Decode QOI data to RGB8 or RGBA8 pixels.
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    let hdr = decode::parse_header(data)?;
    let (width, height, has_alpha) = (hdr.width, hdr.height, hdr.has_alpha);
    if let Some(limits) = limits {
        limits.check(width, height)?;
    }
    let channels: usize = if has_alpha { 4 } else { 3 };
    let out_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(channels))
        .ok_or_else(|| BitmapError::LimitExceeded("output size overflows usize".into()))?;
    limits::check_output_size(out_bytes, limits)?;
    stop.check()?;
    let pixels = decode::decode_pixels(data, width, height, has_alpha, stop)?;
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
) -> Result<Vec<u8>, BitmapError> {
    encode::encode_qoi(pixels, width, height, layout, stop)
}
