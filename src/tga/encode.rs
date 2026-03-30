//! TGA (Targa) encoder.
//!
//! Writes uncompressed TGA (type 2 for RGB/RGBA, type 3 for grayscale).
//! Output uses bottom-left origin (TGA default).

use alloc::vec::Vec;
use enough::Stop;

use crate::error::BitmapError;
use crate::pixel::PixelLayout;

/// Encode pixels to uncompressed TGA format.
///
/// Accepts `Gray8`, `Rgb8`, `Rgba8`, `Bgr8`, `Bgra8` input layouts.
/// Gray8 encodes as type 3 (grayscale), all others as type 2 (truecolor).
/// Output uses bottom-left origin (TGA default convention).
pub(crate) fn encode_tga(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    let w = width as usize;
    let h = height as usize;
    let bpp = layout.bytes_per_pixel();

    let expected = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(bpp))
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    if pixels.len() < expected {
        return Err(BitmapError::BufferTooSmall {
            needed: expected,
            actual: pixels.len(),
        });
    }

    // Validate width/height fit in u16
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err(BitmapError::DimensionsTooLarge { width, height });
    }

    // Determine output pixel depth and image type
    let (image_type, out_depth, out_bpp): (u8, u8, usize) = match layout {
        PixelLayout::Gray8 => (3, 8, 1),
        PixelLayout::Rgb8 | PixelLayout::Bgr8 => (2, 24, 3),
        PixelLayout::Rgba8 | PixelLayout::Bgra8 => (2, 32, 4),
        _ => {
            return Err(BitmapError::UnsupportedVariant(alloc::format!(
                "cannot encode {:?} as TGA (supported: Gray8, Rgb8, Rgba8, Bgr8, Bgra8)",
                layout
            )));
        }
    };

    // Output size: 18 header + w * h * out_bpp
    let pixel_bytes = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(out_bpp))
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;
    let total = pixel_bytes
        .checked_add(18)
        .ok_or(BitmapError::DimensionsTooLarge { width, height })?;

    let mut out = Vec::with_capacity(total);

    // Write 18-byte TGA header
    out.push(0); // id_length
    out.push(0); // color_map_type
    out.push(image_type);
    out.extend_from_slice(&[0, 0]); // color_map_start
    out.extend_from_slice(&[0, 0]); // color_map_length
    out.push(0); // color_map_depth
    out.extend_from_slice(&[0, 0]); // x_origin
    out.extend_from_slice(&[0, 0]); // y_origin
    out.extend_from_slice(&(width as u16).to_le_bytes());
    out.extend_from_slice(&(height as u16).to_le_bytes());
    out.push(out_depth); // pixel_depth
    let alpha_bits: u8 = if out_depth == 32 { 8 } else { 0 };
    out.push(alpha_bits); // descriptor: alpha bits, origin=bottom-left (bit 5=0)

    stop.check()?;

    // Write pixel data bottom-to-top (TGA default origin is bottom-left)
    for y_inv in 0..h {
        let y = h - 1 - y_inv;
        if y_inv % 16 == 0 {
            stop.check()?;
        }
        let row_start = y * w * bpp;

        match layout {
            PixelLayout::Gray8 => {
                // Direct copy
                out.extend_from_slice(&pixels[row_start..row_start + w]);
            }
            PixelLayout::Rgb8 => {
                // RGB → BGR
                #[cfg(feature = "simd")]
                {
                    let start = out.len();
                    out.extend_from_slice(&pixels[row_start..row_start + w * 3]);
                    let _ = garb::bytes::rgb_to_bgr_inplace(&mut out[start..]);
                }
                #[cfg(not(feature = "simd"))]
                for x in 0..w {
                    let off = row_start + x * 3;
                    out.push(pixels[off + 2]); // B
                    out.push(pixels[off + 1]); // G
                    out.push(pixels[off]); // R
                }
            }
            PixelLayout::Rgba8 => {
                // RGBA → BGRA
                #[cfg(feature = "simd")]
                {
                    let start = out.len();
                    out.extend_from_slice(&pixels[row_start..row_start + w * 4]);
                    let _ = garb::bytes::rgba_to_bgra_inplace(&mut out[start..]);
                }
                #[cfg(not(feature = "simd"))]
                for x in 0..w {
                    let off = row_start + x * 4;
                    out.push(pixels[off + 2]); // B
                    out.push(pixels[off + 1]); // G
                    out.push(pixels[off]); // R
                    out.push(pixels[off + 3]); // A
                }
            }
            PixelLayout::Bgr8 => {
                // Already in BGR order — direct copy
                out.extend_from_slice(&pixels[row_start..row_start + w * 3]);
            }
            PixelLayout::Bgra8 => {
                // Already in BGRA order — direct copy
                out.extend_from_slice(&pixels[row_start..row_start + w * 4]);
            }
            _ => unreachable!(), // validated above
        }
    }

    Ok(out)
}
