//! BMP encoder: uncompressed 24-bit and 32-bit BMP.

use crate::error::BitmapError;
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
use enough::Stop;

/// Encode pixels to BMP format.
pub(crate) fn encode_bmp(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    alpha: bool,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let w = width as usize;
    let h = height as usize;
    let expected = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(layout.bytes_per_pixel()))
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    if pixels.len() < expected {
        return Err(BitmapError::BufferTooSmall {
            needed: expected,
            actual: pixels.len(),
        });
    }

    stop.check()?;

    if alpha {
        encode_32bit(pixels, width, height, w, h, layout, stop)
    } else {
        encode_24bit(pixels, width, height, w, h, layout, stop)
    }
}

fn encode_24bit(
    pixels: &[u8],
    width: u32,
    height: u32,
    w: usize,
    h: usize,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let row_stride = w
        .checked_mul(3)
        .and_then(|r| r.checked_add(3))
        .map(|r| r & !3)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let pixel_data_size = row_stride
        .checked_mul(h)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let file_size = pixel_data_size
        .checked_add(54)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;

    let mut out = Vec::with_capacity(file_size);
    write_bmp_header(&mut out, file_size, pixel_data_size, width, height, 24);

    let pad_bytes = row_stride - w * 3;
    let is_bgr_native = matches!(layout, PixelLayout::Bgr8);
    let src_bpp = layout.bytes_per_pixel();
    for row in (0..h).rev() {
        if row % 16 == 0 {
            stop.check()?;
        }
        if is_bgr_native {
            // BGR→BMP24: already in native byte order, direct copy
            let row_start = row * w * src_bpp;
            out.extend_from_slice(&pixels[row_start..row_start + w * 3]);
        } else {
            for col in 0..w {
                let (r, g, b) = get_rgb(pixels, row * w + col, layout)?;
                out.push(b);
                out.push(g);
                out.push(r);
            }
        }
        out.extend(core::iter::repeat_n(0u8, pad_bytes));
    }

    Ok(out)
}

fn encode_32bit(
    pixels: &[u8],
    width: u32,
    height: u32,
    w: usize,
    h: usize,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let row_stride = w
        .checked_mul(4)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let pixel_data_size = row_stride
        .checked_mul(h)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let file_size = pixel_data_size
        .checked_add(54)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;

    let mut out = Vec::with_capacity(file_size);
    write_bmp_header(&mut out, file_size, pixel_data_size, width, height, 32);

    // Only Bgra8 can use the direct copy fast path. Bgrx8 must go through
    // get_rgba() which forces the padding byte to 255 (opaque).
    let is_bgra_native = matches!(layout, PixelLayout::Bgra8);
    for row in (0..h).rev() {
        if row % 16 == 0 {
            stop.check()?;
        }
        if is_bgra_native {
            // BGRA/BGRX→BMP32: already in native byte order, direct copy
            let row_start = row * w * 4;
            out.extend_from_slice(&pixels[row_start..row_start + w * 4]);
        } else {
            for col in 0..w {
                let (r, g, b, a) = get_rgba(pixels, row * w + col, layout)?;
                out.push(b);
                out.push(g);
                out.push(r);
                out.push(a);
            }
        }
    }

    Ok(out)
}

fn write_bmp_header(
    out: &mut Vec<u8>,
    file_size: usize,
    pixel_data_size: usize,
    width: u32,
    height: u32,
    bpp: u16,
) {
    // File header (14 bytes)
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 4]); // reserved
    out.extend_from_slice(&54u32.to_le_bytes()); // data offset

    // DIB header (BITMAPINFOHEADER, 40 bytes)
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes()); // positive = bottom-up
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&bpp.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // compression
    out.extend_from_slice(&(pixel_data_size as u32).to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes()); // h resolution (72 DPI)
    out.extend_from_slice(&2835u32.to_le_bytes()); // v resolution
    out.extend_from_slice(&0u32.to_le_bytes()); // colors used
    out.extend_from_slice(&0u32.to_le_bytes()); // important colors
}

fn get_rgb(pixels: &[u8], idx: usize, layout: PixelLayout) -> Result<(u8, u8, u8), BitmapError> {
    Ok(match layout {
        PixelLayout::Rgb8 => {
            let off = idx * 3;
            (pixels[off], pixels[off + 1], pixels[off + 2])
        }
        PixelLayout::Bgr8 => {
            let off = idx * 3;
            (pixels[off + 2], pixels[off + 1], pixels[off])
        }
        PixelLayout::Rgba8 => {
            let off = idx * 4;
            (pixels[off], pixels[off + 1], pixels[off + 2])
        }
        PixelLayout::Bgra8 | PixelLayout::Bgrx8 => {
            let off = idx * 4;
            (pixels[off + 2], pixels[off + 1], pixels[off])
        }
        PixelLayout::Gray8 => {
            let g = pixels[idx];
            (g, g, g)
        }
        _ => {
            return Err(BitmapError::UnsupportedVariant(alloc::format!(
                "cannot get RGB from {:?}",
                layout
            )));
        }
    })
}

fn get_rgba(
    pixels: &[u8],
    idx: usize,
    layout: PixelLayout,
) -> Result<(u8, u8, u8, u8), BitmapError> {
    Ok(match layout {
        PixelLayout::Rgba8 => {
            let off = idx * 4;
            (
                pixels[off],
                pixels[off + 1],
                pixels[off + 2],
                pixels[off + 3],
            )
        }
        PixelLayout::Bgra8 => {
            let off = idx * 4;
            (
                pixels[off + 2],
                pixels[off + 1],
                pixels[off],
                pixels[off + 3],
            )
        }
        PixelLayout::Bgrx8 => {
            let off = idx * 4;
            (pixels[off + 2], pixels[off + 1], pixels[off], 255)
        }
        PixelLayout::Rgb8 => {
            let off = idx * 3;
            (pixels[off], pixels[off + 1], pixels[off + 2], 255)
        }
        PixelLayout::Bgr8 => {
            let off = idx * 3;
            (pixels[off + 2], pixels[off + 1], pixels[off], 255)
        }
        PixelLayout::Gray8 => {
            let g = pixels[idx];
            (g, g, g, 255)
        }
        _ => {
            return Err(BitmapError::UnsupportedVariant(alloc::format!(
                "cannot get RGBA from {:?}",
                layout
            )));
        }
    })
}
