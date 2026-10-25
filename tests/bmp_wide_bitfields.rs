//! BI_BITFIELDS masks wider than 8 bits (10:10:10:2, 11:11:10) used to be
//! narrowed to 8 bits per channel, dropping the low bits. They now decode to
//! Rgba16 by bit replication, so every code shifts back out exactly.
#![cfg(feature = "bmp")]

use zenbitmaps::{PixelLayout, decode_bmp};

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}
fn le16(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

/// 32-bpp BI_BITFIELDS BMP, bottom-up. `v4` writes a 108-byte BITMAPV4HEADER
/// carrying all four masks; otherwise a 40-byte header followed by the three
/// colour masks (no alpha mask).
fn build_32bpp(
    width: u32,
    height: u32,
    masks: [u32; 4],
    v4: bool,
    rows_bottom_up: &[u32],
) -> Vec<u8> {
    assert_eq!(rows_bottom_up.len(), (width * height) as usize);
    let ihsize: u32 = if v4 { 108 } else { 40 };
    let offset = 14 + ihsize + if v4 { 0 } else { 12 };
    let image_size = width * height * 4;
    let mut b = Vec::new();
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&le32(offset + image_size));
    b.extend_from_slice(&le32(0));
    b.extend_from_slice(&le32(offset));
    b.extend_from_slice(&le32(ihsize));
    b.extend_from_slice(&le32(width));
    b.extend_from_slice(&le32(height));
    b.extend_from_slice(&le16(1));
    b.extend_from_slice(&le16(32));
    b.extend_from_slice(&le32(3)); // BI_BITFIELDS
    b.extend_from_slice(&le32(image_size));
    b.extend_from_slice(&le32(2835));
    b.extend_from_slice(&le32(2835));
    b.extend_from_slice(&le32(0));
    b.extend_from_slice(&le32(0));
    for m in &masks[..3] {
        b.extend_from_slice(&le32(*m));
    }
    if v4 {
        b.extend_from_slice(&le32(masks[3]));
        b.extend_from_slice(&le32(0x7352_4742)); // 'sRGB'
        b.extend_from_slice(&[0u8; 36 + 12]); // endpoints + gamma
    }
    assert_eq!(b.len() as u32, offset);
    for v in rows_bottom_up {
        b.extend_from_slice(&le32(*v));
    }
    b
}

/// Repeat an `n`-bit code until it fills 16 bits.
fn replicate16(code: u32, n: u32) -> u16 {
    let mut out = 0u32;
    let mut shift = 16i32 - n as i32;
    while shift > -(n as i32) {
        out |= if shift >= 0 {
            code << shift
        } else {
            code >> -shift
        };
        shift -= n as i32;
    }
    (out & 0xffff) as u16
}

fn rgba16(px: &[u8]) -> Vec<[u16; 4]> {
    px.as_chunks::<8>()
        .0
        .iter()
        .map(|c| {
            let w = |i: usize| u16::from_ne_bytes([c[2 * i], c[2 * i + 1]]);
            [w(0), w(1), w(2), w(3)]
        })
        .collect()
}

#[test]
fn bitfields_10_10_10_2_decode_to_rgba16_exactly() {
    // A2R10G10B10: alpha in the top two bits.
    let masks = [0x3ff0_0000, 0x000f_fc00, 0x0000_03ff, 0xc000_0000];
    let codes: [[u32; 4]; 8] = [
        [0, 0, 0, 0],
        [1023, 1023, 1023, 3],
        [1, 2, 3, 1],
        [512, 1000, 7, 2],
        [1022, 0, 1023, 0],
        [341, 682, 85, 3],
        [64, 65, 940, 1],
        [1000, 999, 998, 2],
    ];
    let pack = |c: [u32; 4]| (c[3] << 30) | (c[0] << 20) | (c[1] << 10) | c[2];
    // Width 4, height 2. File rows are bottom-up: codes[4..8] is the top row.
    let file: Vec<u32> = codes[0..4]
        .iter()
        .chain(&codes[4..8])
        .map(|&c| pack(c))
        .collect();
    let bmp = build_32bpp(4, 2, masks, true, &file);

    let out = decode_bmp(&bmp, enough::Unstoppable).expect("decode");
    assert_eq!(out.layout, PixelLayout::Rgba16);
    let px = rgba16(out.pixels());
    let top_down: Vec<[u32; 4]> = codes[4..8].iter().chain(&codes[0..4]).copied().collect();
    for (got, c) in px.iter().zip(&top_down) {
        let want = [
            replicate16(c[0], 10),
            replicate16(c[1], 10),
            replicate16(c[2], 10),
            replicate16(c[3], 2),
        ];
        assert_eq!(*got, want, "codes {c:?}");
        // The original codes shift back out unchanged.
        assert_eq!(
            [got[0] >> 6, got[1] >> 6, got[2] >> 6, got[3] >> 14].map(u32::from),
            *c
        );
    }
    assert_eq!(
        px[5], [65535; 4],
        "10-bit white and 2-bit opaque are full scale"
    );
}

#[test]
fn bitfields_11_11_10_without_alpha_mask_are_opaque_rgba16() {
    // R11 G11 B10 (bmpsuite's rgb32-111110 layout), masks after a 40-byte header.
    let masks = [0xffe0_0000, 0x001f_fc00, 0x0000_03ff, 0];
    let codes: [[u32; 3]; 2] = [[2047, 1024, 1023], [1, 2046, 0]];
    let file: Vec<u32> = codes
        .iter()
        .map(|c| (c[0] << 21) | (c[1] << 10) | c[2])
        .collect();
    let bmp = build_32bpp(2, 1, masks, false, &file);

    let out = decode_bmp(&bmp, enough::Unstoppable).expect("decode");
    assert_eq!(out.layout, PixelLayout::Rgba16);
    let px = rgba16(out.pixels());
    for (got, c) in px.iter().zip(&codes) {
        assert_eq!(
            *got,
            [
                replicate16(c[0], 11),
                replicate16(c[1], 11),
                replicate16(c[2], 10),
                65535
            ]
        );
        assert_eq!([got[0] >> 5, got[1] >> 5, got[2] >> 6].map(u32::from), *c);
    }
}

#[test]
fn bitfields_8_8_8_8_stay_rgba8() {
    let masks = [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000];
    let bmp = build_32bpp(1, 1, masks, true, &[0x80_11_22_33]);
    let out = decode_bmp(&bmp, enough::Unstoppable).expect("decode");
    assert_eq!(out.layout, PixelLayout::Rgba8);
    assert_eq!(out.pixels(), &[0x11, 0x22, 0x33, 0x80]);
}

#[test]
fn bitfield_wider_than_16_bits_is_refused() {
    // A 17-bit red mask cannot be held in Rgba16 without dropping a bit.
    let masks = [0x0001_ffff, 0x00fe_0000, 0x7f00_0000, 0];
    let bmp = build_32bpp(1, 1, masks, false, &[0]);
    assert!(decode_bmp(&bmp, enough::Unstoppable).is_err());
}
