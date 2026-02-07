//! PNM encoder: P5, P6, P7, PFM.
//!
//! Credits: Draws from zune-ppm by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use super::PnmFormat;
use crate::error::PnmError;
use crate::pixel::PixelLayout;
use alloc::format;
use alloc::vec::Vec;
use enough::Stop;

/// PNM encoder (standalone, without the unified EncodeRequest).
///
/// For most uses, prefer [`crate::EncodeRequest::pnm`].
pub struct PnmEncoder {
    format: PnmFormat,
}

impl PnmEncoder {
    pub fn new(format: PnmFormat) -> Self {
        Self { format }
    }

    /// Encode pixels to PNM bytes.
    pub fn encode(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        layout: PixelLayout,
    ) -> Result<Vec<u8>, PnmError> {
        encode_pnm(
            pixels,
            width,
            height,
            layout,
            self.format,
            &enough::Unstoppable,
        )
    }
}

/// Internal encode used by both PnmEncoder and EncodeRequest.
pub(crate) fn encode_pnm(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    fmt: PnmFormat,
    stop: &dyn Stop,
) -> Result<Vec<u8>, PnmError> {
    let w = width as usize;
    let h = height as usize;
    let expected = w * h * layout.bytes_per_pixel();
    if pixels.len() < expected {
        return Err(PnmError::BufferTooSmall {
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
) -> Result<Vec<u8>, PnmError> {
    let header = format!("P5\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + w * h);
    out.extend_from_slice(header.as_bytes());

    match layout {
        PixelLayout::Gray8 => {
            out.extend_from_slice(&pixels[..w * h]);
        }
        PixelLayout::Rgb8 | PixelLayout::Bgr8 => {
            let bpp = 3;
            for i in 0..(w * h) {
                if i % (w * 16) == 0 {
                    stop.check()?;
                }
                let r = pixels[i * bpp] as u32;
                let g = pixels[i * bpp + 1] as u32;
                let b = pixels[i * bpp + 2] as u32;
                out.push(((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8);
            }
        }
        PixelLayout::Rgba8 | PixelLayout::Bgra8 => {
            let bpp = 4;
            for i in 0..(w * h) {
                if i % (w * 16) == 0 {
                    stop.check()?;
                }
                let r = pixels[i * bpp] as u32;
                let g = pixels[i * bpp + 1] as u32;
                let b = pixels[i * bpp + 2] as u32;
                out.push(((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8);
            }
        }
        _ => {
            return Err(PnmError::UnsupportedVariant(format!(
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
) -> Result<Vec<u8>, PnmError> {
    let header = format!("P6\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + w * h * 3);
    out.extend_from_slice(header.as_bytes());

    match layout {
        PixelLayout::Rgb8 => {
            out.extend_from_slice(&pixels[..w * h * 3]);
        }
        PixelLayout::Bgr8 => {
            for i in 0..(w * h) {
                if i % (w * 16) == 0 {
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
                if i % (w * 16) == 0 {
                    stop.check()?;
                }
                let off = i * 4;
                out.push(pixels[off]);
                out.push(pixels[off + 1]);
                out.push(pixels[off + 2]);
            }
        }
        PixelLayout::Bgra8 => {
            for i in 0..(w * h) {
                if i % (w * 16) == 0 {
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
            return Err(PnmError::UnsupportedVariant(format!(
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
    _stop: &dyn Stop,
) -> Result<Vec<u8>, PnmError> {
    let (depth, tupltype, maxval) = match layout {
        PixelLayout::Gray8 => (1, "GRAYSCALE", 255),
        PixelLayout::Gray16 => (1, "GRAYSCALE", 65535),
        PixelLayout::Rgb8 => (3, "RGB", 255),
        PixelLayout::Rgba8 => (4, "RGB_ALPHA", 255),
        _ => {
            return Err(PnmError::UnsupportedVariant(format!(
                "cannot encode {:?} as PAM directly; convert to RGB/RGBA first",
                layout
            )));
        }
    };

    let header = format!(
        "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH {depth}\nMAXVAL {maxval}\nTUPLTYPE {tupltype}\nENDHDR\n"
    );

    let pixel_bytes = w * h * layout.bytes_per_pixel();
    let mut out = Vec::with_capacity(header.len() + pixel_bytes);
    out.extend_from_slice(header.as_bytes());
    // PAM is a direct copy of the pixel data — zero transformation
    out.extend_from_slice(&pixels[..pixel_bytes]);

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
) -> Result<Vec<u8>, PnmError> {
    let (magic, depth) = match layout {
        PixelLayout::GrayF32 => ("Pf", 1),
        PixelLayout::RgbF32 => ("PF", 3),
        _ => {
            return Err(PnmError::UnsupportedVariant(format!(
                "PFM requires GrayF32 or RgbF32, got {:?}",
                layout
            )));
        }
    };

    let header = format!("{magic}\n{width} {height}\n-1.0\n");
    let row_bytes = w * depth * 4;
    let mut out = Vec::with_capacity(header.len() + h * row_bytes);
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
