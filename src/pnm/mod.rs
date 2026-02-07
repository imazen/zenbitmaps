//! PNM family: P5 (PGM), P6 (PPM), P7 (PAM), PFM.
//!
//! Credits: Implementation draws from [zune-ppm](https://github.com/etemesi254/zune-image)
//! by Caleb Etemesi (MIT/Apache-2.0/Zlib licensed).

mod decode;
mod encode;

pub use decode::PnmDecoder;
pub use encode::PnmEncoder;

use crate::decode::DecodeOutput;
use crate::error::PnmError;
use crate::info::{BitmapFormat, ImageInfo};
use crate::limits::Limits;
use crate::pixel::PixelLayout;
use enough::Stop;

/// Which PNM sub-format to use.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PnmFormat {
    /// P5 — binary grayscale (PGM).
    Pgm,
    /// P6 — binary RGB (PPM).
    Ppm,
    /// P7 — PAM (arbitrary channels, with TUPLTYPE header).
    Pam,
    /// PFM — floating-point (grayscale or RGB, 32-bit float).
    Pfm,
}

impl PnmFormat {
    fn to_bitmap_format(self) -> BitmapFormat {
        match self {
            PnmFormat::Pgm => BitmapFormat::Pgm,
            PnmFormat::Ppm => BitmapFormat::Ppm,
            PnmFormat::Pam => BitmapFormat::Pam,
            PnmFormat::Pfm => BitmapFormat::Pfm,
        }
    }
}

/// Parsed PNM header (internal).
pub(crate) struct PnmHeader {
    pub format: PnmFormat,
    pub width: u32,
    pub height: u32,
    pub maxval: u32,
    pub depth: u32,
    pub layout: PixelLayout,
    pub pfm_scale: f32,
    pub data_offset: usize,
}

/// Probe header for ImageInfo without decoding.
pub(crate) fn probe_header(data: &[u8]) -> Result<ImageInfo, PnmError> {
    let header = decode::parse_header(data)?;
    Ok(ImageInfo {
        width: header.width,
        height: header.height,
        format: header.format.to_bitmap_format(),
        native_layout: header.layout,
    })
}

/// Decode PNM data (called from DecodeRequest).
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, PnmError> {
    let header = decode::parse_header(data)?;

    if let Some(limits) = limits {
        limits.check(header.width, header.height)?;
    }

    stop.check()?;

    let pixel_data = data
        .get(header.data_offset..)
        .ok_or(PnmError::UnexpectedEof)?;

    let w = header.width as usize;
    let h = header.height as usize;
    let depth = header.depth as usize;
    let bitmap_format = header.format.to_bitmap_format();

    match header.format {
        PnmFormat::Pfm => {
            // PFM always needs transformation (endian swap + row flip)
            let out_bytes = w * h * depth * 4;
            if let Some(limits) = limits {
                limits.check_memory(out_bytes)?;
            }
            let pixels = decode::decode_pfm(pixel_data, &header, stop)?;
            Ok(DecodeOutput::owned(
                pixels,
                header.width,
                header.height,
                header.layout,
                bitmap_format,
            ))
        }
        _ => {
            let is_16bit = header.maxval > 255;
            let src_bps = if is_16bit { 2 } else { 1 };
            let expected_src = w
                .checked_mul(h)
                .and_then(|wh| wh.checked_mul(depth))
                .and_then(|whd| whd.checked_mul(src_bps))
                .ok_or(PnmError::DimensionsTooLarge {
                    width: header.width,
                    height: header.height,
                })?;

            if pixel_data.len() < expected_src {
                return Err(PnmError::UnexpectedEof);
            }

            // Zero-copy path: 8-bit, maxval=255 — data is already in the right format
            if !is_16bit && header.maxval == 255 {
                Ok(DecodeOutput::borrowed(
                    &pixel_data[..expected_src],
                    header.width,
                    header.height,
                    header.layout,
                    bitmap_format,
                ))
            } else {
                // Needs transformation — allocate
                let out_bytes = w * h * depth;
                if let Some(limits) = limits {
                    limits.check_memory(out_bytes)?;
                }
                let pixels =
                    decode::decode_integer_transform(pixel_data, &header, expected_src, stop)?;
                Ok(DecodeOutput::owned(
                    pixels,
                    header.width,
                    header.height,
                    header.layout,
                    bitmap_format,
                ))
            }
        }
    }
}

/// Encode to PNM (called from EncodeRequest).
pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    format: PnmFormat,
    stop: &dyn Stop,
) -> Result<alloc::vec::Vec<u8>, PnmError> {
    encode::encode_pnm(pixels, width, height, layout, format, stop)
}
