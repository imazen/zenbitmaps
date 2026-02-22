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
