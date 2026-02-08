#![cfg(feature = "imgref")]

use enough::Unstoppable;
use zenpnm::*;

#[test]
fn ppm_pixels_roundtrip() {
    let pixels = vec![
        RGB8::new(255, 0, 0),
        RGB8::new(0, 255, 0),
        RGB8::new(0, 0, 255),
        RGB8::new(128, 128, 128),
    ];
    let encoded = encode_ppm_pixels(&pixels, 2, 2, Unstoppable).unwrap();
    let (decoded, w, h) = decode_pixels::<RGB8>(&encoded, Unstoppable).unwrap();
    assert_eq!((w, h), (2, 2));
    assert_eq!(decoded, pixels);
}

#[test]
fn pam_rgba_pixels_roundtrip() {
    let pixels = vec![RGBA8::new(255, 0, 0, 255), RGBA8::new(0, 255, 0, 128)];
    let encoded = encode_pam_pixels(&pixels, 2, 1, Unstoppable).unwrap();
    let (decoded, w, h) = decode_pixels::<RGBA8>(&encoded, Unstoppable).unwrap();
    assert_eq!((w, h), (2, 1));
    assert_eq!(decoded, pixels);
}

#[test]
fn imgvec_roundtrip() {
    let img = imgref::ImgVec::new(
        vec![
            RGB8::new(10, 20, 30),
            RGB8::new(40, 50, 60),
            RGB8::new(70, 80, 90),
            RGB8::new(100, 110, 120),
        ],
        2,
        2,
    );
    let encoded = encode_ppm_img(img.as_ref(), Unstoppable).unwrap();
    let decoded = decode_img::<RGB8>(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width(), 2);
    assert_eq!(decoded.height(), 2);
    assert_eq!(decoded.buf(), img.buf());
}

#[test]
fn decode_into_matching_dims() {
    let pixels = vec![RGB8::new(255, 0, 0); 6];
    let encoded = encode_ppm_pixels(&pixels, 3, 2, Unstoppable).unwrap();

    let mut output = imgref::ImgVec::new(vec![RGB8::new(0, 0, 0); 6], 3, 2);
    decode_into(&encoded, output.as_mut(), Unstoppable).unwrap();
    assert_eq!(output.buf(), &pixels);
}

#[test]
fn decode_into_dimension_mismatch() {
    let pixels = vec![RGB8::new(255, 0, 0); 4];
    let encoded = encode_ppm_pixels(&pixels, 2, 2, Unstoppable).unwrap();

    let mut output = imgref::ImgVec::new(vec![RGB8::new(0, 0, 0); 6], 3, 2);
    assert!(decode_into(&encoded, output.as_mut(), Unstoppable).is_err());
}

#[test]
fn decode_into_layout_mismatch() {
    let pixels = vec![RGB8::new(255, 0, 0); 4];
    let encoded = encode_ppm_pixels(&pixels, 2, 2, Unstoppable).unwrap();

    let mut output = imgref::ImgVec::new(vec![RGBA8::new(0, 0, 0, 0); 4], 2, 2);
    assert!(decode_into(&encoded, output.as_mut(), Unstoppable).is_err());
}

#[test]
fn strided_imgref_encode() {
    // Create a 4x3 image and take a 2x2 sub_image (which will have stride=4)
    let buf = vec![
        RGB8::new(1, 2, 3),
        RGB8::new(4, 5, 6),
        RGB8::new(0, 0, 0),
        RGB8::new(0, 0, 0),
        RGB8::new(7, 8, 9),
        RGB8::new(10, 11, 12),
        RGB8::new(0, 0, 0),
        RGB8::new(0, 0, 0),
        RGB8::new(0, 0, 0),
        RGB8::new(0, 0, 0),
        RGB8::new(0, 0, 0),
        RGB8::new(0, 0, 0),
    ];
    let full = imgref::ImgVec::new(buf, 4, 3);
    let sub = full.as_ref().sub_image(0, 0, 2, 2);

    let encoded = encode_ppm_img(sub, Unstoppable).unwrap();
    let (decoded, w, h) = decode_pixels::<RGB8>(&encoded, Unstoppable).unwrap();
    assert_eq!((w, h), (2, 2));
    assert_eq!(
        decoded,
        vec![
            RGB8::new(1, 2, 3),
            RGB8::new(4, 5, 6),
            RGB8::new(7, 8, 9),
            RGB8::new(10, 11, 12),
        ]
    );
}

#[test]
fn strided_decode_into() {
    let pixels = vec![
        RGB8::new(1, 1, 1),
        RGB8::new(2, 2, 2),
        RGB8::new(3, 3, 3),
        RGB8::new(4, 4, 4),
    ];
    let encoded = encode_ppm_pixels(&pixels, 2, 2, Unstoppable).unwrap();

    // Create a 4x3 buffer and decode into a 2x2 sub_image
    let buf = vec![RGB8::new(0, 0, 0); 12];
    let mut full = imgref::ImgVec::new(buf, 4, 3);
    let mut as_mut = full.as_mut();
    let sub = as_mut.sub_image_mut(0, 0, 2, 2);
    decode_into(&encoded, sub, Unstoppable).unwrap();
    drop(as_mut);
    let buf = full.into_buf();

    // Check that the 2x2 region was filled (stride=4)
    assert_eq!(buf[0], RGB8::new(1, 1, 1));
    assert_eq!(buf[1], RGB8::new(2, 2, 2));
    assert_eq!(buf[2], RGB8::new(0, 0, 0)); // padding
    assert_eq!(buf[4], RGB8::new(3, 3, 3));
    assert_eq!(buf[5], RGB8::new(4, 4, 4));
    assert_eq!(buf[6], RGB8::new(0, 0, 0)); // padding
}

#[test]
fn decode_output_as_pixels() {
    let pixels = vec![RGB8::new(100, 200, 50); 4];
    let encoded = encode_ppm_pixels(&pixels, 2, 2, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    let typed: &[RGB8] = decoded.as_pixels().unwrap();
    assert_eq!(typed.len(), 4);
    assert_eq!(typed[0], RGB8::new(100, 200, 50));
}

#[test]
fn decode_output_to_imgvec() {
    let pixels = vec![RGB8::new(1, 2, 3), RGB8::new(4, 5, 6)];
    let encoded = encode_ppm_pixels(&pixels, 2, 1, Unstoppable).unwrap();
    let decoded = decode(&encoded, Unstoppable).unwrap();
    let img = decoded.to_imgvec::<RGB8>().unwrap();
    assert_eq!(img.width(), 2);
    assert_eq!(img.height(), 1);
    assert_eq!(img.buf(), &pixels);
}
