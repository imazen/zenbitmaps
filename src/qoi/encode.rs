//! QOI encoder.
//!
//! Uses [rapid-qoi](https://github.com/zakarumych/rapid-qoi) for encoding.

use alloc::vec::Vec;
use enough::Stop;

use crate::error::BitmapError;
use crate::pixel::PixelLayout;

/// Encode pixels as QOI format.
///
/// Accepts `Rgb8`, `Rgba8`, `Bgr8` (swizzled to RGB), `Bgra8` (swizzled to RGBA).
pub(crate) fn encode_qoi(
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

    stop.check()?;

    // Determine QOI color space and prepare pixel data
    let (qoi_pixels, colors) = match layout {
        PixelLayout::Rgb8 => (None, rapid_qoi::Colors::Srgb),
        PixelLayout::Rgba8 => (None, rapid_qoi::Colors::SrgbLinA),
        PixelLayout::Bgr8 => {
            // Swizzle BGR → RGB
            let mut rgb = Vec::with_capacity(expected);
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 3).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check()?;
                }
                for pixel in row.chunks_exact(3) {
                    rgb.push(pixel[2]);
                    rgb.push(pixel[1]);
                    rgb.push(pixel[0]);
                }
            }
            (Some(rgb), rapid_qoi::Colors::Srgb)
        }
        PixelLayout::Bgra8 | PixelLayout::Bgrx8 => {
            // Swizzle BGRA → RGBA
            let mut rgba = Vec::with_capacity(w * h * 4);
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 4).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check()?;
                }
                for pixel in row.chunks_exact(4) {
                    rgba.push(pixel[2]);
                    rgba.push(pixel[1]);
                    rgba.push(pixel[0]);
                    rgba.push(if matches!(layout, PixelLayout::Bgrx8) {
                        255
                    } else {
                        pixel[3]
                    });
                }
            }
            (Some(rgba), rapid_qoi::Colors::SrgbLinA)
        }
        _ => {
            return Err(BitmapError::UnsupportedVariant(alloc::format!(
                "cannot encode {layout:?} as QOI (supported: Rgb8, Rgba8, Bgr8, Bgra8)"
            )));
        }
    };

    let qoi = rapid_qoi::Qoi {
        width,
        height,
        colors,
    };

    let encode_data = qoi_pixels.as_deref().unwrap_or(&pixels[..expected]);
    let encoded = qoi
        .encode_alloc(encode_data)
        .map_err(|e| BitmapError::InvalidData(e.to_string()))?;

    Ok(encoded)
}
