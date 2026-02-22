//! PNM encoder: P5, P6, P7, PFM.
//!
//! Credits: Draws from zune-ppm by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use super::PnmFormat;
use crate::error::BitmapError;
use crate::pixel::PixelLayout;
use alloc::format;
use alloc::vec::Vec;
use enough::Stop;

/// Encode pixels to PNM format.
pub(crate) fn encode_pnm(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    fmt: PnmFormat,
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

    match fmt {
        PnmFormat::Pgm => encode_pgm(pixels, width, height, w, h, layout, stop),
        PnmFormat::Ppm => encode_ppm(pixels, width, height, w, h, layout, stop),
        PnmFormat::Pam => encode_pam(pixels, width, height, w, h, layout, stop),
        PnmFormat::Pfm => encode_pfm(pixels, width, height, w, h, layout, stop),
    }
}

fn encode_pgm(
    pixels: &[u8],
    width: u32,
    height: u32,
    w: usize,
    h: usize,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let header = format!("P5\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + w * h);
    out.extend_from_slice(header.as_bytes());

    match layout {
        PixelLayout::Gray8 => {
            out.extend_from_slice(&pixels[..w * h]);
        }
        PixelLayout::Rgb8 => {
            for i in 0..(w * h) {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 3;
                let r = pixels[off] as u32;
                let g = pixels[off + 1] as u32;
                let b = pixels[off + 2] as u32;
                out.push(((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8);
            }
        }
        PixelLayout::Bgr8 => {
            for i in 0..(w * h) {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 3;
                let b = pixels[off] as u32;
                let g = pixels[off + 1] as u32;
                let r = pixels[off + 2] as u32;
                out.push(((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8);
            }
        }
        PixelLayout::Rgba8 => {
            for i in 0..(w * h) {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 4;
                let r = pixels[off] as u32;
                let g = pixels[off + 1] as u32;
                let b = pixels[off + 2] as u32;
                out.push(((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8);
            }
        }
        PixelLayout::Bgra8 | PixelLayout::Bgrx8 => {
            for i in 0..(w * h) {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 4;
                let b = pixels[off] as u32;
                let g = pixels[off + 1] as u32;
                let r = pixels[off + 2] as u32;
                out.push(((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8);
            }
        }
        _ => {
            return Err(BitmapError::UnsupportedVariant(format!(
                "cannot encode {:?} as PGM",
                layout
            )));
        }
    }

    Ok(out)
}

fn encode_ppm(
    pixels: &[u8],
    width: u32,
    height: u32,
    w: usize,
    h: usize,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let header = format!("P6\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + w * h * 3);
    out.extend_from_slice(header.as_bytes());

    match layout {
        PixelLayout::Rgb8 => {
            out.extend_from_slice(&pixels[..w * h * 3]);
        }
        PixelLayout::Bgr8 => {
            for i in 0..(w * h) {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 3;
                out.push(pixels[off + 2]);
                out.push(pixels[off + 1]);
                out.push(pixels[off]);
            }
        }
        PixelLayout::Rgba8 => {
            for i in 0..(w * h) {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 4;
                out.push(pixels[off]);
                out.push(pixels[off + 1]);
                out.push(pixels[off + 2]);
            }
        }
        PixelLayout::Bgra8 | PixelLayout::Bgrx8 => {
            for i in 0..(w * h) {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 4;
                out.push(pixels[off + 2]);
                out.push(pixels[off + 1]);
                out.push(pixels[off]);
            }
        }
        PixelLayout::Gray8 => {
            for &g in &pixels[..w * h] {
                out.push(g);
                out.push(g);
                out.push(g);
            }
        }
        _ => {
            return Err(BitmapError::UnsupportedVariant(format!(
                "cannot encode {:?} as PPM",
                layout
            )));
        }
    }

    Ok(out)
}

fn encode_pam(
    pixels: &[u8],
    width: u32,
    height: u32,
    w: usize,
    h: usize,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let (depth, tupltype, maxval) = match layout {
        PixelLayout::Gray8 => (1, "GRAYSCALE", 255),
        PixelLayout::Gray16 => (1, "GRAYSCALE", 65535),
        PixelLayout::Rgb8 => (3, "RGB", 255),
        PixelLayout::Rgba8 => (4, "RGB_ALPHA", 255),
        PixelLayout::Bgr8 => (3, "RGB", 255),
        PixelLayout::Bgra8 => (4, "RGB_ALPHA", 255),
        PixelLayout::Bgrx8 => (4, "RGB_ALPHA", 255),
        _ => {
            return Err(BitmapError::UnsupportedVariant(format!(
                "cannot encode {:?} as PAM",
                layout
            )));
        }
    };

    let header = format!(
        "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH {depth}\nMAXVAL {maxval}\nTUPLTYPE {tupltype}\nENDHDR\n"
    );

    let pixel_count = w * h;
    let out_bytes = pixel_count * depth;
    let mut out = Vec::with_capacity(header.len() + out_bytes);
    out.extend_from_slice(header.as_bytes());

    match layout {
        PixelLayout::Bgr8 => {
            // Swizzle BGR → RGB
            for i in 0..pixel_count {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 3;
                out.push(pixels[off + 2]); // R
                out.push(pixels[off + 1]); // G
                out.push(pixels[off]); // B
            }
        }
        PixelLayout::Bgra8 => {
            // Swizzle BGRA → RGBA
            for i in 0..pixel_count {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 4;
                out.push(pixels[off + 2]); // R
                out.push(pixels[off + 1]); // G
                out.push(pixels[off]); // B
                out.push(pixels[off + 3]); // A
            }
        }
        PixelLayout::Bgrx8 => {
            // Swizzle BGRX → RGBA (A=255)
            for i in 0..pixel_count {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()?;
                }
                let off = i * 4;
                out.push(pixels[off + 2]); // R
                out.push(pixels[off + 1]); // G
                out.push(pixels[off]); // B
                out.push(255); // A (opaque)
            }
        }
        _ => {
            // Direct copy for native-order formats
            let pixel_bytes = pixel_count * layout.bytes_per_pixel();
            out.extend_from_slice(&pixels[..pixel_bytes]);
        }
    }

    Ok(out)
}

fn encode_pfm(
    pixels: &[u8],
    width: u32,
    height: u32,
    w: usize,
    h: usize,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let (magic, depth) = match layout {
        PixelLayout::GrayF32 => ("Pf", 1),
        PixelLayout::RgbF32 => ("PF", 3),
        _ => {
            return Err(BitmapError::UnsupportedVariant(format!(
                "PFM requires GrayF32 or RgbF32, got {:?}",
                layout
            )));
        }
    };

    let header = format!("{magic}\n{width} {height}\n-1.0\n");
    let row_bytes = w
        .checked_mul(depth)
        .and_then(|wd| wd.checked_mul(4))
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let total_pixels = h
        .checked_mul(row_bytes)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let mut out = Vec::with_capacity(header.len().saturating_add(total_pixels));
    out.extend_from_slice(header.as_bytes());

    // PFM stores bottom-to-top
    for row in (0..h).rev() {
        if row % 16 == 0 {
            stop.check()?;
        }
        let start = row * row_bytes;
        out.extend_from_slice(&pixels[start..start + row_bytes]);
    }

    Ok(out)
}
