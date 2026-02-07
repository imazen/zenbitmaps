//! Test corpus: roundtrip tests with various patterns, sizes, and formats.
//! Verifies zero-copy borrowing and correctness of flat one-shot API.

use enough::Unstoppable;
use zenpnm::*;

/// Generate a checkerboard pattern.
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

/// Generate noise-like pattern (deterministic).
fn noise_pattern(w: usize, h: usize, bpp: usize) -> Vec<u8> {
    let mut pixels = vec![0u8; w * h * bpp];
    let mut state: u32 = 0xDEAD_BEEF;
    for p in pixels.iter_mut() {
        // xorshift32
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *p = state as u8;
    }
    pixels
}

// ── PNM flat API roundtrips ──────────────────────────────────────────

#[test]
fn flat_ppm_roundtrip() {
    let pixels = checkerboard(8, 6, 3);
    let encoded = encode_ppm(&pixels, 8, 6, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 8);
    assert_eq!(decoded.height, 6);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed(), "PPM should be zero-copy");
}

#[test]
fn flat_pgm_roundtrip() {
    let pixels = noise_pattern(16, 12, 1);
    let encoded = encode_pgm(&pixels, 16, 12, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed(), "PGM should be zero-copy");
}

#[test]
fn flat_pam_roundtrip_rgba() {
    let pixels = noise_pattern(5, 7, 4);
    let encoded = encode_pam(&pixels, 5, 7, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed(), "PAM should be zero-copy");
}

#[test]
fn flat_pam_roundtrip_gray() {
    let pixels = vec![0, 64, 128, 192, 255, 42];
    let encoded = encode_pam(&pixels, 3, 2, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[test]
fn flat_pam_roundtrip_rgb() {
    let pixels = checkerboard(4, 4, 3);
    let encoded = encode_pam(&pixels, 4, 4, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
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
    // PFM roundtrip: data goes through bottom-to-top reorder
    assert_eq!(decoded.pixels().len(), pixels.len());
    // Verify actual float values survive roundtrip
    let out_floats: Vec<f32> = decoded
        .pixels()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    for (i, (a, b)) in floats.iter().zip(out_floats.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-6,
            "PFM float mismatch at {i}: {a} vs {b}"
        );
    }
}

#[test]
fn flat_pfm_roundtrip_rgbf32() {
    let floats: Vec<f32> = (0..24).map(|i| i as f32 / 23.0).collect();
    let pixels: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    let encoded = encode_pfm(&pixels, 4, 2, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::RgbF32);
    let out_floats: Vec<f32> = decoded
        .pixels()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    for (i, (a, b)) in floats.iter().zip(out_floats.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-6,
            "PFM RGB float mismatch at {i}: {a} vs {b}"
        );
    }
}

// ── BMP flat API roundtrips ──────────────────────────────────────────

#[test]
fn flat_bmp_roundtrip() {
    let pixels = checkerboard(10, 8, 3);
    let encoded = encode_bmp(&pixels, 10, 8, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert_eq!(&encoded[0..2], b"BM");
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 10);
    assert_eq!(decoded.height, 8);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    // BMP is never zero-copy (BGR→RGB, row flip)
    assert!(!decoded.is_borrowed());
}

#[test]
fn flat_bmp_rgba_roundtrip() {
    let pixels = noise_pattern(7, 5, 4);
    let encoded = encode_bmp_rgba(&pixels, 7, 5, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
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

#[test]
fn single_pixel_bmp() {
    let pixels = vec![255, 0, 128];
    let encoded = encode_bmp(&pixels, 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[255, 0, 128]);
}

#[test]
fn wide_image_ppm() {
    // Test padding/alignment edge: wide but short
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

#[test]
fn bmp_odd_width_padding() {
    // BMP rows must be 4-byte aligned. Width=3 with RGB = 9 bytes/row, padded to 12.
    let pixels = noise_pattern(3, 3, 3);
    let encoded = encode_bmp(&pixels, 3, 3, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[test]
fn bmp_width_1_padding() {
    // Width=1 RGB = 3 bytes/row, padded to 4
    let pixels = vec![10, 20, 30, 40, 50, 60];
    let encoded = encode_bmp(&pixels, 1, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

// ── ImageInfo probing ────────────────────────────────────────────────

#[test]
fn probe_all_formats() {
    // PPM
    let ppm = encode_ppm(&[0u8; 12], 2, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let info = ImageInfo::from_bytes(&ppm).unwrap();
    assert_eq!(info.format, BitmapFormat::Ppm);
    assert_eq!(info.width, 2);
    assert_eq!(info.native_layout, PixelLayout::Rgb8);

    // PGM
    let pgm = encode_pgm(&[0u8; 4], 2, 2, PixelLayout::Gray8, Unstoppable).unwrap();
    let info = ImageInfo::from_bytes(&pgm).unwrap();
    assert_eq!(info.format, BitmapFormat::Pgm);
    assert_eq!(info.native_layout, PixelLayout::Gray8);

    // PAM
    let pam = encode_pam(&[0u8; 16], 2, 2, PixelLayout::Rgba8, Unstoppable).unwrap();
    let info = ImageInfo::from_bytes(&pam).unwrap();
    assert_eq!(info.format, BitmapFormat::Pam);
    assert_eq!(info.native_layout, PixelLayout::Rgba8);

    // BMP
    let bmp = encode_bmp(&[0u8; 12], 2, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let info = ImageInfo::from_bytes(&bmp).unwrap();
    assert_eq!(info.format, BitmapFormat::Bmp);
}

// ── Limits enforcement ───────────────────────────────────────────────

#[test]
fn limits_max_width() {
    let encoded = encode_ppm(&[0u8; 12], 2, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_width: Some(1),
        ..Default::default()
    };
    let result = decode_with_limits(&encoded, &limits, Unstoppable);
    assert!(result.is_err());
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

#[test]
fn limits_max_memory() {
    // BMP always allocates, so memory limit applies
    let encoded = encode_bmp(&[0u8; 12], 2, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_memory_bytes: Some(1),
        ..Default::default()
    };
    assert!(decode_with_limits(&encoded, &limits, Unstoppable).is_err());
}

// ── Decode from real PNM files ───────────────────────────────────────

#[test]
fn decode_external_ppm_if_available() {
    // Try to decode a real PPM from the filesystem (non-fatal if missing)
    let path = "/home/lilith/work/libwebp/examples/test_ref.ppm";
    if let Ok(data) = std::fs::read(path) {
        let decoded = decode(&data, Unstoppable).unwrap();
        assert!(decoded.width > 0);
        assert!(decoded.height > 0);
        assert_eq!(decoded.format, BitmapFormat::Ppm);
        // Re-encode and decode again
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

#[test]
fn decode_external_bmp_if_available() {
    let path = "/home/lilith/work/salzweg/test-assets/sunflower.bmp";
    if let Ok(data) = std::fs::read(path) {
        let decoded = decode(&data, Unstoppable).unwrap();
        assert!(decoded.width > 0);
        assert!(decoded.height > 0);
        assert_eq!(decoded.format, BitmapFormat::Bmp);
        // Re-encode and roundtrip
        let reencoded = encode_bmp(
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
