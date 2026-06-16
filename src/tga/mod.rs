//! TGA (Targa) image format decoder and encoder (internal).
//!
//! TGA is a simple raster format supporting truecolor (BGR/BGRA),
//! grayscale, and color-mapped images with optional RLE compression.
//! No external dependencies — implemented from scratch.

pub(crate) mod decode;
mod encode;

use crate::decode::DecodeOutput;
use crate::error::BitmapError;
use crate::limits::{self, Limits};
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Decode TGA data to RGB8, RGBA8, or Gray8 pixels.
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> crate::Result<DecodeOutput<'a>> {
    let header = decode::parse_header(data)?;
    let width = header.width as u32;
    let height = header.height as u32;

    limits::check_dimensions(width, height, limits)?;

    // Estimate output size for memory limit check
    let out_channels: usize = if matches!(header.image_type, 3 | 11) {
        1
    } else if header.pixel_depth == 32
        || (matches!(header.image_type, 1 | 9) && header.color_map_depth == 32)
        || (header.descriptor & 0x0F) > 0
    {
        4
    } else {
        3
    };
    let out_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(out_channels))
        .ok_or_else(|| {
            whereat::at!(BitmapError::LimitExceeded(
                "output size overflows usize".into()
            ))
        })?;
    limits::check_output_size(out_bytes, limits)?;

    stop.check()
        .map_err(|r| whereat::at!(BitmapError::from(r)))?;

    let (pixels, layout) = decode::decode_pixels(data, &header, stop)?;
    Ok(DecodeOutput::owned(pixels, width, height, layout))
}

/// Encode pixels as TGA.
pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    encode::encode_tga(pixels, width, height, layout, stop)
}
