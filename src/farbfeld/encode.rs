//! Farbfeld encoder.
//!
//! Forked from zune-farbfeld 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib).

use alloc::vec::Vec;
use enough::Stop;
use whereat::at;

use crate::error::{BitmapError, UnsupportedKind};
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
) -> crate::Result<Vec<u8>> {
    if width == 0 {
        // Rows of zero bytes can't be split into rows; the decoder refuses
        // such files too.
        return Err(at!(BitmapError::InvalidData(
            "farbfeld width is zero".into()
        )));
    }
    let w = width as usize;
    let h = height as usize;
    let bpp = layout.bytes_per_pixel();
    let expected = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(bpp))
        .ok_or_else(|| at!(BitmapError::DimensionsTooLarge { width, height }))?;
    if pixels.len() < expected {
        return Err(at!(BitmapError::BufferTooSmall {
            needed: expected,
            actual: pixels.len(),
        }));
    }

    // Output: 16 header + w*h*8 pixel bytes
    let pixel_bytes = w
        .checked_mul(h)
        .and_then(|wh| wh.checked_mul(8))
        .ok_or_else(|| at!(BitmapError::DimensionsTooLarge { width, height }))?;
    let total = pixel_bytes
        .checked_add(16)
        .ok_or_else(|| at!(BitmapError::DimensionsTooLarge { width, height }))?;

    let mut out = Vec::with_capacity(total);

    // Header
    out.extend_from_slice(b"farbfeld");
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());

    stop.check().map_err(|r| at!(BitmapError::from(r)))?;

    if h == 0
        && matches!(
            layout,
            PixelLayout::Rgba16
                | PixelLayout::Rgba8
                | PixelLayout::Rgb8
                | PixelLayout::Bgra8
                | PixelLayout::Bgrx8
                | PixelLayout::Bgr8
                | PixelLayout::Gray8
        )
    {
        // Zero-height images are header-only. Return before the arms below
        // compute `w * row_bytes`: on 32-bit targets those products overflow
        // (checked) or wrap to zero (`chunks_exact(0)` panics) for widths
        // >= 2**29 even though no rows exist. Unsupported layouts still fall
        // through to the rejection below, unchanged.
        return Ok(out);
    }

    match layout {
        PixelLayout::Rgba16 => {
            // Native endian u16 → big endian u16, with a fixed output extent.
            out.resize(total, 0);
            for (row_idx, (row, dst)) in pixels[..expected]
                .chunks_exact(w * 8)
                .zip(out[16..].chunks_exact_mut(w * 8))
                .enumerate()
            {
                if row_idx % 16 == 0 {
                    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
                }
                for (&pair, output) in row
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .zip(dst.as_chunks_mut::<2>().0.iter_mut())
                {
                    *output = u16::from_ne_bytes(pair).to_be_bytes();
                }
            }
        }
        PixelLayout::Rgba8 => {
            // Expand u8 → u16 via val * 257
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 4).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
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
                    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
                }
                for pixel in row.as_chunks::<3>().0.iter() {
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
        PixelLayout::Bgra8 => {
            // Expand BGRA u8 → RGBA u16 (swap B↔R)
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 4).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
                }
                for pixel in row.as_chunks::<4>().0.iter() {
                    let r: u16 = pixel[2] as u16 * 257;
                    let g: u16 = pixel[1] as u16 * 257;
                    let b: u16 = pixel[0] as u16 * 257;
                    let a: u16 = pixel[3] as u16 * 257;
                    out.extend_from_slice(&r.to_be_bytes());
                    out.extend_from_slice(&g.to_be_bytes());
                    out.extend_from_slice(&b.to_be_bytes());
                    out.extend_from_slice(&a.to_be_bytes());
                }
            }
        }
        PixelLayout::Bgrx8 => {
            // Expand BGRX u8 → RGBA u16 (swap B↔R, alpha=65535)
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 4).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
                }
                for pixel in row.as_chunks::<4>().0.iter() {
                    let r: u16 = pixel[2] as u16 * 257;
                    let g: u16 = pixel[1] as u16 * 257;
                    let b: u16 = pixel[0] as u16 * 257;
                    out.extend_from_slice(&r.to_be_bytes());
                    out.extend_from_slice(&g.to_be_bytes());
                    out.extend_from_slice(&b.to_be_bytes());
                    out.extend_from_slice(&65535u16.to_be_bytes());
                }
            }
        }
        PixelLayout::Bgr8 => {
            // Expand BGR u8 → RGBA u16 (swap B↔R, alpha=65535)
            for (row_idx, row) in pixels[..expected].chunks_exact(w * 3).enumerate() {
                if row_idx % 16 == 0 {
                    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
                }
                for pixel in row.as_chunks::<3>().0.iter() {
                    let r: u16 = pixel[2] as u16 * 257;
                    let g: u16 = pixel[1] as u16 * 257;
                    let b: u16 = pixel[0] as u16 * 257;
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
                    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
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
            return Err(at!(BitmapError::UnsupportedVariant(
                UnsupportedKind::Feature,
                alloc::format!(
                    "cannot encode {:?} as farbfeld (supported: Rgba16, Rgba8, Rgb8, Gray8)",
                    layout
                )
            )));
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba16_preserves_every_word_in_big_endian_order() {
        for (width, height) in [(64u32, 256u32), (31, 17), (1, 33)] {
            let words = width as usize * height as usize * 4;
            let pixels: Vec<u8> = (0..words).flat_map(|i| (i as u16).to_ne_bytes()).collect();
            let actual = encode_farbfeld(
                &pixels,
                width,
                height,
                PixelLayout::Rgba16,
                &enough::Unstoppable,
            )
            .unwrap();
            let mut expected = b"farbfeld".to_vec();
            expected.extend_from_slice(&width.to_be_bytes());
            expected.extend_from_slice(&height.to_be_bytes());
            for i in 0..words {
                expected.extend_from_slice(&(i as u16).to_be_bytes());
            }
            assert_eq!(actual, expected, "{width}x{height}");
        }
    }

    /// h=0 must return the 16-byte header on every encodable layout, for
    /// widths whose `w * row_bytes` products overflow a 32-bit usize.
    /// Covers the F6 i686 panic: `chunks_exact(w * bpp)` evaluated eagerly
    /// even though a zero-height image has no rows.
    #[test]
    fn zero_height_encodes_header_only_for_all_layouts_and_huge_widths() {
        let layouts = [
            PixelLayout::Rgba16,
            PixelLayout::Rgba8,
            PixelLayout::Rgb8,
            PixelLayout::Bgra8,
            PixelLayout::Bgrx8,
            PixelLayout::Bgr8,
            PixelLayout::Gray8,
        ];
        let widths = [1u32, 4096, 1 << 29, 1 << 30, 1 << 31, 3 << 30, u32::MAX];
        for layout in layouts {
            for width in widths {
                // Trailing input bytes must remain ignored at h=0.
                let out = encode_farbfeld(&[0xa5; 64], width, 0, layout, &enough::Unstoppable)
                    .unwrap_or_else(|e| panic!("{layout:?} {width}x0: {e:?}"));
                let mut want = b"farbfeld".to_vec();
                want.extend_from_slice(&width.to_be_bytes());
                want.extend_from_slice(&0u32.to_be_bytes());
                assert_eq!(out, want, "{layout:?} at {width}x0");
            }
        }
    }

    #[test]
    fn zero_height_rejects_unsupported_layouts() {
        for layout in [
            PixelLayout::Gray16,
            PixelLayout::GrayF32,
            PixelLayout::RgbF32,
        ] {
            for width in [1u32, 4096, 1 << 29, 1 << 30, 1 << 31, 3 << 30, u32::MAX] {
                let error = encode_farbfeld(&[], width, 0, layout, &enough::Unstoppable)
                    .expect_err("unsupported layout must not produce a header-only image");
                assert!(
                    matches!(
                        error.error(),
                        BitmapError::UnsupportedVariant(UnsupportedKind::Feature, _)
                    ),
                    "{layout:?} {width}x0: {error:?}"
                );
            }
        }
    }

    /// h=0 keeps the unchanged initial `stop.check()`: it still observes a
    /// cancel, and no per-row checks run because there are no rows.
    #[test]
    fn zero_height_runs_the_initial_cancellation_check_once() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        struct CancelAfter {
            calls: AtomicUsize,
            fail_at: usize,
        }
        impl Stop for CancelAfter {
            fn check(&self) -> Result<(), enough::StopReason> {
                if self.calls.fetch_add(1, Ordering::SeqCst) + 1 >= self.fail_at {
                    Err(enough::StopReason::Cancelled)
                } else {
                    Ok(())
                }
            }
        }

        for layout in [PixelLayout::Rgba8, PixelLayout::Gray8] {
            let cancelled = encode_farbfeld(
                &[],
                8,
                0,
                layout,
                &CancelAfter {
                    calls: AtomicUsize::new(0),
                    fail_at: 1,
                },
            );
            assert!(
                matches!(
                    cancelled.unwrap_err().error(),
                    BitmapError::Cancelled(enough::StopReason::Cancelled)
                ),
                "{layout:?} h=0 must observe cancel"
            );

            let stop = CancelAfter {
                calls: AtomicUsize::new(0),
                fail_at: 2,
            };
            let out = encode_farbfeld(&[], 8, 0, layout, &stop).expect("header-only");
            assert_eq!(out.len(), 16);
            assert_eq!(
                stop.calls.load(Ordering::SeqCst),
                1,
                "{layout:?} h=0 ran the wrong number of checks"
            );
        }
    }

    /// The width==0 rejection precedes any zero-height handling, unchanged.
    #[test]
    fn zero_width_is_rejected_even_at_zero_height() {
        for layout in [PixelLayout::Rgba8, PixelLayout::Gray8] {
            let e = encode_farbfeld(&[], 0, 0, layout, &enough::Unstoppable)
                .expect_err("zero width must still be rejected");
            let dbg = alloc::format!("{e:?}");
            assert!(dbg.contains("width is zero"), "{layout:?}: {dbg}");
        }
    }
}
