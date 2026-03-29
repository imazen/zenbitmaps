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
