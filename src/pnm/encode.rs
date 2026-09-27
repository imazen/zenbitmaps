//! PNM encoder: P5, P6, P7, PFM.
//!
//! Credits: Draws from zune-ppm by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use super::PnmFormat;
use crate::error::{BitmapError, UnsupportedKind};
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
) -> crate::Result<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let expected = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(layout.bytes_per_pixel()))
        .ok_or_else(|| whereat::at!(BitmapError::DimensionsTooLarge { width, height }))?;
    if pixels.len() < expected {
        return Err(whereat::at!(BitmapError::BufferTooSmall {
            needed: expected,
            actual: pixels.len(),
        }));
    }

    stop.check()
        .map_err(|r| whereat::at!(BitmapError::from(r)))?;

    match fmt {
        PnmFormat::Pbm => Err(whereat::at!(BitmapError::UnsupportedVariant(
            UnsupportedKind::Type,
            "PBM encode not supported, use PGM (encode_pgm)".into(),
        ))),
        PnmFormat::Pgm => encode_pgm(pixels, width, height, w, h, layout, stop),
        PnmFormat::Ppm => encode_ppm(pixels, width, height, w, h, layout, stop),
        PnmFormat::Pam => encode_pam(pixels, width, height, w, h, layout, stop),
        PnmFormat::Pfm => encode_pfm(pixels, width, height, w, h, layout, stop),
    }
}

/// Copy `src` into `out` in bounded chunks, polling `stop` between chunks.
///
/// A one-shot `extend_from_slice` of a 100+ MiB payload is an uninterruptible
/// allocation+memcpy span. Use byte-sized bands so narrow images do not
/// trigger thousands of tiny copies and wide rows still have bounded bands.
fn extend_copy_polled(
    out: &mut Vec<u8>,
    src: &[u8],
    row_bytes: usize,
    stop: &dyn Stop,
) -> crate::Result<()> {
    let band = row_bytes.saturating_mul(16).clamp(64 * 1024, 1024 * 1024);
    for chunk in src.chunks(band) {
        stop.check()
            .map_err(|r| whereat::at!(BitmapError::from(r)))?;
        out.extend_from_slice(chunk);
    }
    Ok(())
}

fn encode_pgm(
    pixels: &[u8],
    width: u32,
    height: u32,
    w: usize,
    h: usize,
    layout: PixelLayout,
    stop: &dyn Stop,
) -> crate::Result<Vec<u8>> {
    let header = format!("P5\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + w * h);
    out.extend_from_slice(header.as_bytes());

    match layout {
        PixelLayout::Gray8 => {
            extend_copy_polled(&mut out, &pixels[..w * h], w, stop)?;
        }
        PixelLayout::Rgb8 => {
            for i in 0..(w * h) {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
                }
                let off = i * 4;
                let b = pixels[off] as u32;
                let g = pixels[off + 1] as u32;
                let r = pixels[off + 2] as u32;
                out.push(((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8);
            }
        }
        _ => {
            return Err(whereat::at!(BitmapError::UnsupportedVariant(
                UnsupportedKind::Feature,
                format!("cannot encode {:?} as PGM", layout)
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
) -> crate::Result<Vec<u8>> {
    let header = format!("P6\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + w * h * 3);
    out.extend_from_slice(header.as_bytes());

    match layout {
        PixelLayout::Rgb8 => {
            extend_copy_polled(&mut out, &pixels[..w * h * 3], w * 3, stop)?;
        }
        PixelLayout::Bgr8 => {
            // Row-at-a-time slice writes instead of per-pixel `Vec::push`.
            // BGR -> RGB, routed through garb when the `simd` feature is on.
            // Kept per-row so the every-16-rows cancellation check survives.
            for row in 0..h {
                if row % 16 == 0 {
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
                }
                let s = out.len();
                out.resize(s + w * 3, 0);
                let src = &pixels[row * w * 3..(row + 1) * w * 3];
                crate::swizzle::bgr_to_rgb_into(src, &mut out[s..]);
            }
        }
        PixelLayout::Rgba8 => {
            // Row-at-a-time slice writes instead of per-pixel `Vec::push`.
            // RGBA -> RGB (drop alpha).
            // Kept per-row so the every-16-rows cancellation check survives.
            for row in 0..h {
                if row % 16 == 0 {
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
                }
                let s = out.len();
                out.resize(s + w * 3, 0);
                let src = &pixels[row * w * 4..(row + 1) * w * 4];
                crate::swizzle::rgba_to_rgb_into(src, &mut out[s..]);
            }
        }
        PixelLayout::Bgra8 | PixelLayout::Bgrx8 => {
            // Row-at-a-time slice writes instead of per-pixel `Vec::push`.
            // BGRA -> RGB (swap R/B, drop alpha).
            // Kept per-row so the every-16-rows cancellation check survives.
            for row in 0..h {
                if row % 16 == 0 {
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
                }
                let s = out.len();
                out.resize(s + w * 3, 0);
                let src = &pixels[row * w * 4..(row + 1) * w * 4];
                crate::swizzle::bgra_to_rgb_into(src, &mut out[s..]);
            }
        }
        PixelLayout::Gray8 => {
            // Each gray sample is independent. Bound the work in pixels,
            // including narrow images where a row band would be tiny.
            let band = w.saturating_mul(16).clamp(16 * 1024, 256 * 1024);
            for src_band in pixels[..w * h].chunks(band) {
                stop.check()
                    .map_err(|r| whereat::at!(BitmapError::from(r)))?;
                let s = out.len();
                out.resize(s + src_band.len() * 3, 0);
                crate::swizzle::gray_to_rgb_into(src_band, &mut out[s..]);
            }
        }
        _ => {
            return Err(whereat::at!(BitmapError::UnsupportedVariant(
                UnsupportedKind::Feature,
                format!("cannot encode {:?} as PPM", layout)
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
) -> crate::Result<Vec<u8>> {
    let (depth, tupltype, maxval) = match layout {
        PixelLayout::Gray8 => (1, "GRAYSCALE", 255),
        PixelLayout::Gray16 => (1, "GRAYSCALE", 65535),
        PixelLayout::Rgb8 => (3, "RGB", 255),
        PixelLayout::Rgba8 => (4, "RGB_ALPHA", 255),
        PixelLayout::Bgr8 => (3, "RGB", 255),
        PixelLayout::Bgra8 => (4, "RGB_ALPHA", 255),
        PixelLayout::Bgrx8 => (4, "RGB_ALPHA", 255),
        _ => {
            return Err(whereat::at!(BitmapError::UnsupportedVariant(
                UnsupportedKind::Feature,
                format!("cannot encode {:?} as PAM", layout)
            )));
        }
    };

    let header = format!(
        "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH {depth}\nMAXVAL {maxval}\nTUPLTYPE {tupltype}\nENDHDR\n"
    );

    let pixel_count = w * h;
    // Capacity by byte width, not channel count: `Gray16` has DEPTH 1 but two
    // bytes per pixel, so `pixel_count * depth` would under-allocate by half.
    let out_bytes = pixel_count * layout.bytes_per_pixel();
    let mut out = Vec::with_capacity(header.len() + out_bytes);
    out.extend_from_slice(header.as_bytes());

    match layout {
        PixelLayout::Bgr8 => {
            // Swizzle BGR → RGB
            for i in 0..pixel_count {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
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
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
                }
                let off = i * 4;
                out.push(pixels[off + 2]); // R
                out.push(pixels[off + 1]); // G
                out.push(pixels[off]); // B
                out.push(255); // A (opaque)
            }
        }
        PixelLayout::Gray16 => {
            // 16-bit samples are stored big-endian on disk (PAM spec); `Gray16`
            // is native-endian in memory (issue #12). Convert native → big-endian,
            // mirroring the decode path (`decode_integer_transform`) and farbfeld
            // so `decode → encode_pam → decode` stays pixel-lossless and the
            // on-disk bytes are spec-compliant. A no-op on big-endian hosts.
            for i in 0..pixel_count {
                if i % w.saturating_mul(16).max(1) == 0 {
                    stop.check()
                        .map_err(|r| whereat::at!(BitmapError::from(r)))?;
                }
                let off = i * 2;
                let val = u16::from_ne_bytes([pixels[off], pixels[off + 1]]);
                out.extend_from_slice(&val.to_be_bytes());
            }
        }
        _ => {
            // Direct copy for native-order formats, banded so a 100+ MiB
            // payload stays cancellable.
            let pixel_bytes = pixel_count * layout.bytes_per_pixel();
            extend_copy_polled(
                &mut out,
                &pixels[..pixel_bytes],
                w * layout.bytes_per_pixel(),
                stop,
            )?;
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
) -> crate::Result<Vec<u8>> {
    let (magic, depth) = match layout {
        PixelLayout::GrayF32 => ("Pf", 1),
        PixelLayout::RgbF32 => ("PF", 3),
        _ => {
            return Err(whereat::at!(BitmapError::UnsupportedVariant(
                UnsupportedKind::Feature,
                format!("PFM requires GrayF32 or RgbF32, got {:?}", layout)
            )));
        }
    };

    let header = format!("{magic}\n{width} {height}\n-1.0\n");
    let row_bytes = w
        .checked_mul(depth)
        .and_then(|wd| wd.checked_mul(4))
        .ok_or_else(|| whereat::at!(BitmapError::DimensionsTooLarge { width, height }))?;
    let total_pixels = h
        .checked_mul(row_bytes)
        .ok_or_else(|| whereat::at!(BitmapError::DimensionsTooLarge { width, height }))?;
    let mut out = Vec::with_capacity(header.len().saturating_add(total_pixels));
    out.extend_from_slice(header.as_bytes());

    // PFM stores bottom-to-top
    for row in (0..h).rev() {
        if row % 16 == 0 {
            stop.check()
                .map_err(|r| whereat::at!(BitmapError::from(r)))?;
        }
        let start = row * row_bytes;
        out.extend_from_slice(&pixels[start..start + row_bytes]);
    }

    Ok(out)
}

#[cfg(test)]
mod copy_band_tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    struct Count(AtomicUsize);
    impl Stop for Count {
        fn check(&self) -> Result<(), enough::StopReason> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[test]
    fn narrow_images_use_bulk_copies() {
        let input = vec![19; 64 * 1024];
        let mut output = Vec::with_capacity(input.len());
        let stop = Count(AtomicUsize::new(0));
        extend_copy_polled(&mut output, &input, 1, &stop).unwrap();
        assert_eq!(output, input);
        assert_eq!(stop.0.load(Ordering::Relaxed), 1);
    }
}
