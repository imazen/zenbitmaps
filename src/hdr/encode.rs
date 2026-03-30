//! Radiance HDR (.hdr / RGBE) encoder.

use alloc::vec::Vec;
use enough::Stop;

use crate::error::BitmapError;
use crate::pixel::PixelLayout;

/// Encode pixel data as Radiance HDR (RGBE with new-style RLE).
///
/// Accepts `RgbF32` (12 bytes/pixel, 3×f32) or `Rgb8` (converted via /255.0).
pub(crate) fn encode_hdr(
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

    // Estimate output size: header (~60 bytes) + compressed data (roughly w*h*4)
    let est_size = 64 + w * h * 4;
    let mut out = Vec::with_capacity(est_size);

    // Write header
    out.extend_from_slice(b"#?RADIANCE\n");
    out.extend_from_slice(b"FORMAT=32-bit_rle_rgbe\n");
    out.extend_from_slice(b"\n");

    // Write resolution line
    let res_line = alloc::format!("-Y {height} +X {width}\n");
    out.extend_from_slice(res_line.as_bytes());

    stop.check()?;

    // Convert each scanline to RGBE, then RLE-encode
    let mut rgbe_scanline = alloc::vec![[0u8; 4]; w];

    for row in 0..h {
        if row % 16 == 0 {
            stop.check()?;
        }

        // Convert this row's pixels to RGBE
        match layout {
            PixelLayout::RgbF32 => {
                let row_start = row * w * 12;
                for (col, rgbe) in rgbe_scanline.iter_mut().enumerate().take(w) {
                    let base = row_start + col * 12;
                    let r = f32::from_le_bytes([
                        pixels[base],
                        pixels[base + 1],
                        pixels[base + 2],
                        pixels[base + 3],
                    ]);
                    let g = f32::from_le_bytes([
                        pixels[base + 4],
                        pixels[base + 5],
                        pixels[base + 6],
                        pixels[base + 7],
                    ]);
                    let b = f32::from_le_bytes([
                        pixels[base + 8],
                        pixels[base + 9],
                        pixels[base + 10],
                        pixels[base + 11],
                    ]);
                    *rgbe = f32_to_rgbe(r, g, b);
                }
            }
            PixelLayout::Rgb8 => {
                let row_start = row * w * 3;
                for (col, rgbe) in rgbe_scanline.iter_mut().enumerate().take(w) {
                    let base = row_start + col * 3;
                    let r = pixels[base] as f32 / 255.0;
                    let g = pixels[base + 1] as f32 / 255.0;
                    let b = pixels[base + 2] as f32 / 255.0;
                    *rgbe = f32_to_rgbe(r, g, b);
                }
            }
            _ => {
                return Err(BitmapError::UnsupportedVariant(alloc::format!(
                    "cannot encode {layout:?} as HDR (supported: RgbF32, Rgb8)"
                )));
            }
        }

        // Write new-style RLE if width is in the valid range
        if (8..=0x7FFF).contains(&w) {
            // Marker: [2, 2, width_hi, width_lo]
            out.push(2);
            out.push(2);
            out.push((w >> 8) as u8);
            out.push((w & 0xFF) as u8);

            // Encode each channel separately
            for ch in 0..4 {
                rle_encode_channel(&rgbe_scanline, ch, &mut out);
            }
        } else {
            // Flat RGBE (width < 8 or > 32767)
            for px in &rgbe_scanline {
                out.extend_from_slice(px);
            }
        }
    }

    Ok(out)
}

/// Convert 3×f32 linear RGB to RGBE (4 bytes).
fn f32_to_rgbe(r: f32, g: f32, b: f32) -> [u8; 4] {
    let max = r.max(g).max(b);
    if max < 1e-32 {
        return [0, 0, 0, 0];
    }
    // Extract exponent from IEEE 754 bits.
    // For a normal f32: bits = sign(1) | exponent(8) | mantissa(23)
    // Biased exponent = (bits >> 23) & 0xFF
    // True exponent = biased - 127, and the value is in [1.0, 2.0) * 2^true_exp
    // frexp returns mantissa in [0.5, 1.0), so its exponent = true_exp + 1
    let bits = max.to_bits();
    let biased_exp = ((bits >> 23) & 0xFF) as i32;

    // frexp exponent: for normal floats, this is (biased_exp - 127 + 1) = biased_exp - 126
    let raw_exp = biased_exp - 126;

    // scale = 256 / 2^raw_exp = 2^(8 - raw_exp)
    // In IEEE 754: exponent field = (8 - raw_exp + 127) = 135 - raw_exp
    let scale_exp = 135i32 - raw_exp;

    // Clamp to valid IEEE 754 exponent range [1, 254] (avoid denormals and inf)
    if !(1..=254).contains(&scale_exp) {
        // Value is either too large or too small to represent in RGBE
        if scale_exp < 1 {
            // Very large value — clamp to max representable
            return [255, 255, 255, 255];
        }
        // Very small value
        return [0, 0, 0, 0];
    }

    let scale = f32::from_bits((scale_exp as u32) << 23);
    let re = (r * scale) as u8;
    let ge = (g * scale) as u8;
    let be = (b * scale) as u8;
    let e = (raw_exp + 128) as u8;
    [re, ge, be, e]
}

/// RLE-encode one channel of a scanline and append to `out`.
///
/// Uses the new-style Radiance RLE:
/// - Runs of identical values: [128 + count, value] (count in 1..=128)
/// - Literal sequences: [count, val1, val2, ...] (count in 1..=128)
fn rle_encode_channel(scanline: &[[u8; 4]], ch: usize, out: &mut Vec<u8>) {
    let w = scanline.len();
    let mut col = 0;

    while col < w {
        // Look for a run of identical values
        let val = scanline[col][ch];
        let mut run_len = 1;
        while col + run_len < w && scanline[col + run_len][ch] == val && run_len < 127 {
            run_len += 1;
        }

        if run_len >= 3 {
            // Emit as a run
            out.push(128 + run_len as u8);
            out.push(val);
            col += run_len;
        } else {
            // Emit as literals. Scan forward to find where the next run starts.
            let lit_start = col;
            col += run_len;

            while col < w {
                // Check if a run of 3+ starts here
                let v = scanline[col][ch];
                let mut ahead = 1;
                while col + ahead < w && scanline[col + ahead][ch] == v && ahead < 3 {
                    ahead += 1;
                }
                if ahead >= 3 {
                    break; // Stop literal, run follows
                }
                col += 1;
                if col - lit_start >= 127 {
                    break; // Max literal length
                }
            }

            let lit_len = col - lit_start;
            out.push(lit_len as u8);
            for entry in &scanline[lit_start..col] {
                out.push(entry[ch]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn f32_to_rgbe_zero() {
        assert_eq!(f32_to_rgbe(0.0, 0.0, 0.0), [0, 0, 0, 0]);
    }

    #[test]
    fn f32_to_rgbe_tiny() {
        assert_eq!(f32_to_rgbe(1e-40, 1e-40, 1e-40), [0, 0, 0, 0]);
    }

    #[test]
    fn f32_to_rgbe_roundtrip_unit() {
        // Encode 1.0, 0.5, 0.25 and verify decode is close
        let rgbe = f32_to_rgbe(1.0, 0.5, 0.25);
        let (r, g, b) = super::super::decode::rgbe_to_f32(rgbe[0], rgbe[1], rgbe[2], rgbe[3]);
        assert!((r - 1.0).abs() < 0.02, "r = {r}");
        assert!((g - 0.5).abs() < 0.02, "g = {g}");
        assert!((b - 0.25).abs() < 0.02, "b = {b}");
    }

    #[test]
    fn rle_encode_run() {
        // 5 identical values should produce [128+5, val]
        let scanline: Vec<[u8; 4]> = (0..5).map(|_| [42, 0, 0, 0]).collect();
        let mut out = Vec::new();
        rle_encode_channel(&scanline, 0, &mut out);
        assert_eq!(out, &[128 + 5, 42]);
    }

    #[test]
    fn rle_encode_literals() {
        // 3 distinct values: [3, a, b, c]
        let scanline: Vec<[u8; 4]> = vec![[10, 0, 0, 0], [20, 0, 0, 0], [30, 0, 0, 0]];
        let mut out = Vec::new();
        rle_encode_channel(&scanline, 0, &mut out);
        // With only 3 distinct values, they'll be emitted as literals
        assert_eq!(out[0], 3); // literal count
        assert_eq!(&out[1..], &[10, 20, 30]);
    }
}
