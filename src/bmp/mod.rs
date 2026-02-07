//! BMP image format decoder and encoder.
//!
//! Supports uncompressed BMP with 24-bit (RGB) and 32-bit (RGBA) pixel data.
//! RLE and indexed color are not yet supported.

mod decode;
mod encode;

pub use decode::BmpDecoder;
pub use encode::BmpEncoder;

use crate::decode::DecodeOutput;
use crate::error::PnmError;
use crate::info::{BitmapFormat, ImageInfo};
use crate::limits::Limits;
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Decoded BMP output (standalone).
#[derive(Clone, Debug)]
pub struct BmpOutput {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub layout: PixelLayout,
}

/// Probe BMP header for ImageInfo.
pub(crate) fn probe_header(data: &[u8]) -> Result<ImageInfo, PnmError> {
    let (width, height, layout) = decode::parse_bmp_header(data)?;
    Ok(ImageInfo {
        width,
        height,
        format: BitmapFormat::Bmp,
        native_layout: layout,
    })
}

/// Decode BMP data (called from DecodeRequest).
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, PnmError> {
    let (width, height, layout) = decode::parse_bmp_header(data)?;

    if let Some(limits) = limits {
        limits.check(width, height)?;
    }

    stop.check()?;

    let out_bytes = width as usize * height as usize * layout.bytes_per_pixel();
    if let Some(limits) = limits {
        limits.check_memory(out_bytes)?;
    }

    // BMP always needs transformation (BGR→RGB, row flipping, padding removal)
    let pixels = decode::decode_bmp_pixels(data, width, height, layout, stop)?;
    Ok(DecodeOutput::owned(
        pixels,
        width,
        height,
        layout,
        BitmapFormat::Bmp,
    ))
}

/// Encode to BMP (called from EncodeRequest).
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
