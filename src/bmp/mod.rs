//! Full BMP image format decoder and basic encoder (internal).
//!
//! Use top-level [`crate::decode_bmp`], [`crate::encode_bmp`], etc.

mod decode;
mod encode;
mod utils;

use crate::decode::DecodeOutput;
use crate::error::PnmError;
use crate::limits::Limits;
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Decode BMP data (output in RGB/RGBA byte order).
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, PnmError> {
    let header = decode::parse_bmp_header(data)?;
    check_limits(limits, header.width, header.height, &header.layout)?;
    stop.check()?;
    let (pixels, layout) = decode::decode_bmp_pixels(data, stop)?;
    Ok(DecodeOutput::owned(pixels, header.width, header.height, layout))
}

/// Decode BMP data in native byte order (BGR/BGRA — no channel swizzle).
pub(crate) fn decode_native<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, PnmError> {
    let header = decode::parse_bmp_header(data)?;
    check_limits(limits, header.width, header.height, &header.layout)?;
    stop.check()?;
    let (pixels, native_layout) = decode::decode_bmp_pixels_native(data, stop)?;
    Ok(DecodeOutput::owned(pixels, header.width, header.height, native_layout))
}

fn check_limits(
    limits: Option<&Limits>,
    width: u32,
    height: u32,
    layout: &PixelLayout,
) -> Result<(), PnmError> {
    if let Some(limits) = limits {
        limits.check(width, height)?;
    }
    let out_bytes = width as usize * height as usize * layout.bytes_per_pixel();
    if let Some(limits) = limits {
        limits.check_memory(out_bytes)?;
    }
    Ok(())
}

/// Encode to BMP.
pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    alpha: bool,
    stop: &dyn Stop,
) -> Result<Vec<u8>, PnmError> {
    encode::encode_bmp(pixels, width, height, layout, alpha, stop)
}
