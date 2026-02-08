//! Test corpus: roundtrip tests with various patterns, sizes, and formats.

use enough::Unstoppable;
use zenpnm::*;

fn checkerboard(w: usize, h: usize, bpp: usize) -> Vec<u8> {
    let mut pixels = vec![0u8; w * h * bpp];
    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * bpp;
            if (x + y) % 2 == 0 {
                for c in 0..bpp {
                    pixels[off + c] = 200 + (c as u8 * 20);
                }
            } else {
                for c in 0..bpp {
                    pixels[off + c] = 10 + (c as u8 * 30);
                }
            }
        }
    }
    pixels
}

fn noise_pattern(w: usize, h: usize, bpp: usize) -> Vec<u8> {
    let mut pixels = vec![0u8; w * h * bpp];
    let mut state: u32 = 0xDEAD_BEEF;
    for p in pixels.iter_mut() {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *p = state as u8;
    }
    pixels
}

// ── PNM roundtrips ───────────────────────────────────────────────────

#[test]
fn flat_ppm_roundtrip() {
    let pixels = checkerboard(8, 6, 3);
    let encoded = encode_ppm(&pixels, 8, 6, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[test]
fn flat_pgm_roundtrip() {
    let pixels = noise_pattern(16, 12, 1);
    let encoded = encode_pgm(&pixels, 16, 12, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[test]
fn flat_pam_roundtrip_rgba() {
    let pixels = noise_pattern(5, 7, 4);
    let encoded = encode_pam(&pixels, 5, 7, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[test]
fn flat_pam_roundtrip_gray() {
    let pixels = vec![0, 64, 128, 192, 255, 42];
    let encoded = encode_pam(&pixels, 3, 2, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[test]
fn flat_pam_roundtrip_rgb() {
    let pixels = checkerboard(4, 4, 3);
    let encoded = encode_pam(&pixels, 4, 4, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[test]
fn flat_pfm_roundtrip_grayf32() {
    let floats: Vec<f32> = (0..12).map(|i| i as f32 / 11.0).collect();
    let pixels: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    let encoded = encode_pfm(&pixels, 4, 3, PixelLayout::GrayF32, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::GrayF32);
    let out_floats: Vec<f32> = decoded
        .pixels()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    for (i, (a, b)) in floats.iter().zip(out_floats.iter()).enumerate() {
        assert!((a - b).abs() < 1e-6, "PFM mismatch at {i}: {a} vs {b}");
    }
}

#[test]
fn flat_pfm_roundtrip_rgbf32() {
    let floats: Vec<f32> = (0..24).map(|i| i as f32 / 23.0).collect();
    let pixels: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    let encoded = encode_pfm(&pixels, 4, 2, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    let out_floats: Vec<f32> = decoded
        .pixels()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    for (i, (a, b)) in floats.iter().zip(out_floats.iter()).enumerate() {
        assert!((a - b).abs() < 1e-6, "PFM mismatch at {i}: {a} vs {b}");
    }
}

// ── BMP roundtrips ───────────────────────────────────────────────────

#[cfg(feature = "basic-bmp")]
#[test]
fn flat_bmp_roundtrip() {
    let pixels = checkerboard(10, 8, 3);
    let encoded = encode_bmp(&pixels, 10, 8, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(!decoded.is_borrowed());
}

#[cfg(feature = "basic-bmp")]
#[test]
fn flat_bmp_rgba_roundtrip() {
    let pixels = noise_pattern(7, 5, 4);
    let encoded = encode_bmp_rgba(&pixels, 7, 5, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

// ── Edge cases ───────────────────────────────────────────────────────

#[test]
fn single_pixel_ppm() {
    let pixels = vec![42, 100, 200];
    let encoded = encode_ppm(&pixels, 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[42, 100, 200]);
    assert!(decoded.is_borrowed());
}

#[test]
fn single_pixel_pgm() {
    let pixels = vec![128];
    let encoded = encode_pgm(&pixels, 1, 1, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[128]);
    assert!(decoded.is_borrowed());
}

#[cfg(feature = "basic-bmp")]
#[test]
fn single_pixel_bmp() {
    let pixels = vec![255, 0, 128];
    let encoded = encode_bmp(&pixels, 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[255, 0, 128]);
}

#[test]
fn wide_image_ppm() {
    let pixels = noise_pattern(1000, 1, 3);
    let encoded = encode_ppm(&pixels, 1000, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[test]
fn tall_image_pgm() {
    let pixels = noise_pattern(1, 1000, 1);
    let encoded = encode_pgm(&pixels, 1, 1000, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[cfg(feature = "basic-bmp")]
#[test]
fn bmp_odd_width_padding() {
    let pixels = noise_pattern(3, 3, 3);
    let encoded = encode_bmp(&pixels, 3, 3, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "basic-bmp")]
#[test]
fn bmp_width_1_padding() {
    let pixels = vec![10, 20, 30, 40, 50, 60];
    let encoded = encode_bmp(&pixels, 1, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

// ── Limits ───────────────────────────────────────────────────────────

#[test]
fn limits_max_width() {
    let encoded = encode_ppm(&[0u8; 12], 2, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_width: Some(1),
        ..Default::default()
    };
    assert!(decode_with_limits(&encoded, &limits, Unstoppable).is_err());
}

#[test]
fn limits_max_height() {
    let encoded = encode_ppm(&[0u8; 12], 2, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_height: Some(1),
        ..Default::default()
    };
    assert!(decode_with_limits(&encoded, &limits, Unstoppable).is_err());
}

#[cfg(feature = "basic-bmp")]
#[test]
fn limits_max_memory_bmp() {
    let encoded = encode_bmp(&[0u8; 12], 2, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_memory_bytes: Some(1),
        ..Default::default()
    };
    assert!(decode_bmp_with_limits(&encoded, &limits, Unstoppable).is_err());
}

// ── External files ───────────────────────────────────────────────────

#[test]
fn decode_external_ppm_if_available() {
    let path = "/home/lilith/work/libwebp/examples/test_ref.ppm";
    if let Ok(data) = std::fs::read(path) {
        let decoded = decode(&data, Unstoppable).unwrap();
        assert!(decoded.width > 0);
        let reencoded = encode_ppm(
            decoded.pixels(),
            decoded.width,
            decoded.height,
            decoded.layout,
            Unstoppable,
        )
        .unwrap();
        let decoded2 = decode(&reencoded, Unstoppable).unwrap();
        assert_eq!(decoded.pixels(), decoded2.pixels());
    }
}

#[cfg(feature = "basic-bmp")]
#[test]
fn decode_external_bmp_if_available() {
    let path = "/home/lilith/work/salzweg/test-assets/sunflower.bmp";
    if let Ok(data) = std::fs::read(path) {
        let decoded = decode_bmp(&data, Unstoppable).unwrap();
        assert!(decoded.width > 0);
        let reencoded = encode_bmp(
            decoded.pixels(),
            decoded.width,
            decoded.height,
            decoded.layout,
            Unstoppable,
        )
        .unwrap();
        let decoded2 = decode_bmp(&reencoded, Unstoppable).unwrap();
        assert_eq!(decoded.pixels(), decoded2.pixels());
    }
}
