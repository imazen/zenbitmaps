//! Farbfeld image format decoder and encoder (internal).
//!
//! Farbfeld is a simple lossless format: 8-byte magic ("farbfeld"),
//! width/height as u32 big-endian, then RGBA u16 big-endian pixels.
//!
//! Implementation draws from [zune-farbfeld](https://github.com/etemesi254/zune-image)
//! by Caleb Etemesi (MIT/Apache-2.0/Zlib licensed).

pub(crate) mod decode;
mod encode;

use crate::decode::DecodeOutput;
use crate::error::BitmapError;
use crate::limits::Limits;
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Decode farbfeld data to RGBA16 pixels (native endian).
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    let (width, height) = decode::parse_header(data)?;
    if let Some(limits) = limits {
        limits.check(width, height)?;
    }
    let out_bytes = width as usize * height as usize * 8; // 4 channels × 2 bytes
    if let Some(limits) = limits {
        limits.check_memory(out_bytes)?;
    }
    stop.check()?;
    let pixels = decode::decode_pixels(data, width, height, stop)?;
    Ok(DecodeOutput::owned(
        pixels,
        width,
        height,
        PixelLayout::Rgba16,
    ))
}

/// Encode pixels as farbfeld.
pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    encode::encode_farbfeld(pixels, width, height, layout, stop)
}
