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

#[cfg(feature = "bmp")]
#[test]
fn flat_bmp_roundtrip() {
    let pixels = checkerboard(10, 8, 3);
    let encoded = encode_bmp(&pixels, 10, 8, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(!decoded.is_borrowed());
}

#[cfg(feature = "bmp")]
#[test]
fn flat_bmp_rgba_roundtrip() {
    let pixels = noise_pattern(7, 5, 4);
    let encoded = encode_bmp_rgba(&pixels, 7, 5, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

// ── BGR/BGRA/BGRX support ───────────────────────────────────────────

/// Build BGRA pixels: B at [0], G at [1], R at [2], A at [3].
fn bgra_pattern(w: usize, h: usize) -> Vec<u8> {
    let mut pixels = vec![0u8; w * h * 4];
    let mut state: u32 = 0xCAFE_BABE;
    for chunk in pixels.chunks_exact_mut(4) {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        chunk[0] = state as u8; // B
        chunk[1] = (state >> 8) as u8; // G
        chunk[2] = (state >> 16) as u8; // R
        chunk[3] = (state >> 24) as u8; // A
    }
    pixels
}

/// Build BGR pixels: B at [0], G at [1], R at [2].
fn bgr_pattern(w: usize, h: usize) -> Vec<u8> {
    let mut pixels = vec![0u8; w * h * 3];
    let mut state: u32 = 0xBADF00D;
    for chunk in pixels.chunks_exact_mut(3) {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        chunk[0] = state as u8; // B
        chunk[1] = (state >> 8) as u8; // G
        chunk[2] = (state >> 16) as u8; // R
    }
    pixels
}

#[test]
fn pgm_from_bgr_correct_luminance() {
    // Pure red in BGR: B=0, G=0, R=255
    let bgr_red = vec![0u8, 0, 255];
    let pgm = encode_pgm(&bgr_red, 1, 1, PixelLayout::Bgr8, Unstoppable).unwrap();
    let decoded = decode(&pgm, Unstoppable).unwrap();
    // Luminance of pure red: 255*299/1000 ≈ 76
    assert_eq!(decoded.pixels(), &[76]);

    // Pure blue in BGR: B=255, G=0, R=0
    let bgr_blue = vec![255u8, 0, 0];
    let pgm = encode_pgm(&bgr_blue, 1, 1, PixelLayout::Bgr8, Unstoppable).unwrap();
    let decoded = decode(&pgm, Unstoppable).unwrap();
    // Luminance of pure blue: 255*114/1000 ≈ 29
    assert_eq!(decoded.pixels(), &[29]);
}

#[test]
fn pgm_from_bgra_correct_luminance() {
    // Pure red in BGRA: B=0, G=0, R=255, A=255
    let bgra_red = vec![0u8, 0, 255, 255];
    let pgm = encode_pgm(&bgra_red, 1, 1, PixelLayout::Bgra8, Unstoppable).unwrap();
    let decoded = decode(&pgm, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[76]);
}

#[test]
fn pgm_from_bgrx_correct_luminance() {
    // Pure red in BGRX: B=0, G=0, R=255, X=0
    let bgrx_red = vec![0u8, 0, 255, 0];
    let pgm = encode_pgm(&bgrx_red, 1, 1, PixelLayout::Bgrx8, Unstoppable).unwrap();
    let decoded = decode(&pgm, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[76]);
}

#[test]
fn ppm_from_bgr_roundtrip_via_rgb() {
    let bgr = bgr_pattern(4, 3);
    let ppm = encode_ppm(&bgr, 4, 3, PixelLayout::Bgr8, Unstoppable).unwrap();
    let decoded = decode(&ppm, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    // Verify channel swap: BGR[B,G,R] → RGB[R,G,B]
    for i in 0..(4 * 3) {
        let b = bgr[i * 3];
        let g = bgr[i * 3 + 1];
        let r = bgr[i * 3 + 2];
        let off = i * 3;
        assert_eq!(decoded.pixels()[off], r);
        assert_eq!(decoded.pixels()[off + 1], g);
        assert_eq!(decoded.pixels()[off + 2], b);
    }
}

#[test]
fn ppm_from_bgra_drops_alpha() {
    let bgra = bgra_pattern(3, 2);
    let ppm = encode_ppm(&bgra, 3, 2, PixelLayout::Bgra8, Unstoppable).unwrap();
    let decoded = decode(&ppm, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    for i in 0..(3 * 2) {
        let b = bgra[i * 4];
        let g = bgra[i * 4 + 1];
        let r = bgra[i * 4 + 2];
        let off = i * 3;
        assert_eq!(decoded.pixels()[off], r);
        assert_eq!(decoded.pixels()[off + 1], g);
        assert_eq!(decoded.pixels()[off + 2], b);
    }
}

#[cfg(feature = "bmp")]
#[test]
fn bmp_encode_from_bgra_roundtrip() {
    let bgra = bgra_pattern(5, 4);
    // Encode BGRA → 32-bit BMP, decode back as RGBA
    let encoded = encode_bmp_rgba(&bgra, 5, 4, PixelLayout::Bgra8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    // Verify BGRA→RGBA conversion happened
    for i in 0..(5 * 4) {
        let b = bgra[i * 4];
        let g = bgra[i * 4 + 1];
        let r = bgra[i * 4 + 2];
        let a = bgra[i * 4 + 3];
        let off = i * 4;
        assert_eq!(decoded.pixels()[off], r, "R mismatch at pixel {i}");
        assert_eq!(decoded.pixels()[off + 1], g, "G mismatch at pixel {i}");
        assert_eq!(decoded.pixels()[off + 2], b, "B mismatch at pixel {i}");
        assert_eq!(decoded.pixels()[off + 3], a, "A mismatch at pixel {i}");
    }
}

#[cfg(feature = "bmp")]
#[test]
fn bmp_native_decode_bgra_roundtrip() {
    let bgra = bgra_pattern(5, 4);
    // Encode BGRA → 32-bit BMP (native fast path), decode as native BGRA
    let encoded = encode_bmp_rgba(&bgra, 5, 4, PixelLayout::Bgra8, Unstoppable).unwrap();
    let decoded = decode_bmp_native(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Bgra8);
    // Should be identical to input — no channel swizzle
    assert_eq!(decoded.pixels(), &bgra[..]);
}

#[cfg(feature = "bmp")]
#[test]
fn bmp_native_decode_bgr_roundtrip() {
    let bgr = bgr_pattern(6, 3);
    // Encode BGR → 24-bit BMP (native fast path), decode as native BGR
    let encoded = encode_bmp(&bgr, 6, 3, PixelLayout::Bgr8, Unstoppable).unwrap();
    let decoded = decode_bmp_native(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Bgr8);
    assert_eq!(decoded.pixels(), &bgr[..]);
}

#[cfg(feature = "bmp")]
#[test]
fn bmp_encode_from_bgrx_roundtrip() {
    // BGRX: 4th byte is padding (should become 255 in output)
    let bgrx: Vec<u8> = (0..20)
        .flat_map(|i| [i * 10, i * 5, 200 - i * 8, 0u8]) // B, G, R, X=0
        .collect();
    let encoded = encode_bmp_rgba(&bgrx, 5, 4, PixelLayout::Bgrx8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    for i in 0..20 {
        let b = bgrx[i * 4];
        let g = bgrx[i * 4 + 1];
        let r = bgrx[i * 4 + 2];
        let off = i * 4;
        assert_eq!(decoded.pixels()[off], r);
        assert_eq!(decoded.pixels()[off + 1], g);
        assert_eq!(decoded.pixels()[off + 2], b);
        assert_eq!(decoded.pixels()[off + 3], 255, "BGRX alpha should be 255");
    }
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

#[cfg(feature = "bmp")]
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

#[cfg(feature = "bmp")]
#[test]
fn bmp_odd_width_padding() {
    let pixels = noise_pattern(3, 3, 3);
    let encoded = encode_bmp(&pixels, 3, 3, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "bmp")]
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

#[cfg(feature = "bmp")]
#[test]
fn limits_max_memory_bmp() {
    let encoded = encode_bmp(&[0u8; 12], 2, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_memory_bytes: Some(1),
        ..Default::default()
    };
    assert!(decode_bmp_with_limits(&encoded, &limits, Unstoppable).is_err());
}

// ── Farbfeld roundtrips ──────────────────────────────────────────────

#[test]
fn farbfeld_rgba16_roundtrip() {
    // Create RGBA16 pixel data (native endian u16 as bytes)
    let w = 3u32;
    let h = 2u32;
    let mut pixels = Vec::with_capacity(w as usize * h as usize * 8);
    for i in 0..(w * h) {
        let r = (i * 1000) as u16;
        let g = (i * 2000) as u16;
        let b = (i * 3000) as u16;
        let a = 65535u16;
        pixels.extend_from_slice(&r.to_ne_bytes());
        pixels.extend_from_slice(&g.to_ne_bytes());
        pixels.extend_from_slice(&b.to_ne_bytes());
        pixels.extend_from_slice(&a.to_ne_bytes());
    }
    let encoded = encode_farbfeld(&pixels, w, h, PixelLayout::Rgba16, Unstoppable).unwrap();
    assert_eq!(&encoded[0..8], b"farbfeld");

    let decoded = decode_farbfeld(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba16);
    assert_eq!(decoded.width, w);
    assert_eq!(decoded.height, h);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[test]
fn farbfeld_auto_detect() {
    // Farbfeld should be auto-detected by decode()
    let pixels: Vec<u8> = (0..8)
        .flat_map(|_| {
            let r = 1000u16;
            let g = 2000u16;
            let b = 3000u16;
            let a = 65535u16;
            let mut v = Vec::new();
            v.extend_from_slice(&r.to_ne_bytes());
            v.extend_from_slice(&g.to_ne_bytes());
            v.extend_from_slice(&b.to_ne_bytes());
            v.extend_from_slice(&a.to_ne_bytes());
            v
        })
        .collect();
    let encoded = encode_farbfeld(&pixels, 4, 2, PixelLayout::Rgba16, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba16);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[test]
fn farbfeld_from_rgb8() {
    // Encode RGB8 as farbfeld (expand to RGBA16), verify header
    let pixels = vec![255, 0, 0, 0, 255, 0]; // 2 RGB pixels
    let encoded = encode_farbfeld(&pixels, 2, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert_eq!(&encoded[0..8], b"farbfeld");
    // Should be 16 header + 2 * 8 = 32 bytes total
    assert_eq!(encoded.len(), 32);

    let decoded = decode_farbfeld(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba16);
    // Check first pixel: R=65535, G=0, B=0, A=65535
    let r = u16::from_ne_bytes([decoded.pixels()[0], decoded.pixels()[1]]);
    let g = u16::from_ne_bytes([decoded.pixels()[2], decoded.pixels()[3]]);
    let b = u16::from_ne_bytes([decoded.pixels()[4], decoded.pixels()[5]]);
    let a = u16::from_ne_bytes([decoded.pixels()[6], decoded.pixels()[7]]);
    assert_eq!(r, 65535); // 255 * 257
    assert_eq!(g, 0);
    assert_eq!(b, 0);
    assert_eq!(a, 65535);
}

#[test]
fn farbfeld_from_gray8() {
    let pixels = vec![128]; // single gray pixel
    let encoded = encode_farbfeld(&pixels, 1, 1, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode_farbfeld(&encoded, Unstoppable).unwrap();
    // Gray8 128 → RGBA16: all channels = 128*257 = 32896, alpha=65535
    let r = u16::from_ne_bytes([decoded.pixels()[0], decoded.pixels()[1]]);
    let a = u16::from_ne_bytes([decoded.pixels()[6], decoded.pixels()[7]]);
    assert_eq!(r, 128 * 257);
    assert_eq!(a, 65535);
}

#[test]
fn farbfeld_limits_reject() {
    let pixels: Vec<u8> = vec![0; 4 * 8]; // 4 pixels × 8 bytes
    let encoded = encode_farbfeld(&pixels, 2, 2, PixelLayout::Rgba16, Unstoppable).unwrap();
    let limits = Limits {
        max_memory_bytes: Some(1),
        ..Default::default()
    };
    assert!(decode_farbfeld_with_limits(&encoded, &limits, Unstoppable).is_err());
}

// ── BMP auto-detection ──────────────────────────────────────────────

#[cfg(feature = "bmp")]
#[test]
fn bmp_auto_detect() {
    let pixels = checkerboard(4, 4, 3);
    let encoded = encode_bmp(&pixels, 4, 4, PixelLayout::Rgb8, Unstoppable).unwrap();
    // decode() should auto-detect BMP
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "bmp")]
#[test]
fn bmp_auto_detect_rgba() {
    let pixels = noise_pattern(3, 3, 4);
    let encoded = encode_bmp_rgba(&pixels, 3, 3, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
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

#[cfg(feature = "bmp")]
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
