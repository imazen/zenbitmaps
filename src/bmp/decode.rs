//! BMP decoder: uncompressed 24-bit and 32-bit BMP.

use crate::error::PnmError;
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Parse BMP header, returning (width, height, layout).
pub(crate) fn parse_bmp_header(data: &[u8]) -> Result<(u32, u32, PixelLayout), PnmError> {
    if data.len() < 54 {
        return Err(PnmError::UnexpectedEof);
    }
    if &data[0..2] != b"BM" {
        return Err(PnmError::UnrecognizedFormat);
    }

    let width_raw = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);

    if width_raw <= 0 {
        return Err(PnmError::InvalidHeader(alloc::format!(
            "BMP width must be positive, got {width_raw}"
        )));
    }
    if height_raw == 0 {
        return Err(PnmError::InvalidHeader("BMP height cannot be zero".into()));
    }

    let width = width_raw as u32;
    let height = height_raw.unsigned_abs();

    let bits_per_pixel = u16::from_le_bytes([data[28], data[29]]);

    let layout = match bits_per_pixel {
        24 => PixelLayout::Rgb8,
        32 => PixelLayout::Rgba8,
        other => {
            return Err(PnmError::UnsupportedVariant(alloc::format!(
                "BMP {other}-bit not supported (only 24/32)"
            )));
        }
    };

    Ok((width, height, layout))
}

/// Decode BMP pixel data, handling BGR→RGB, row flipping, and padding.
pub(crate) fn decode_bmp_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, PnmError> {
    let data_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let top_down = height_raw < 0;
    let compression = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);

    if compression != 0 {
        return Err(PnmError::UnsupportedVariant(alloc::format!(
            "BMP compression type {compression} not supported"
        )));
    }

    if data_offset < 54 || data_offset > data.len() {
        return Err(PnmError::UnexpectedEof);
    }

    let pixel_data = &data[data_offset..];
    let w = width as usize;
    let h = height as usize;

    match layout {
        PixelLayout::Rgb8 => decode_24bit(pixel_data, w, h, top_down, stop),
        PixelLayout::Rgba8 => decode_32bit(pixel_data, w, h, top_down, stop),
        _ => {
            Err(PnmError::UnsupportedVariant(alloc::format!(
                "BMP layout {:?} not supported in pixel decoder",
                layout
            )))
        }
    }
}

fn decode_24bit(
    pixel_data: &[u8],
    w: usize,
    h: usize,
    top_down: bool,
    stop: &dyn Stop,
) -> Result<Vec<u8>, PnmError> {
    let row_stride = w
        .checked_mul(3)
        .and_then(|r| r.checked_add(3))
        .map(|r| r & !3)
        .ok_or(PnmError::DimensionsTooLarge {
            width: w as u32,
            height: h as u32,
        })?;
    let needed = row_stride
        .checked_mul(h)
        .ok_or(PnmError::DimensionsTooLarge {
            width: w as u32,
            height: h as u32,
        })?;
    if pixel_data.len() < needed {
        return Err(PnmError::UnexpectedEof);
    }

    let out_size =
        w.checked_mul(h)
            .and_then(|wh| wh.checked_mul(3))
            .ok_or(PnmError::DimensionsTooLarge {
                width: w as u32,
                height: h as u32,
            })?;
    let mut out = Vec::with_capacity(out_size);
    for row in 0..h {
        if row % 16 == 0 {
            stop.check()?;
        }
        let src_row = if top_down { row } else { h - 1 - row };
        let row_start = src_row * row_stride;
        for col in 0..w {
            let off = row_start + col * 3;
            out.push(pixel_data[off + 2]); // R
            out.push(pixel_data[off + 1]); // G
            out.push(pixel_data[off]); // B
        }
    }

    Ok(out)
}

fn decode_32bit(
    pixel_data: &[u8],
    w: usize,
    h: usize,
    top_down: bool,
    stop: &dyn Stop,
) -> Result<Vec<u8>, PnmError> {
    let row_stride = w.checked_mul(4).ok_or(PnmError::DimensionsTooLarge {
        width: w as u32,
        height: h as u32,
    })?;
    let needed = row_stride
        .checked_mul(h)
        .ok_or(PnmError::DimensionsTooLarge {
            width: w as u32,
            height: h as u32,
        })?;
    if pixel_data.len() < needed {
        return Err(PnmError::UnexpectedEof);
    }

    let out_size =
        w.checked_mul(h)
            .and_then(|wh| wh.checked_mul(4))
            .ok_or(PnmError::DimensionsTooLarge {
                width: w as u32,
                height: h as u32,
            })?;
    let mut out = Vec::with_capacity(out_size);
    for row in 0..h {
        if row % 16 == 0 {
            stop.check()?;
        }
        let src_row = if top_down { row } else { h - 1 - row };
        let row_start = src_row * row_stride;
        for col in 0..w {
            let off = row_start + col * 4;
            out.push(pixel_data[off + 2]); // R
            out.push(pixel_data[off + 1]); // G
            out.push(pixel_data[off]); // B
            out.push(pixel_data[off + 3]); // A
        }
    }

    Ok(out)
}
