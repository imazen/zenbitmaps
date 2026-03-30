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
    Pbm,
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
        b"P1" | b"P2" | b"P3" | b"P4" | b"P5" | b"P6" | b"P7" | b"Pf" | b"PF" => {}
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
        PnmFormat::Pbm => {
            // P1 (ASCII) or P4 (binary bit-packed)
            let is_ascii = data[1] == b'1';
            let out_bytes = w.checked_mul(h).ok_or(BitmapError::DimensionsTooLarge {
                width: header.width,
                height: header.height,
            })?;
            if let Some(limits) = limits {
                limits.check_memory(out_bytes)?;
            }
            let pixels = if is_ascii {
                decode::decode_ascii_pbm(pixel_data, &header, stop)?
            } else {
                decode::decode_p4_bitpacked(pixel_data, &header, stop)?
            };
            Ok(DecodeOutput::owned(
                pixels,
                header.width,
                header.height,
                PixelLayout::Gray8,
            ))
        }
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
        PnmFormat::Pgm | PnmFormat::Ppm => {
            let is_ascii = matches!(data[1], b'2' | b'3');
            if is_ascii {
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
                let pixels = decode::decode_ascii_samples(pixel_data, &header, stop)?;
                Ok(DecodeOutput::owned(
                    pixels,
                    header.width,
                    header.height,
                    header.layout,
                ))
            } else {
                // Binary P5/P6
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
        PnmFormat::Pam => {
            // PAM (P7) is always binary
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
