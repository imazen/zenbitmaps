//! Farbfeld encoder.
//!
//! Forked from zune-farbfeld 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use alloc::vec::Vec;
use enough::Stop;

use crate::error::PnmError;
use crate::pixel::PixelLayout;

/// Encode pixels to farbfeld format.
///
/// Accepts `Rgba16` (direct), `Rgba8` (expand via `val * 257`),
/// or `Rgb8` (expand + alpha=65535).
pub(crate) fn encode_farbfeld(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, PnmError> {
    let w = width as usize;
    let h = height as usize;
    let bpp = layout.bytes_per_pixel();
    let expected = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(bpp))
        .ok_or(PnmError::DimensionsTooLarge { width, height })?;
    if pixels.len() < expected {
        return Err(PnmError::BufferTooSmall {
            needed: expected,
            actual: pixels.len(),
        });
    }

    // Output: 16 header + w*h*8 pixel bytes
    let pixel_bytes = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(8))
        .ok_or(PnmError::DimensionsTooLarge { width, height })?;
    let total = pixel_bytes
        .checked_add(16)
        .ok_or(PnmError::DimensionsTooLarge { width, height })?;

    let mut out = Vec::with_capacity(total);

    // Header
    out.extend_from_slice(b"farbfeld");
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());

    stop.check()?;

    match layout {
        PixelLayout::Rgba16 => {
            // Native endian u16 → big endian u16
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 8).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check()?;
                }
                for pair in row.chunks_exact(2) {
                    let val = u16::from_ne_bytes([pair[0], pair[1]]);
                    out.extend_from_slice(&val.to_be_bytes());
                }
            }
        }
        PixelLayout::Rgba8 => {
            // Expand u8 → u16 via val * 257
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 4).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check()?;
                }
                for &byte in row {
                    let val: u16 = byte as u16 * 257;
                    out.extend_from_slice(&val.to_be_bytes());
                }
            }
        }
        PixelLayout::Rgb8 => {
            // Expand RGB u8 → RGBA u16 (alpha = 65535)
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 3).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check()?;
                }
                for pixel in row.chunks_exact(3) {
                    let r: u16 = pixel[0] as u16 * 257;
                    let g: u16 = pixel[1] as u16 * 257;
                    let b: u16 = pixel[2] as u16 * 257;
                    out.extend_from_slice(&r.to_be_bytes());
                    out.extend_from_slice(&g.to_be_bytes());
                    out.extend_from_slice(&b.to_be_bytes());
                    out.extend_from_slice(&65535u16.to_be_bytes());
                }
            }
        }
        PixelLayout::Gray8 => {
            // Expand gray u8 → RGBA u16 (R=G=B=gray, alpha=65535)
            for (row_idx, row) in pixels[..expected].chunks_exact(w).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check()?;
                }
                for &byte in row {
                    let val: u16 = byte as u16 * 257;
                    out.extend_from_slice(&val.to_be_bytes());
                    out.extend_from_slice(&val.to_be_bytes());
                    out.extend_from_slice(&val.to_be_bytes());
                    out.extend_from_slice(&65535u16.to_be_bytes());
                }
            }
        }
        _ => {
            return Err(PnmError::UnsupportedVariant(alloc::format!(
                "cannot encode {:?} as farbfeld (supported: Rgba16, Rgba8, Rgb8, Gray8)",
                layout
            )));
        }
    }

    Ok(out)
}
