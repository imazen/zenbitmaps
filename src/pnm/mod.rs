//! PNM family: P5 (PGM), P6 (PPM), P7 (PAM), PFM.
//!
//! Credits: Implementation draws from [zune-ppm](https://github.com/etemesi254/zune-image)
//! by Caleb Etemesi (MIT/Apache-2.0/Zlib licensed).

pub(crate) mod decode;
mod encode;

use crate::decode::DecodeOutput;
use crate::error::BitmapError;
use crate::limits::Limits;
use crate::pixel::PixelLayout;
use enough::Stop;

/// Which PNM sub-format to use (internal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PnmFormat {
    Pgm,
    Ppm,
    Pam,
    Pfm,
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

/// Decode PNM data (called from top-level decode functions).
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    if data.len() < 3 {
        return Err(BitmapError::UnexpectedEof);
    }

    // Verify magic bytes
    match &data[..2] {
        b"P5" | b"P6" | b"P7" | b"Pf" | b"PF" => {}
        _ => return Err(BitmapError::UnrecognizedFormat),
    }

    let header = decode::parse_header(data)?;

    if let Some(limits) = limits {
        limits.check(header.width, header.height)?;
    }

    stop.check()?;

    let pixel_data = data
        .get(header.data_offset..)
        .ok_or(BitmapError::UnexpectedEof)?;

    let w = header.width as usize;
    let h = header.height as usize;
    let depth = header.depth as usize;

    match header.format {
        PnmFormat::Pfm => {
            let out_bytes = w
                .checked_mul(h)
                .and_then(|wh| wh.checked_mul(depth))
                .and_then(|whd| whd.checked_mul(4))
                .ok_or(BitmapError::DimensionsTooLarge {
                    width: header.width,
                    height: header.height,
                })?;
            if let Some(limits) = limits {
                limits.check_memory(out_bytes)?;
            }
            let pixels = decode::decode_pfm(pixel_data, &header, stop)?;
            Ok(DecodeOutput::owned(
                pixels,
                header.width,
                header.height,
                header.layout,
            ))
        }
        _ => {
            let is_16bit = header.maxval > 255;
            let src_bps = if is_16bit { 2 } else { 1 };
            let expected_src = w
                .checked_mul(h)
                .and_then(|wh| wh.checked_mul(depth))
                .and_then(|whd| whd.checked_mul(src_bps))
                .ok_or(BitmapError::DimensionsTooLarge {
                    width: header.width,
                    height: header.height,
                })?;

            if pixel_data.len() < expected_src {
                return Err(BitmapError::UnexpectedEof);
            }

            if !is_16bit && header.maxval == 255 {
                Ok(DecodeOutput::borrowed(
                    &pixel_data[..expected_src],
                    header.width,
                    header.height,
                    header.layout,
                ))
            } else {
                let out_bytes = w
                    .checked_mul(h)
                    .and_then(|wh| wh.checked_mul(depth))
                    .ok_or(BitmapError::DimensionsTooLarge {
                        width: header.width,
                        height: header.height,
                    })?;
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
                ))
            }
        }
    }
}

/// Encode to PNM.
pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    format: PnmFormat,
    stop: &dyn Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
    encode::encode_pnm(pixels, width, height, layout, format, stop)
}
