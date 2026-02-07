use enough::Unstoppable;
use zenpnm::*;

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

    let encoded = EncodeRequest::pnm(pnm::PnmFormat::Ppm)
        .encode(&pixels, w as u32, h as u32, PixelLayout::Rgb8, Unstoppable)
        .unwrap();

    let decoded = DecodeRequest::new(&encoded).decode(Unstoppable).unwrap();
    assert_eq!(decoded.width, w as u32);
    assert_eq!(decoded.height, h as u32);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    // P6 with maxval=255 should be zero-copy
    assert!(decoded.is_borrowed(), "PPM decode should be zero-copy");
}

#[test]
fn pam_roundtrip_rgba8() {
    let w = 2;
    let h = 2;
    let pixels = vec![
        255, 0, 0, 255, // red
        0, 255, 0, 128, // green semi-transparent
        0, 0, 255, 0, // blue transparent
        128, 128, 128, 255, // gray
    ];

    let encoded = EncodeRequest::pnm(pnm::PnmFormat::Pam)
        .encode(&pixels, w as u32, h as u32, PixelLayout::Rgba8, Unstoppable)
        .unwrap();

    let decoded = DecodeRequest::new(&encoded).decode(Unstoppable).unwrap();
    assert_eq!(decoded.width, w as u32);
    assert_eq!(decoded.height, h as u32);
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed(), "PAM decode should be zero-copy");
}

#[test]
fn pgm_roundtrip_gray8() {
    let w = 3;
    let h = 2;
    let pixels = vec![0, 64, 128, 192, 255, 100];

    let encoded = EncodeRequest::pnm(pnm::PnmFormat::Pgm)
        .encode(&pixels, w as u32, h as u32, PixelLayout::Gray8, Unstoppable)
        .unwrap();

    let decoded = DecodeRequest::new(&encoded).decode(Unstoppable).unwrap();
    assert_eq!(decoded.width, w as u32);
    assert_eq!(decoded.height, h as u32);
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    assert!(decoded.is_borrowed(), "PGM decode should be zero-copy");
}

#[test]
fn bmp_roundtrip_rgb8() {
    let w = 3;
    let h = 2;
    let pixels = vec![
        255, 0, 0, 0, 255, 0, 0, 0, 255, // row 0: R G B
        128, 128, 128, 64, 64, 64, 0, 0, 0, // row 1: gray dark black
    ];

    let encoded = EncodeRequest::bmp()
        .encode(&pixels, w as u32, h as u32, PixelLayout::Rgb8, Unstoppable)
        .unwrap();

    assert_eq!(&encoded[0..2], b"BM");

    let decoded = DecodeRequest::new(&encoded).decode(Unstoppable).unwrap();
    assert_eq!(decoded.width, w as u32);
    assert_eq!(decoded.height, h as u32);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &pixels[..]);
    // BMP cannot be zero-copy (BGR→RGB, row flip)
    assert!(!decoded.is_borrowed());
}

#[test]
fn bmp_roundtrip_rgba8() {
    let w = 2;
    let h = 2;
    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 128, // row 0
        0, 0, 255, 64, 128, 128, 128, 255, // row 1
    ];

    let encoded = EncodeRequest::bmp_with_alpha()
        .encode(&pixels, w as u32, h as u32, PixelLayout::Rgba8, Unstoppable)
        .unwrap();

    let decoded = DecodeRequest::new(&encoded).decode(Unstoppable).unwrap();
    assert_eq!(decoded.width, w as u32);
    assert_eq!(decoded.height, h as u32);
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[test]
fn image_info_probe() {
    // Create a PPM and probe it
    let pixels = vec![255u8; 6]; // 1x2 RGB
    let encoded = EncodeRequest::pnm(pnm::PnmFormat::Ppm)
        .encode(&pixels, 1, 2, PixelLayout::Rgb8, Unstoppable)
        .unwrap();

    let info = ImageInfo::from_bytes(&encoded).unwrap();
    assert_eq!(info.width, 1);
    assert_eq!(info.height, 2);
    assert_eq!(info.format, BitmapFormat::Ppm);
    assert_eq!(info.native_layout, PixelLayout::Rgb8);
}

#[test]
fn limits_reject_large() {
    let pixels = vec![255u8; 6];
    let encoded = EncodeRequest::pnm(pnm::PnmFormat::Ppm)
        .encode(&pixels, 1, 2, PixelLayout::Rgb8, Unstoppable)
        .unwrap();

    let limits = Limits {
        max_pixels: Some(1), // only 1 pixel allowed
        ..Default::default()
    };

    let result = DecodeRequest::new(&encoded)
        .with_limits(&limits)
        .decode(Unstoppable);
    assert!(result.is_err());
    match result.unwrap_err() {
        PnmError::LimitExceeded(_) => {}
        other => panic!("expected LimitExceeded, got {other:?}"),
    }
}

#[test]
fn into_owned_works() {
    let pixels = vec![1u8, 2, 3];
    let encoded = EncodeRequest::pnm(pnm::PnmFormat::Pgm)
        .encode(&pixels, 1, 3, PixelLayout::Gray8, Unstoppable)
        .unwrap();

    let decoded = DecodeRequest::new(&encoded).decode(Unstoppable).unwrap();
    assert!(decoded.is_borrowed());

    let owned = decoded.into_owned();
    assert!(!owned.is_borrowed());
    assert_eq!(owned.pixels(), &[1, 2, 3]);
}
