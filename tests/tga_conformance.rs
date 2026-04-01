#![cfg(feature = "tga")]
//! Comprehensive TGA conformance tests.
//!
//! Tests all TGA image types, pixel depths, origins, RLE edge cases,
//! color maps, and error conditions using synthetic test data.

use enough::Unstoppable;
use zenbitmaps::*;

/// Helper: build a minimal TGA file from parts.
#[allow(clippy::too_many_arguments)]
fn build_tga(
    image_type: u8,
    width: u16,
    height: u16,
    pixel_depth: u8,
    descriptor: u8,
    color_map_type: u8,
    color_map_start: u16,
    color_map_length: u16,
    color_map_depth: u8,
    id: &[u8],
    color_map: &[u8],
    pixel_data: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(id.len() as u8);
    buf.push(color_map_type);
    buf.push(image_type);
    buf.extend_from_slice(&color_map_start.to_le_bytes());
    buf.extend_from_slice(&color_map_length.to_le_bytes());
    buf.push(color_map_depth);
    buf.extend_from_slice(&0u16.to_le_bytes()); // x_origin
    buf.extend_from_slice(&0u16.to_le_bytes()); // y_origin
    buf.extend_from_slice(&width.to_le_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    buf.push(pixel_depth);
    buf.push(descriptor);
    buf.extend_from_slice(id);
    buf.extend_from_slice(color_map);
    buf.extend_from_slice(pixel_data);
    buf
}

/// Helper: build a simple uncompressed TGA.
fn build_simple_tga(
    image_type: u8,
    width: u16,
    height: u16,
    pixel_depth: u8,
    descriptor: u8,
    pixel_data: &[u8],
) -> Vec<u8> {
    build_tga(
        image_type,
        width,
        height,
        pixel_depth,
        descriptor,
        0,
        0,
        0,
        0,
        &[],
        &[],
        pixel_data,
    )
}

// ══════════════════════════════════════════════════════════════════════
// Type 2: Uncompressed true color
// ══════════════════════════════════════════════════════════════════════

#[test]
fn type2_24bit_1x1() {
    // Single BGR pixel → RGB
    let data = build_simple_tga(2, 1, 1, 24, 0x20, &[0, 128, 255]); // BGR
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 1);
    assert_eq!(decoded.height, 1);
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &[255, 128, 0]); // RGB
}

#[test]
fn type2_24bit_2x2_bottom_origin() {
    // Bottom-left origin (descriptor bit5 = 0): rows stored bottom-to-top
    // Row 0 (bottom) = red, Row 1 (top) = blue
    let mut pixels = Vec::new();
    pixels.extend_from_slice(&[0, 0, 255, 0, 0, 255]); // bottom row: 2x red BGR
    pixels.extend_from_slice(&[255, 0, 0, 255, 0, 0]); // top row: 2x blue BGR
    let data = build_simple_tga(2, 2, 2, 24, 0, &pixels); // bit5=0 → bottom origin
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    // After flip: top row should be blue, bottom should be red
    let p = decoded.pixels();
    assert_eq!(&p[0..3], &[0, 0, 255]); // first output pixel: blue (was bottom→flipped to top)
    assert_eq!(&p[6..9], &[255, 0, 0]); // third pixel: red
}

#[test]
fn type2_24bit_2x2_top_origin() {
    // Top-left origin (descriptor bit5 = 1): rows stored top-to-bottom (no flip)
    let mut pixels = Vec::new();
    pixels.extend_from_slice(&[0, 0, 255, 0, 0, 255]); // top row: 2x red BGR
    pixels.extend_from_slice(&[255, 0, 0, 255, 0, 0]); // bottom row: 2x blue BGR
    let data = build_simple_tga(2, 2, 2, 24, 0x20, &pixels); // bit5=1 → top origin
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    let p = decoded.pixels();
    assert_eq!(&p[0..3], &[255, 0, 0]); // top row = red (as stored)
    assert_eq!(&p[6..9], &[0, 0, 255]); // bottom row = blue (as stored)
}

#[test]
fn type2_32bit_rgba() {
    // BGRA pixel with alpha
    let data = build_simple_tga(2, 1, 1, 32, 0x28, &[50, 100, 200, 128]); // BGRA, bit5=1, alpha_bits=8
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &[200, 100, 50, 128]); // RGBA
}

#[test]
fn type2_32bit_2x2() {
    let pixels: Vec<u8> = vec![
        10, 20, 30, 255, 40, 50, 60, 200, // row 0
        70, 80, 90, 128, 100, 110, 120, 64, // row 1
    ];
    let data = build_simple_tga(2, 2, 2, 32, 0x28, &pixels); // top origin + 8 alpha bits
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    // First pixel: BGRA(10,20,30,255) → RGBA(30,20,10,255)
    assert_eq!(&decoded.pixels()[0..4], &[30, 20, 10, 255]);
}

#[test]
fn type2_16bit_555() {
    // 16-bit 5-5-5 pixel: R=31, G=0, B=0 → all red
    // Bit layout: 0RRRRRGGGGGBBBBB, little-endian
    // R=31, G=0, B=0 → 0_11111_00000_00000 = 0x7C00
    let pixel = 0x7C00u16.to_le_bytes();
    let data = build_simple_tga(2, 1, 1, 16, 0x20, &pixel);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    // R should be 255 (31 * 255 / 31), G=0, B=0
    assert_eq!(decoded.pixels()[0], 255);
    assert_eq!(decoded.pixels()[1], 0);
    assert_eq!(decoded.pixels()[2], 0);
}

#[test]
fn type2_16bit_green() {
    // G=31, R=0, B=0 → 0_00000_11111_00000 = 0x03E0
    let pixel = 0x03E0u16.to_le_bytes();
    let data = build_simple_tga(2, 1, 1, 16, 0x20, &pixel);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels()[0], 0);
    assert_eq!(decoded.pixels()[1], 255);
    assert_eq!(decoded.pixels()[2], 0);
}

#[test]
fn type2_16bit_blue() {
    // B=31, R=0, G=0 → 0_00000_00000_11111 = 0x001F
    let pixel = 0x001Fu16.to_le_bytes();
    let data = build_simple_tga(2, 1, 1, 16, 0x20, &pixel);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels()[0], 0);
    assert_eq!(decoded.pixels()[1], 0);
    assert_eq!(decoded.pixels()[2], 255);
}

#[test]
fn type2_15bit() {
    // 15-bit is treated identically to 16-bit
    let pixel = 0x7C00u16.to_le_bytes();
    let data = build_simple_tga(2, 1, 1, 15, 0x20, &pixel);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels()[0], 255);
}

// ══════════════════════════════════════════════════════════════════════
// Type 3: Uncompressed grayscale
// ══════════════════════════════════════════════════════════════════════

#[test]
fn type3_gray_1x1() {
    let data = build_simple_tga(3, 1, 1, 8, 0x20, &[128]);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    assert_eq!(decoded.pixels(), &[128]);
}

#[test]
fn type3_gray_3x2() {
    let pixels = [0, 64, 128, 192, 255, 32];
    let data = build_simple_tga(3, 3, 2, 8, 0x20, &pixels);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.pixels(), &pixels);
}

#[test]
fn type3_gray_bottom_origin() {
    // bottom-left origin: row 0 (stored first) is bottom row
    let pixels = [10, 20, 30, 40]; // row0=bottom, row1=top
    let data = build_simple_tga(3, 2, 2, 8, 0, &pixels); // bit5=0
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    // After flip: output row 0 should be what was stored as row 1
    assert_eq!(decoded.pixels(), &[30, 40, 10, 20]);
}

// ══════════════════════════════════════════════════════════════════════
// Type 10: RLE true color
// ══════════════════════════════════════════════════════════════════════

#[test]
fn type10_rle_single_run() {
    // 2x1 image, one RLE packet: repeat 2 pixels of blue (BGR)
    let pixel_data = [0x81, 255, 0, 0]; // packet: run of 2, value = (255, 0, 0) BGR = blue
    let data = build_simple_tga(10, 2, 1, 24, 0x20, &pixel_data);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[0, 0, 255, 0, 0, 255]); // 2x blue RGB
}

#[test]
fn type10_rle_single_literal() {
    // 2x1 image, one raw packet: 2 distinct pixels
    let pixel_data = [0x01, 0, 0, 255, 0, 255, 0]; // raw 2: red BGR, green BGR
    let data = build_simple_tga(10, 2, 1, 24, 0x20, &pixel_data);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[255, 0, 0, 0, 255, 0]); // red RGB, green RGB
}

#[test]
fn type10_rle_mixed_packets() {
    // 4x1: run of 2 red + raw 2 (green, blue)
    let mut pixel_data = Vec::new();
    pixel_data.push(0x81); // run of 2
    pixel_data.extend_from_slice(&[0, 0, 255]); // red BGR
    pixel_data.push(0x01); // raw 2
    pixel_data.extend_from_slice(&[0, 255, 0]); // green BGR
    pixel_data.extend_from_slice(&[255, 0, 0]); // blue BGR
    let data = build_simple_tga(10, 4, 1, 24, 0x20, &pixel_data);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(
        decoded.pixels(),
        &[255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255]
    );
}

#[test]
fn type10_rle_32bit() {
    // RLE with alpha
    let pixel_data = [0x80, 50, 100, 200, 128]; // run of 1, BGRA
    let data = build_simple_tga(10, 1, 1, 32, 0x28, &pixel_data);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &[200, 100, 50, 128]); // RGBA
}

#[test]
fn type10_rle_max_run() {
    // Maximum run length: 128 pixels (packet header = 0xFF)
    let pixel_data = [0xFF, 0, 0, 255]; // run of 128, red BGR
    // 128x1 image
    let data = build_simple_tga(10, 128, 1, 24, 0x20, &pixel_data);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.width, 128);
    for chunk in decoded.pixels().chunks_exact(3) {
        assert_eq!(chunk, &[255, 0, 0]);
    }
}

// ══════════════════════════════════════════════════════════════════════
// Type 11: RLE grayscale
// ══════════════════════════════════════════════════════════════════════

#[test]
fn type11_rle_gray() {
    // 3x1, run of 3 gray(200)
    let pixel_data = [0x82, 200]; // run of 3
    let data = build_simple_tga(11, 3, 1, 8, 0x20, &pixel_data);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Gray8);
    assert_eq!(decoded.pixels(), &[200, 200, 200]);
}

#[test]
fn type11_rle_gray_mixed() {
    // 4x1: run of 2 (val=100) + literal of 2 (val=50, val=75)
    let pixel_data = [0x81, 100, 0x01, 50, 75];
    let data = build_simple_tga(11, 4, 1, 8, 0x20, &pixel_data);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[100, 100, 50, 75]);
}

// ══════════════════════════════════════════════════════════════════════
// Type 1: Uncompressed color-mapped
// ══════════════════════════════════════════════════════════════════════

#[test]
fn type1_colormapped_24bit() {
    // 2-entry palette (24-bit BGR), 2x1 image
    let color_map = [255, 0, 0, 0, 255, 0]; // entry 0: blue BGR, entry 1: green BGR
    let indices = [0, 1];
    let data = build_tga(1, 2, 1, 8, 0x20, 1, 0, 2, 24, &[], &color_map, &indices);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgb8);
    assert_eq!(decoded.pixels(), &[0, 0, 255, 0, 255, 0]); // blue RGB, green RGB
}

#[test]
fn type1_colormapped_32bit() {
    // 1-entry palette (32-bit BGRA), 1x1 image
    let color_map = [10, 20, 30, 200]; // BGRA
    let indices = [0];
    let data = build_tga(1, 1, 1, 8, 0x28, 1, 0, 1, 32, &[], &color_map, &indices);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.layout, PixelLayout::Rgba8);
    assert_eq!(decoded.pixels(), &[30, 20, 10, 200]); // RGBA
}

#[test]
fn type1_colormapped_16bit() {
    // 16-bit palette entries
    let r_pixel = 0x7C00u16; // R=31, G=0, B=0
    let g_pixel = 0x03E0u16; // G=31
    let color_map: Vec<u8> = r_pixel
        .to_le_bytes()
        .iter()
        .chain(g_pixel.to_le_bytes().iter())
        .copied()
        .collect();
    let indices = [0, 1];
    let data = build_tga(1, 2, 1, 8, 0x20, 1, 0, 2, 16, &[], &color_map, &indices);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(&decoded.pixels()[0..3], &[255, 0, 0]); // red
    assert_eq!(&decoded.pixels()[3..6], &[0, 255, 0]); // green
}

#[test]
fn type1_colormapped_with_offset() {
    // Color map starts at index 10 — indices reference 10, 11
    let color_map = [0, 0, 255, 255, 0, 0]; // entry 10: red BGR, entry 11: blue BGR
    let indices = [10, 11];
    let data = build_tga(1, 2, 1, 8, 0x20, 1, 10, 2, 24, &[], &color_map, &indices);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[255, 0, 0, 0, 0, 255]);
}

// ══════════════════════════════════════════════════════════════════════
// Type 9: RLE color-mapped
// ══════════════════════════════════════════════════════════════════════

#[test]
fn type9_rle_colormapped() {
    // 2-entry palette, 3x1 image: run of 3 (index 1)
    let color_map = [0, 0, 255, 0, 255, 0]; // entry 0: red BGR, entry 1: green BGR
    let pixel_data = [0x82, 1]; // run of 3, index 1
    let data = build_tga(9, 3, 1, 8, 0x20, 1, 0, 2, 24, &[], &color_map, &pixel_data);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[0, 255, 0, 0, 255, 0, 0, 255, 0]);
}

// ══════════════════════════════════════════════════════════════════════
// Image origin handling
// ══════════════════════════════════════════════════════════════════════

#[test]
fn right_to_left_origin() {
    // 3x1 image, right-to-left (descriptor bit4 = 1), top origin (bit5 = 1)
    let pixels = [0, 0, 255, 0, 255, 0, 255, 0, 0]; // BGR: blue, green, red
    let data = build_simple_tga(2, 3, 1, 24, 0x30, &pixels); // 0x30 = bit4 + bit5
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    // After right-to-left flip: red, green, blue
    assert_eq!(decoded.pixels(), &[0, 0, 255, 0, 255, 0, 255, 0, 0]);
}

#[test]
fn bottom_right_origin() {
    // 2x2, bottom-right (bit4=1, bit5=0): need both horizontal and vertical flip
    // Stored order (bottom-right origin): pixel (1,1), (0,1), (1,0), (0,0)
    let pixels = [
        10, 20, 30, 40, 50, 60, // bottom row, right-to-left
        70, 80, 90, 100, 110, 120, // top row, right-to-left
    ];
    let data = build_simple_tga(2, 2, 2, 24, 0x10, &pixels); // bit4=1, bit5=0
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    // After vertical flip: top row comes from data bottom row
    // After horizontal flip: pixels reversed within each row
    // Expected: top-left origin reading order
    let p = decoded.pixels();
    // Top row (originally bottom row [70,80,90, 100,110,120]), flipped horizontally
    assert_eq!(&p[0..3], &[120, 110, 100]); // was rightmost pixel
    assert_eq!(&p[3..6], &[90, 80, 70]); // was leftmost pixel
}

// ══════════════════════════════════════════════════════════════════════
// Image ID field
// ══════════════════════════════════════════════════════════════════════

#[test]
fn with_image_id() {
    // 32-byte image ID field should be skipped
    let id = b"This is a TGA image identifier!!"; // 32 bytes
    let pixels = [0, 255, 0]; // green BGR
    let data = build_tga(2, 1, 1, 24, 0x20, 0, 0, 0, 0, id, &[], &pixels);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[0, 255, 0]); // green RGB
}

#[test]
fn with_max_image_id() {
    // Maximum image ID: 255 bytes
    let id = vec![b'X'; 255];
    let pixels = [128];
    let data = build_tga(3, 1, 1, 8, 0x20, 0, 0, 0, 0, &id, &[], &pixels);
    let decoded = decode_tga(&data, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &[128]);
}

// ══════════════════════════════════════════════════════════════════════
// Encode→Decode roundtrip for all layouts
// ══════════════════════════════════════════════════════════════════════

#[test]
fn roundtrip_rgb8_10x10() {
    let pixels: Vec<u8> = (0..10 * 10)
        .flat_map(|i| {
            [
                (i * 7 % 256) as u8,
                (i * 13 % 256) as u8,
                (i * 19 % 256) as u8,
            ]
        })
        .collect();
    let encoded = encode_tga(&pixels, 10, 10, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[test]
fn roundtrip_rgba8_large() {
    let pixels: Vec<u8> = (0..50 * 30)
        .flat_map(|i| {
            [
                (i % 256) as u8,
                ((i * 3) % 256) as u8,
                ((i * 7) % 256) as u8,
                255,
            ]
        })
        .collect();
    let encoded = encode_tga(&pixels, 50, 30, PixelLayout::Rgba8, Unstoppable).unwrap();
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

#[test]
fn roundtrip_gray8_wide() {
    let pixels: Vec<u8> = (0..200).map(|i| (i % 256) as u8).collect();
    let encoded = encode_tga(&pixels, 200, 1, PixelLayout::Gray8, Unstoppable).unwrap();
    let decoded = decode_tga(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.pixels(), &pixels[..]);
}

// ══════════════════════════════════════════════════════════════════════
// Error conditions
// ══════════════════════════════════════════════════════════════════════

#[test]
fn invalid_image_type_0() {
    let data = build_simple_tga(0, 1, 1, 24, 0x20, &[0, 0, 0]);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn invalid_image_type_4() {
    let data = build_simple_tga(4, 1, 1, 24, 0x20, &[0, 0, 0]);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn invalid_color_map_type_2() {
    let mut data = build_simple_tga(2, 1, 1, 24, 0x20, &[0, 0, 0]);
    data[1] = 2; // invalid color_map_type
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn colormapped_without_colormap() {
    // Type 1 with color_map_type=0
    let mut data = build_simple_tga(2, 1, 1, 24, 0x20, &[0, 0, 0]);
    data[2] = 1; // change to color-mapped
    data[1] = 0; // no color map
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn zero_width() {
    let data = build_simple_tga(2, 0, 1, 24, 0x20, &[]);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn zero_height() {
    let data = build_simple_tga(2, 1, 0, 24, 0x20, &[]);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn invalid_pixel_depth() {
    let data = build_simple_tga(2, 1, 1, 12, 0x20, &[0, 0, 0]);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn truncated_pixel_data() {
    // 2x2 24-bit needs 12 bytes of pixel data, provide only 6
    let data = build_simple_tga(2, 2, 2, 24, 0x20, &[0; 6]);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn rle_packet_exceeds_bounds() {
    // 1x1 image but RLE packet says run of 5
    let pixel_data = [0x84, 0, 0, 255]; // run of 5, but image is only 1 pixel
    let data = build_simple_tga(10, 1, 1, 24, 0x20, &pixel_data);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn rle_truncated_run_value() {
    // RLE header says run, but no pixel value follows
    let pixel_data = [0x80]; // run of 1, but no pixel data
    let data = build_simple_tga(10, 1, 1, 24, 0x20, &pixel_data);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

#[test]
fn palette_index_out_of_range() {
    // Palette has 2 entries, but pixel references index 5
    let color_map = [0, 0, 255, 255, 0, 0]; // 2 entries
    let indices = [5]; // out of range
    let data = build_tga(1, 1, 1, 8, 0x20, 1, 0, 2, 24, &[], &color_map, &indices);
    assert!(decode_tga(&data, Unstoppable).is_err());
}

// ══════════════════════════════════════════════════════════════════════
// Limits
// ══════════════════════════════════════════════════════════════════════

#[test]
fn limits_max_width_tga() {
    let pixels = vec![0u8; 100 * 3];
    let encoded = encode_tga(&pixels, 100, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_width: Some(50),
        ..Default::default()
    };
    assert!(decode_tga_with_limits(&encoded, &limits, Unstoppable).is_err());
}

#[test]
fn limits_max_memory_tga() {
    let pixels = vec![0u8; 10 * 10 * 3];
    let encoded = encode_tga(&pixels, 10, 10, PixelLayout::Rgb8, Unstoppable).unwrap();
    let limits = Limits {
        max_memory_bytes: Some(10),
        ..Default::default()
    };
    assert!(decode_tga_with_limits(&encoded, &limits, Unstoppable).is_err());
}

// ══════════════════════════════════════════════════════════════════════
// Cancellation
// ══════════════════════════════════════════════════════════════════════

#[test]
fn cancellation_decode_tga() {
    struct AlreadyStopped;
    impl enough::Stop for AlreadyStopped {
        fn check(&self) -> Result<(), enough::StopReason> {
            Err(enough::StopReason::Cancelled)
        }
    }

    let pixels: Vec<u8> = (0..10 * 32).flat_map(|i| [(i % 256) as u8, 0, 0]).collect();
    let encoded = encode_tga(&pixels, 10, 32, PixelLayout::Rgb8, Unstoppable).unwrap();
    assert!(matches!(
        decode_tga(&encoded, AlreadyStopped),
        Err(BitmapError::Cancelled(_))
    ));
}

#[test]
fn cancellation_encode_tga() {
    struct AlreadyStopped;
    impl enough::Stop for AlreadyStopped {
        fn check(&self) -> Result<(), enough::StopReason> {
            Err(enough::StopReason::Cancelled)
        }
    }

    let pixels = vec![0u8; 10 * 32 * 3];
    assert!(matches!(
        encode_tga(&pixels, 10, 32, PixelLayout::Rgb8, AlreadyStopped),
        Err(BitmapError::Cancelled(_))
    ));
}
