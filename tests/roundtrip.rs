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
fn into_owned_works() {
    let pixels = vec![1u8, 2, 3];
    let encoded = encode_pgm(&pixels, 1, 3, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    assert!(decoded.is_borrowed());
    let owned = decoded.into_owned();
    assert!(!owned.is_borrowed());
    assert_eq!(owned.pixels(), &[1, 2, 3]);
}
