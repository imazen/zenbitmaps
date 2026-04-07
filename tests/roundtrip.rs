use enough::Unstoppable;
use zenbitmaps::*;

#[test]
fn ppm_roundtrip_rgb8() {
    let w = 4;
    let h = 3;
    let mut pixels = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * 3;
            if (x + y) % 2 == 0 {
                pixels[off] = 255;
                pixels[off + 1] = 0;
                pixels[off + 2] = 128;
            } else {
                pixels[off] = 0;
                pixels[off + 1] = 200;
                pixels[off + 2] = 50;
            }
        }
    }

    let encoded = encode_ppm(&pixels, w as u32, h as u32, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, w as u32);
    assert_eq!(decoded.height, h as u32);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed(), "PPM decode should be zero-copy");
}

// ── P1 (ASCII PBM) ──────────────────────────────────────────────────

#[test]
fn p1_ascii_pbm_2x2() {
    let data = b"P1\n2 2\n1 0\n0 1\n";
    let decoded = decode(data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    // 1=black(0), 0=white(255)
    assert_eq!(decoded.pixels(), &[0, 255, 255, 0]);
}

#[test]
fn p1_ascii_pbm_with_comments() {
    let data = b"P1\n# comment\n3 1\n1 0 1\n";
    let decoded = decode(data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[0, 255, 0]);
}

#[test]
fn p1_ascii_pbm_1x1() {
    let data = b"P1\n1 1\n0\n";
    let decoded = decode(data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[255]);
}

// ── P2 (ASCII PGM) ──────────────────────────────────────────────────

#[test]
fn p2_ascii_pgm_3x2() {
    let data = b"P2\n3 2\n255\n0 128 255\n64 192 32\n";
    let decoded = decode(data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    assert_eq!(decoded.pixels(), &[0, 128, 255, 64, 192, 32]);
}

#[test]
fn p2_ascii_pgm_maxval_scaling() {
    // maxval=15, values scale: 0→0, 8→136, 15→255
    let data = b"P2\n3 1\n15\n0 8 15\n";
    let decoded = decode(data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels()[0], 0);
    assert!(decoded.pixels()[1] > 120 && decoded.pixels()[1] < 150); // ~136
    assert_eq!(decoded.pixels()[2], 255);
}

#[test]
fn p2_ascii_pgm_with_comments() {
    let data = b"P2\n# A comment\n2 1\n# maxval\n255\n100 200\n";
    let decoded = decode(data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[100, 200]);
}

// ── P3 (ASCII PPM) ──────────────────────────────────────────────────

#[test]
fn p3_ascii_ppm_2x1() {
    let data = b"P3\n2 1\n255\n255 0 0 0 255 0\n";
    let decoded = decode(data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 1);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &[255, 0, 0, 0, 255, 0]);
}

#[test]
fn p3_ascii_ppm_multiline() {
    // Values can span multiple lines
    let data = b"P3\n1 2\n255\n10\n20\n30\n40\n50\n60\n";
    let decoded = decode(data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[10, 20, 30, 40, 50, 60]);
}

#[test]
fn p3_ascii_ppm_maxval_scaling() {
    let data = b"P3\n1 1\n100\n50 100 0\n";
    let decoded = decode(data, Unstoppable).unwrap();
    // 50/100*255 ≈ 128, 100/100*255 = 255, 0/100*255 = 0
    assert!(decoded.pixels()[0] > 125 && decoded.pixels()[0] < 131);
    assert_eq!(decoded.pixels()[1], 255);
    assert_eq!(decoded.pixels()[2], 0);
}

// ── P4 (binary PBM) ────────────────────────────────────────────────

#[test]
fn p4_binary_pbm_8x1() {
    // 8 pixels in one byte: 0b10101010 = pixels: B,W,B,W,B,W,B,W
    let mut data = Vec::from(&b"P4\n8 1\n"[..]);
    data.push(0b10101010);
    let decoded = decode(&data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 8);
    assert_eq!(decoded.height, 1);
    assert_eq!(decoded.pixels(), &[0, 255, 0, 255, 0, 255, 0, 255]);
}

#[test]
fn p4_binary_pbm_3x1_padded() {
    // 3 pixels = 3 bits used, 5 bits padding in byte
    // 0b11100000 = pixels: B,B,B (+ 5 padding bits)
    let mut data = Vec::from(&b"P4\n3 1\n"[..]);
    data.push(0b11100000);
    let decoded = decode(&data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.pixels(), &[0, 0, 0]);
}

#[test]
fn p4_binary_pbm_2x2() {
    // Row 1: 0b10000000 → B,W (6 padding bits)
    // Row 2: 0b01000000 → W,B (6 padding bits)
    let mut data = Vec::from(&b"P4\n2 2\n"[..]);
    data.push(0b10000000);
    data.push(0b01000000);
    let decoded = decode(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[0, 255, 255, 0]);
}

#[test]
fn p4_binary_pbm_16x1() {
    // 16 pixels = 2 bytes, all white (0x00, 0x00)
    let mut data = Vec::from(&b"P4\n16 1\n"[..]);
    data.push(0x00);
    data.push(0x00);
    let decoded = decode(&data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 16);
    assert!(decoded.pixels().iter().all(|&p| p == 255));
}

#[test]
fn p4_binary_pbm_all_black() {
    let mut data = Vec::from(&b"P4\n8 1\n"[..]);
    data.push(0xFF);
    let decoded = decode(&data, Unstoppable).unwrap();
    assert!(decoded.pixels().iter().all(|&p| p == 0));
}

// ── Format detection ────────────────────────────────────────────────

// ── P1-P4 error cases ───────────────────────────────────────────────

#[test]
fn p1_truncated() {
    assert!(decode(b"P1\n2 2\n1 0\n", Unstoppable).is_err()); // only 2 of 4 pixels
}

#[test]
fn p1_invalid_char() {
    assert!(decode(b"P1\n1 1\n2\n", Unstoppable).is_err()); // '2' invalid for PBM
}

#[test]
fn p2_truncated() {
    assert!(decode(b"P2\n2 1\n255\n42\n", Unstoppable).is_err()); // 1 of 2 samples
}

#[test]
fn p3_truncated() {
    assert!(decode(b"P3\n1 1\n255\n10 20\n", Unstoppable).is_err()); // 2 of 3 channels
}

#[test]
fn p4_truncated() {
    assert!(decode(b"P4\n16 1\n\x00", Unstoppable).is_err()); // need 2 bytes, got 1
}

#[test]
fn p2_zero_dimensions() {
    assert!(decode(b"P2\n0 1\n255\n", Unstoppable).is_err());
}

#[test]
fn detect_format_p1_p4() {
    assert_eq!(detect_format(b"P1\n1 1\n0"), Some(ImageFormat::Pnm));
    assert_eq!(detect_format(b"P2\n1 1\n255\n0"), Some(ImageFormat::Pnm));
    assert_eq!(
        detect_format(b"P3\n1 1\n255\n0 0 0"),
        Some(ImageFormat::Pnm)
    );
    assert_eq!(detect_format(b"P4\n1 1\n\x00"), Some(ImageFormat::Pnm));
}

#[test]
fn pam_roundtrip_rgba8() {
    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 128, 128, 128, 255,
    ];
    let encoded = encode_pam(&pixels, 2, 2, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[test]
fn pgm_roundtrip_gray8() {
    let pixels = vec![0, 64, 128, 192, 255, 100];
    let encoded = encode_pgm(&pixels, 3, 2, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed());
}

#[cfg(feature = "bmp")]
#[test]
fn bmp_roundtrip_rgb8() {
    let pixels = vec![
        255, 0, 0, 0, 255, 0, 0, 0, 255, 128, 128, 128, 64, 64, 64, 0, 0, 0,
    ];
    let encoded = encode_bmp(&pixels, 3, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert_eq!(&encoded[0..2], b"BM");

    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(!decoded.is_borrowed());

    // Auto-detect now recognizes BMP via "BM" magic
    let auto_decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(auto_decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "bmp")]
#[test]
fn bmp_roundtrip_rgba8() {
    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 128, 128, 128, 255,
    ];
    let encoded = encode_bmp_rgba(&pixels, 2, 2, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode_bmp(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[test]
fn crafted_p3_oversized_dimensions_returns_error() {
    // Fuzz-found crash artifact: P3 header with width=424011, causing OOM
    // from unbounded Vec::with_capacity before the hard cap was added.
    let artifact: &[u8] = &[
        0x50, 0x33, 0x34, 0x32, 0x34, 0x30, 0x31, 0x31, 0x23, 0x23, 0x50, 0x35, 0x32, 0x31, 0x31,
        0x30, 0x50, 0x35, 0x32, 0x31, 0x31, 0x30, 0x31, 0x31, 0x30, 0x0a, 0x30, 0x31, 0x31, 0x32,
        0x31, 0x31, 0x30, 0x0a, 0x30, 0x31, 0x31, 0x32, 0x32, 0x50, 0x32, 0x00, 0x35, 0x32, 0x31,
        0x31, 0x30, 0x31, 0x31, 0x30, 0x0a, 0x30, 0x31, 0x31, 0x32, 0x32, 0x32, 0x32, 0x32, 0x30,
        0x30, 0x30,
    ];
    let result = decode(artifact, Unstoppable);
    assert!(
        result.is_err(),
        "crafted P3 with huge dimensions must return error, not OOM"
    );
}

#[test]
fn limits_reject_large() {
    let encoded = encode_ppm(&[255u8; 6], 1, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_pixels: Some(1),
        ..Default::default()
    };
    let result = decode_with_limits(&encoded, &limits, Unstoppable);
    assert!(result.is_err());
    match result.unwrap_err() {
        BitmapError::LimitExceeded(_) => {}
        other => panic!("expected LimitExceeded, got {other:?}"),
    }
}

#[test]
fn detect_format_pnm() {
    let ppm = encode_ppm(&[255u8; 6], 2, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert_eq!(detect_format(&ppm), Some(ImageFormat::Pnm));

    let pgm = encode_pgm(&[128u8; 4], 2, 2, PixelLayout::Gray8, Unstoppable).unwrap();
    assert_eq!(detect_format(&pgm), Some(ImageFormat::Pnm));

    let pam = encode_pam(&[0u8; 4], 1, 1, PixelLayout::Rgba8, Unstoppable).unwrap();
    assert_eq!(detect_format(&pam), Some(ImageFormat::Pnm));

    let pfm = encode_pfm(&[0u8; 4], 1, 1, PixelLayout::GrayF32, Unstoppable).unwrap();
    assert_eq!(detect_format(&pfm), Some(ImageFormat::Pnm));
}

#[test]
fn detect_format_farbfeld() {
    let ff = encode_farbfeld(&[0u8; 8], 1, 1, PixelLayout::Rgba16, Unstoppable).unwrap();
    assert_eq!(detect_format(&ff), Some(ImageFormat::Farbfeld));
}

#[cfg(feature = "bmp")]
#[test]
fn detect_format_bmp() {
    let bmp = encode_bmp(&[255u8; 3], 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert_eq!(detect_format(&bmp), Some(ImageFormat::Bmp));
}

#[test]
fn detect_format_unknown() {
    assert_eq!(detect_format(&[]), None);
    assert_eq!(detect_format(&[0]), None);
    assert_eq!(detect_format(b"JPEG"), None);
}

#[test]
fn decode_unrecognized_format() {
    let result = decode(b"NOTAFORMAT", Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnrecognizedFormat)));
}

#[test]
fn pam_encode_bgra8() {
    // BGRA pixels: blue=100, green=150, red=200, alpha=255
    let bgra = vec![100u8, 150, 200, 255];
    let encoded = encode_pam(&bgra, 1, 1, PixelLayout::Bgra8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    // Should be swizzled to RGBA: red=200, green=150, blue=100, alpha=255
    assert_eq!(decoded.pixels(), &[200, 150, 100, 255]);
}

#[test]
fn pam_encode_bgrx8() {
    // BGRX pixels: blue=50, green=100, red=150, x=0 (padding)
    let bgrx = vec![50u8, 100, 150, 0];
    let encoded = encode_pam(&bgrx, 1, 1, PixelLayout::Bgrx8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    // Should be swizzled to RGBA with A=255
    assert_eq!(decoded.pixels(), &[150, 100, 50, 255]);
}

#[test]
fn pam_encode_bgr8() {
    // BGR pixels: blue=10, green=20, red=30
    let bgr = vec![10u8, 20, 30];
    let encoded = encode_pam(&bgr, 1, 1, PixelLayout::Bgr8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    // Should be swizzled to RGB
    assert_eq!(decoded.pixels(), &[30, 20, 10]);
}

#[test]
fn farbfeld_encode_bgra8() {
    // BGRA: blue=100, green=150, red=200, alpha=255
    let bgra = vec![100u8, 150, 200, 255];
    let encoded = encode_farbfeld(&bgra, 1, 1, PixelLayout::Bgra8, Unstoppable).unwrap();
    // Decode and verify channel order
    let decoded = decode_farbfeld(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba16);
    let px = decoded.pixels();
    // RGBA16 big-endian: R=200*257, G=150*257, B=100*257, A=255*257
    let r = u16::from_be_bytes([px[0], px[1]]);
    let g = u16::from_be_bytes([px[2], px[3]]);
    let b = u16::from_be_bytes([px[4], px[5]]);
    let a = u16::from_be_bytes([px[6], px[7]]);
    assert_eq!(r, 200 * 257);
    assert_eq!(g, 150 * 257);
    assert_eq!(b, 100 * 257);
    assert_eq!(a, 255 * 257);
}

#[test]
fn farbfeld_encode_bgr8() {
    // BGR: blue=10, green=20, red=30
    let bgr = vec![10u8, 20, 30];
    let encoded = encode_farbfeld(&bgr, 1, 1, PixelLayout::Bgr8, Unstoppable).unwrap();
    let decoded = decode_farbfeld(&encoded, Unstoppable).unwrap();
    let px = decoded.pixels();
    let r = u16::from_be_bytes([px[0], px[1]]);
    let g = u16::from_be_bytes([px[2], px[3]]);
    let b = u16::from_be_bytes([px[4], px[5]]);
    let a = u16::from_be_bytes([px[6], px[7]]);
    assert_eq!(r, 30 * 257);
    assert_eq!(g, 20 * 257);
    assert_eq!(b, 10 * 257);
    assert_eq!(a, 65535);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_roundtrip_rgb8() {
    let pixels = vec![
        255, 0, 0, 0, 255, 0, 0, 0, 255, 128, 128, 128, 64, 64, 64, 0, 0, 0,
    ];
    let encoded = encode_qoi(&pixels, 3, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert_eq!(&encoded[..4], b"qoif");

    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);

    // Auto-detect
    let auto = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(auto.pixels(), &pixels[..]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_roundtrip_rgba8() {
    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 128, 128, 128, 255,
    ];
    let encoded = encode_qoi(&pixels, 2, 2, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_roundtrip_bgra8() {
    // BGRA: blue=100, green=150, red=200, alpha=255
    let bgra = vec![100u8, 150, 200, 255];
    let encoded = encode_qoi(&bgra, 1, 1, PixelLayout::Bgra8, Unstoppable).unwrap();
    // QOI stores as RGBA, so decode gives RGBA
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    // Should be swizzled: R=200, G=150, B=100, A=255
    assert_eq!(decoded.pixels(), &[200, 150, 100, 255]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_limits_reject() {
    let pixels = vec![0u8; 100 * 100 * 3];
    let encoded = encode_qoi(&pixels, 100, 100, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_pixels: Some(50),
        ..Default::default()
    };
    let result = decode_qoi_with_limits(&encoded, &limits, Unstoppable);
    assert!(matches!(result, Err(BitmapError::LimitExceeded(_))));
}

#[cfg(feature = "qoi")]
#[test]
fn detect_format_qoi() {
    let pixels = vec![0u8; 3];
    let encoded = encode_qoi(&pixels, 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert_eq!(detect_format(&encoded), Some(ImageFormat::Qoi));
}

// ── QOI edge cases ──────────────────────────────────────────────────

#[cfg(feature = "qoi")]
#[test]
fn qoi_1x1_rgb() {
    let pixels = vec![42u8, 99, 200];
    let encoded = encode_qoi(&pixels, 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 1);
    assert_eq!(decoded.height, 1);
    assert_eq!(decoded.pixels(), &[42, 99, 200]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_1x1_rgba() {
    let pixels = vec![10u8, 20, 30, 128];
    let encoded = encode_qoi(&pixels, 1, 1, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &[10, 20, 30, 128]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_wide_image() {
    // 200x1 — single row, many pixels
    let pixels: Vec<u8> = (0..200u8).flat_map(|i| [i, 255 - i, i / 2]).collect();
    let encoded = encode_qoi(&pixels, 200, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 200);
    assert_eq!(decoded.height, 1);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_tall_image() {
    // 1x200 — many rows, one pixel each
    let pixels: Vec<u8> = (0..200u8).flat_map(|i| [i, i, i]).collect();
    let encoded = encode_qoi(&pixels, 1, 200, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 1);
    assert_eq!(decoded.height, 200);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_large_enough_for_cancellation_check() {
    // 10x32 = 320 pixels, 32 rows — exercises the row%16==0 check path
    // Use varying pixel data to avoid massive RLE runs
    let pixels: Vec<u8> = (0..10 * 32)
        .flat_map(|i| {
            let v = (i % 256) as u8;
            [v, v.wrapping_mul(3), v.wrapping_mul(7), 255]
        })
        .collect();
    let encoded = encode_qoi(&pixels, 10, 32, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

// ── QOI encode layout coverage ──────────────────────────────────────

#[cfg(feature = "qoi")]
#[test]
fn qoi_encode_bgr8() {
    // BGR: blue=10, green=20, red=30
    let bgr = vec![10u8, 20, 30];
    let encoded = encode_qoi(&bgr, 1, 1, PixelLayout::Bgr8, Unstoppable).unwrap();
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    // Should be swizzled: R=30, G=20, B=10
    assert_eq!(decoded.pixels(), &[30, 20, 10]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_encode_bgrx8() {
    // BGRX: blue=50, green=100, red=150, x=0 (padding)
    let bgrx = vec![50u8, 100, 150, 0];
    let encoded = encode_qoi(&bgrx, 1, 1, PixelLayout::Bgrx8, Unstoppable).unwrap();
    let decoded = decode_qoi(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    // Swizzled to RGBA with A=255
    assert_eq!(decoded.pixels(), &[150, 100, 50, 255]);
}

// ── QOI error handling ──────────────────────────────────────────────

#[cfg(feature = "qoi")]
#[test]
fn qoi_decode_empty_input() {
    let result = decode_qoi(&[], Unstoppable);
    assert!(result.is_err());
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_decode_truncated_header() {
    // Less than 14 bytes (QOI header size)
    let result = decode_qoi(b"qoif12345", Unstoppable);
    assert!(result.is_err());
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_decode_wrong_magic() {
    let result = decode_qoi(b"qoix\x00\x00\x00\x01\x00\x00\x00\x01\x03\x00", Unstoppable);
    assert!(result.is_err());
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_decode_truncated_pixel_data() {
    // Valid header for 2x2 RGB but no pixel data after header
    let mut data = Vec::new();
    data.extend_from_slice(b"qoif");
    data.extend_from_slice(&2u32.to_be_bytes()); // width
    data.extend_from_slice(&2u32.to_be_bytes()); // height
    data.push(3); // channels = RGB
    data.push(0); // colorspace = sRGB
    // No pixel data — decoder should error
    let result = decode_qoi(&data, Unstoppable);
    assert!(result.is_err());
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_encode_unsupported_layout_gray8() {
    let result = encode_qoi(&[128u8], 1, 1, PixelLayout::Gray8, Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnsupportedVariant(_))));
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_encode_unsupported_layout_rgba16() {
    let result = encode_qoi(&[0u8; 8], 1, 1, PixelLayout::Rgba16, Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnsupportedVariant(_))));
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_encode_unsupported_layout_grayf32() {
    let result = encode_qoi(&[0u8; 4], 1, 1, PixelLayout::GrayF32, Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnsupportedVariant(_))));
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_encode_unsupported_layout_rgbf32() {
    let result = encode_qoi(&[0u8; 12], 1, 1, PixelLayout::RgbF32, Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnsupportedVariant(_))));
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_encode_buffer_too_small() {
    // Claim 2x2 RGB (12 bytes needed) but only provide 6
    let result = encode_qoi(&[0u8; 6], 2, 2, PixelLayout::Rgb8, Unstoppable);
    assert!(matches!(result, Err(BitmapError::BufferTooSmall { .. })));
}

// ── QOI limit variants ──────────────────────────────────────────────

#[cfg(feature = "qoi")]
#[test]
fn qoi_limits_max_width() {
    let pixels = vec![0u8; 100 * 10 * 3];
    let encoded = encode_qoi(&pixels, 100, 10, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_width: Some(50),
        ..Default::default()
    };
    let result = decode_qoi_with_limits(&encoded, &limits, Unstoppable);
    assert!(matches!(result, Err(BitmapError::LimitExceeded(_))));
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_limits_max_height() {
    let pixels = vec![0u8; 10 * 100 * 3];
    let encoded = encode_qoi(&pixels, 10, 100, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_height: Some(50),
        ..Default::default()
    };
    let result = decode_qoi_with_limits(&encoded, &limits, Unstoppable);
    assert!(matches!(result, Err(BitmapError::LimitExceeded(_))));
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_limits_max_memory() {
    let pixels = vec![0u8; 100 * 100 * 3];
    let encoded = encode_qoi(&pixels, 100, 100, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_memory_bytes: Some(100), // way too small for 30000 bytes output
        ..Default::default()
    };
    let result = decode_qoi_with_limits(&encoded, &limits, Unstoppable);
    assert!(matches!(result, Err(BitmapError::LimitExceeded(_))));
}

// ── QOI cancellation ────────────────────────────────────────────────

#[cfg(feature = "qoi")]
#[test]
fn qoi_decode_cancellation() {
    struct AlreadyStopped;
    impl enough::Stop for AlreadyStopped {
        fn check(&self) -> Result<(), enough::StopReason> {
            Err(enough::StopReason::Cancelled)
        }
    }

    let pixels = vec![0u8; 10 * 32 * 3]; // 32 rows to hit the row%16 check
    let encoded = encode_qoi(&pixels, 10, 32, PixelLayout::Rgb8, Unstoppable).unwrap();

    let result = decode_qoi(&encoded, AlreadyStopped);
    assert!(matches!(result, Err(BitmapError::Cancelled(_))));
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_encode_cancellation() {
    struct AlreadyStopped;
    impl enough::Stop for AlreadyStopped {
        fn check(&self) -> Result<(), enough::StopReason> {
            Err(enough::StopReason::Cancelled)
        }
    }

    let pixels = vec![0u8; 10 * 32 * 3];
    let result = encode_qoi(&pixels, 10, 32, PixelLayout::Rgb8, AlreadyStopped);
    assert!(matches!(result, Err(BitmapError::Cancelled(_))));
}

// ── QOI auto-detect decode ──────────────────────────────────────────

#[cfg(feature = "qoi")]
#[test]
fn qoi_auto_detect_decode() {
    let pixels = vec![255u8, 0, 0, 0, 255, 0];
    let encoded = encode_qoi(&pixels, 2, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    // decode() should auto-detect QOI from magic and dispatch
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_auto_detect_with_limits() {
    let pixels = vec![0u8; 3];
    let encoded = encode_qoi(&pixels, 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_pixels: Some(100),
        ..Default::default()
    };
    let decoded = decode_with_limits(&encoded, &limits, Unstoppable).unwrap();
    assert_eq!(decoded.width, 1);
}

#[test]
fn into_owned_works() {
    let pixels = vec![1u8, 2, 3];
    let encoded = encode_pgm(&pixels, 1, 3, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert!(decoded.is_borrowed());
    let owned = decoded.into_owned();
    assert!(!owned.is_borrowed());
    assert_eq!(owned.pixels(), &[1, 2, 3]);
}

// ── TGA tests ──────────────────────────────────────────────────────

#[cfg(feature = "tga")]
#[test]
fn tga_roundtrip_rgb8() {
    // 3x2 checkerboard
    let pixels = vec![
        255, 0, 0, 0, 255, 0, 0, 0, 255, 128, 128, 128, 64, 64, 64, 0, 0, 0,
    ];
    let encoded = encode_tga(&pixels, 3, 2, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "tga")]
#[test]
fn tga_roundtrip_rgba8() {
    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 128, 128, 128, 255,
    ];
    let encoded = encode_tga(&pixels, 2, 2, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "tga")]
#[test]
fn tga_roundtrip_gray8() {
    let pixels = vec![0, 64, 128, 192, 255, 100];
    let encoded = encode_tga(&pixels, 3, 2, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[cfg(feature = "tga")]
#[test]
fn tga_1x1() {
    // Minimal image — single RGB pixel
    let pixels = vec![42u8, 99, 200];
    let encoded = encode_tga(&pixels, 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 1);
    assert_eq!(decoded.height, 1);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &[42, 99, 200]);
}

#[cfg(feature = "tga")]
#[test]
fn tga_encode_bgr8() {
    // BGR input — TGA stores BGR natively, so encode is direct copy
    let bgr = vec![10u8, 20, 30]; // B=10, G=20, R=30
    let encoded = encode_tga(&bgr, 1, 1, PixelLayout::Bgr8, Unstoppable).unwrap();
    // Decode gives RGB
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    // Should be swizzled to RGB: R=30, G=20, B=10
    assert_eq!(decoded.pixels(), &[30, 20, 10]);
}

#[cfg(feature = "tga")]
#[test]
fn tga_encode_bgra8() {
    // BGRA input — TGA stores BGRA natively
    let bgra = vec![100u8, 150, 200, 255]; // B=100, G=150, R=200, A=255
    let encoded = encode_tga(&bgra, 1, 1, PixelLayout::Bgra8, Unstoppable).unwrap();
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    // Swizzled to RGBA: R=200, G=150, B=100, A=255
    assert_eq!(decoded.pixels(), &[200, 150, 100, 255]);
}

#[cfg(feature = "tga")]
#[test]
fn tga_limits_reject() {
    let pixels = vec![0u8; 100 * 100 * 3];
    let encoded = encode_tga(&pixels, 100, 100, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_pixels: Some(50),
        ..Default::default()
    };
    let result = decode_tga_with_limits(&encoded, &limits, Unstoppable);
    assert!(matches!(result, Err(BitmapError::LimitExceeded(_))));
}

#[cfg(feature = "tga")]
#[test]
fn tga_decode_empty() {
    let result = decode_tga(&[], Unstoppable);
    assert!(result.is_err());
}

#[cfg(feature = "tga")]
#[test]
fn tga_decode_truncated() {
    // Valid-looking header but no pixel data
    let mut data = vec![0u8; 18];
    data[2] = 2; // image_type = truecolor
    data[12] = 10; // width = 10
    data[14] = 10; // height = 10
    data[16] = 24; // pixel_depth = 24
    let result = decode_tga(&data, Unstoppable);
    assert!(result.is_err());
}

#[cfg(feature = "tga")]
#[test]
fn tga_encode_unsupported_layout() {
    let result = encode_tga(&[0u8; 12], 1, 1, PixelLayout::RgbF32, Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnsupportedVariant(_))));

    let result = encode_tga(&[0u8; 8], 1, 1, PixelLayout::Rgba16, Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnsupportedVariant(_))));

    let result = encode_tga(&[0u8; 4], 1, 1, PixelLayout::GrayF32, Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnsupportedVariant(_))));
}

#[cfg(feature = "tga")]
#[test]
fn detect_format_tga() {
    let pixels = vec![255u8, 0, 0, 0, 255, 0];
    let encoded = encode_tga(&pixels, 2, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert_eq!(detect_format(&encoded), Some(ImageFormat::Tga));
}

#[cfg(feature = "tga")]
#[test]
fn tga_auto_detect_decode() {
    let pixels = vec![255u8, 0, 0, 0, 255, 0];
    let encoded = encode_tga(&pixels, 2, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    // decode() should auto-detect TGA from header heuristics and dispatch
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

// ── HDR roundtrip tests ────────────────────────────────────────────

/// Helper: build f32 RGB pixel bytes from f32 triples.
#[cfg(feature = "hdr")]
fn make_rgbf32_pixels(values: &[(f32, f32, f32)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 12);
    for &(r, g, b) in values {
        out.extend_from_slice(&r.to_le_bytes());
        out.extend_from_slice(&g.to_le_bytes());
        out.extend_from_slice(&b.to_le_bytes());
    }
    out
}

/// Helper: read f32 RGB triples from pixel bytes.
#[cfg(feature = "hdr")]
fn read_rgbf32_pixels(data: &[u8]) -> Vec<(f32, f32, f32)> {
    data.chunks_exact(12)
        .map(|chunk| {
            let r = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let g = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            let b = f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
            (r, g, b)
        })
        .collect()
}

/// Assert two f32 values are within RGBE precision (~1% per channel).
#[cfg(feature = "hdr")]
fn assert_f32_close(actual: f32, expected: f32, label: &str) {
    let eps = 0.02 * expected.abs().max(0.01);
    assert!(
        (actual - expected).abs() <= eps,
        "{label}: expected {expected}, got {actual} (eps={eps})"
    );
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_roundtrip_rgbf32() {
    let values = vec![
        (1.0, 0.5, 0.25),
        (0.0, 0.0, 0.0),
        (2.0, 3.0, 4.0),
        (0.1, 0.2, 0.3),
        (100.0, 200.0, 50.0),
        (0.001, 0.002, 0.003),
        (10.0, 10.0, 10.0),
        (0.5, 0.5, 0.5),
        // 2 more to make a 5x2 image (width < 8, flat path)
        (1.0, 1.0, 1.0),
        (0.75, 0.75, 0.75),
    ];
    let pixels = make_rgbf32_pixels(&values);
    let encoded = encode_hdr(&pixels, 5, 2, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 5);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.layout, PixelLayout::RgbF32);

    let result = read_rgbf32_pixels(decoded.pixels());
    for (i, (&(er, eg, eb), &(ar, ag, ab))) in values.iter().zip(result.iter()).enumerate() {
        if er == 0.0 && eg == 0.0 && eb == 0.0 {
            assert_eq!(ar, 0.0, "pixel {i} R");
            assert_eq!(ag, 0.0, "pixel {i} G");
            assert_eq!(ab, 0.0, "pixel {i} B");
        } else {
            assert_f32_close(ar, er, &format!("pixel {i} R"));
            assert_f32_close(ag, eg, &format!("pixel {i} G"));
            assert_f32_close(ab, eb, &format!("pixel {i} B"));
        }
    }
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_1x1() {
    let pixels = make_rgbf32_pixels(&[(1.0, 2.0, 3.0)]);
    let encoded = encode_hdr(&pixels, 1, 1, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 1);
    assert_eq!(decoded.height, 1);
    assert_eq!(decoded.layout, PixelLayout::RgbF32);
    let result = read_rgbf32_pixels(decoded.pixels());
    assert_f32_close(result[0].0, 1.0, "R");
    assert_f32_close(result[0].1, 2.0, "G");
    assert_f32_close(result[0].2, 3.0, "B");
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_wide_image() {
    // Width=64, height=2 -- exercises the new-style RLE path (width >= 8)
    let mut values = Vec::with_capacity(128);
    for i in 0..128 {
        let v = (i as f32 + 1.0) * 0.1;
        values.push((v, v * 0.5, v * 0.25));
    }
    let pixels = make_rgbf32_pixels(&values);
    let encoded = encode_hdr(&pixels, 64, 2, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 64);
    assert_eq!(decoded.height, 2);

    let result = read_rgbf32_pixels(decoded.pixels());
    assert_eq!(result.len(), 128);
    for (i, (&(er, eg, eb), &(ar, ag, ab))) in values.iter().zip(result.iter()).enumerate() {
        assert_f32_close(ar, er, &format!("pixel {i} R"));
        assert_f32_close(ag, eg, &format!("pixel {i} G"));
        assert_f32_close(ab, eb, &format!("pixel {i} B"));
    }
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_decode_empty() {
    let result = decode_hdr(&[], Unstoppable);
    assert!(result.is_err());
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_decode_truncated() {
    // Valid magic but truncated before resolution line
    let result = decode_hdr(b"#?RADIANCE\n", Unstoppable);
    assert!(result.is_err());
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_limits_reject() {
    let pixels = make_rgbf32_pixels(&vec![(1.0, 1.0, 1.0); 100]);
    let encoded = encode_hdr(&pixels, 10, 10, PixelLayout::RgbF32, Unstoppable).unwrap();
    let limits = Limits {
        max_pixels: Some(50),
        ..Default::default()
    };
    let result = decode_hdr_with_limits(&encoded, &limits, Unstoppable);
    assert!(matches!(result, Err(BitmapError::LimitExceeded(_))));
}

#[cfg(feature = "hdr")]
#[test]
fn detect_format_hdr() {
    let pixels = make_rgbf32_pixels(&[(1.0, 1.0, 1.0)]);
    let encoded = encode_hdr(&pixels, 1, 1, PixelLayout::RgbF32, Unstoppable).unwrap();
    assert_eq!(detect_format(&encoded), Some(ImageFormat::Hdr));

    // Also test raw magic bytes
    assert_eq!(
        detect_format(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n"),
        Some(ImageFormat::Hdr)
    );
    assert_eq!(
        detect_format(b"#?RGBE\nFORMAT=32-bit_rle_rgbe\n"),
        Some(ImageFormat::Hdr)
    );
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_auto_detect_decode() {
    let pixels = make_rgbf32_pixels(&[(0.5, 1.0, 1.5)]);
    let encoded = encode_hdr(&pixels, 1, 1, PixelLayout::RgbF32, Unstoppable).unwrap();
    // decode() should auto-detect HDR from magic and dispatch
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::RgbF32);
    let result = read_rgbf32_pixels(decoded.pixels());
    assert_f32_close(result[0].0, 0.5, "R");
    assert_f32_close(result[0].1, 1.0, "G");
    assert_f32_close(result[0].2, 1.5, "B");
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_encode_rgb8() {
    // Test Rgb8 -> HDR -> decode roundtrip
    let rgb8_pixels = vec![255u8, 128, 64, 0, 0, 0, 200, 100, 50];
    let encoded = encode_hdr(&rgb8_pixels, 3, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::RgbF32);
    let result = read_rgbf32_pixels(decoded.pixels());

    // First pixel: 255/255=1.0, 128/255~0.502, 64/255~0.251
    assert_f32_close(result[0].0, 1.0, "px0 R");
    assert_f32_close(result[0].1, 128.0 / 255.0, "px0 G");
    assert_f32_close(result[0].2, 64.0 / 255.0, "px0 B");

    // Second pixel: all zero
    assert_eq!(result[1].0, 0.0);
    assert_eq!(result[1].1, 0.0);
    assert_eq!(result[1].2, 0.0);

    // Third pixel: 200/255, 100/255, 50/255
    assert_f32_close(result[2].0, 200.0 / 255.0, "px2 R");
    assert_f32_close(result[2].1, 100.0 / 255.0, "px2 G");
    assert_f32_close(result[2].2, 50.0 / 255.0, "px2 B");
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_encode_unsupported_layout() {
    let result = encode_hdr(&[0u8; 4], 1, 1, PixelLayout::Rgba8, Unstoppable);
    assert!(matches!(result, Err(BitmapError::UnsupportedVariant(_))));
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_cancellation() {
    struct AlreadyStopped;
    impl enough::Stop for AlreadyStopped {
        fn check(&self) -> Result<(), enough::StopReason> {
            Err(enough::StopReason::Cancelled)
        }
    }

    // Encode should cancel
    let pixels = make_rgbf32_pixels(&vec![(1.0, 1.0, 1.0); 100]);
    let result = encode_hdr(&pixels, 10, 10, PixelLayout::RgbF32, AlreadyStopped);
    assert!(matches!(result, Err(BitmapError::Cancelled(_))));

    // Decode should cancel (use a valid encoded file)
    let encoded = encode_hdr(&pixels, 10, 10, PixelLayout::RgbF32, Unstoppable).unwrap();
    let result = decode_hdr(&encoded, AlreadyStopped);
    assert!(matches!(result, Err(BitmapError::Cancelled(_))));
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_1000x1000_roundtrip() {
    let w = 1000u32;
    let h = 1000u32;
    let pixels: Vec<u8> = (0..w * h)
        .flat_map(|i| {
            let v = (i % 1000) as f32 / 1000.0;
            let mut p = [0u8; 12];
            p[0..4].copy_from_slice(&v.to_le_bytes());
            p[4..8].copy_from_slice(&(v * 0.5).to_le_bytes());
            p[8..12].copy_from_slice(&(v * 0.25).to_le_bytes());
            p
        })
        .collect();
    let encoded = encode_hdr(&pixels, w, h, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, w);
    assert_eq!(decoded.height, h);
}
