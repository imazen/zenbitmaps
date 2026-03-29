//! Comprehensive Radiance HDR conformance tests.
//!
//! Tests header parsing, RGBE conversion precision, RLE edge cases,
//! flat (uncompressed) path, roundtrip fidelity, and error conditions.

use enough::Unstoppable;
use zenbitmaps::*;

/// Build an HDR file with the given pixel data (RGBE quads or RLE-encoded scanlines).
fn build_hdr(width: u32, height: u32, pixel_data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"#?RADIANCE\n");
    buf.extend_from_slice(b"FORMAT=32-bit_rle_rgbe\n");
    buf.extend_from_slice(b"\n");
    let res = alloc::format!("-Y {height} +X {width}\n");
    buf.extend_from_slice(res.as_bytes());
    buf.extend_from_slice(pixel_data);
    buf
}

extern crate alloc;

/// Build an HDR file with flat (uncompressed) RGBE data from f32 triples.
fn build_hdr_from_f32(width: u32, height: u32, pixels: &[(f32, f32, f32)]) -> Vec<u8> {
    let rgbe_data: Vec<u8> = pixels
        .iter()
        .flat_map(|&(r, g, b)| f32_to_rgbe(r, g, b))
        .collect();
    build_hdr(width, height, &rgbe_data)
}

/// Convert f32 RGB to RGBE (mirrors the encoder's logic).
fn f32_to_rgbe(r: f32, g: f32, b: f32) -> [u8; 4] {
    let max = r.max(g).max(b);
    if max < 1e-32 {
        return [0, 0, 0, 0];
    }
    let bits = max.to_bits();
    let biased_exp = ((bits >> 23) & 0xFF) as i32;
    let raw_exp = biased_exp - 126;
    let scale_exp = 135i32 - raw_exp;
    if !(1..=254).contains(&scale_exp) {
        if scale_exp < 1 {
            return [255, 255, 255, 255];
        }
        return [0, 0, 0, 0];
    }
    let scale = f32::from_bits((scale_exp as u32) << 23);
    [(r * scale) as u8, (g * scale) as u8, (b * scale) as u8, (raw_exp + 128) as u8]
}

/// Assert two f32 values are within relative tolerance.
fn assert_f32_close(actual: f32, expected: f32, tol: f32, label: &str) {
    if expected.abs() < 1e-6 {
        assert!(
            actual.abs() < tol,
            "{label}: expected ~0, got {actual}"
        );
    } else {
        let rel = (actual - expected).abs() / expected.abs();
        assert!(
            rel < tol,
            "{label}: expected {expected}, got {actual} (rel err {rel})"
        );
    }
}

/// Extract f32 from decoded RgbF32 bytes at pixel index.
fn pixel_f32(data: &[u8], idx: usize) -> (f32, f32, f32) {
    let base = idx * 12;
    let r = f32::from_le_bytes([data[base], data[base + 1], data[base + 2], data[base + 3]]);
    let g = f32::from_le_bytes([data[base + 4], data[base + 5], data[base + 6], data[base + 7]]);
    let b = f32::from_le_bytes([
        data[base + 8],
        data[base + 9],
        data[base + 10],
        data[base + 11],
    ]);
    (r, g, b)
}

// ══════════════════════════════════════════════════════════════════════
// RGBE precision tests
// ══════════════════════════════════════════════════════════════════════

#[test]
fn rgbe_precision_unit_values() {
    // Test common values in [0, 1] range
    let cases: &[(f32, f32, f32)] = &[
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 0.0, 1.0),
        (1.0, 1.0, 1.0),
        (0.5, 0.5, 0.5),
        (0.25, 0.25, 0.25),
    ];
    for &(r, g, b) in cases {
        let hdr = build_hdr_from_f32(1, 1, &[(r, g, b)]);
        let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
        let (dr, dg, db) = pixel_f32(decoded.pixels(), 0);
        assert_f32_close(dr, r, 0.02, &alloc::format!("R of ({r},{g},{b})"));
        assert_f32_close(dg, g, 0.02, &alloc::format!("G of ({r},{g},{b})"));
        assert_f32_close(db, b, 0.02, &alloc::format!("B of ({r},{g},{b})"));
    }
}

#[test]
fn rgbe_precision_hdr_values() {
    // High dynamic range values (>1.0)
    let cases: &[(f32, f32, f32)] = &[
        (2.0, 1.0, 0.5),
        (10.0, 10.0, 10.0),
        (100.0, 50.0, 25.0),
        (1000.0, 500.0, 250.0),
    ];
    for &(r, g, b) in cases {
        let hdr = build_hdr_from_f32(1, 1, &[(r, g, b)]);
        let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
        let (dr, dg, db) = pixel_f32(decoded.pixels(), 0);
        assert_f32_close(dr, r, 0.02, &alloc::format!("R of ({r},{g},{b})"));
        assert_f32_close(dg, g, 0.02, &alloc::format!("G of ({r},{g},{b})"));
        assert_f32_close(db, b, 0.02, &alloc::format!("B of ({r},{g},{b})"));
    }
}

#[test]
fn rgbe_precision_low_values() {
    // Very small values
    let cases: &[(f32, f32, f32)] = &[
        (0.01, 0.01, 0.01),
        (0.001, 0.001, 0.001),
        (0.1, 0.05, 0.025),
    ];
    for &(r, g, b) in cases {
        let hdr = build_hdr_from_f32(1, 1, &[(r, g, b)]);
        let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
        let (dr, dg, db) = pixel_f32(decoded.pixels(), 0);
        // Lower precision for small values (RGBE has ~1% relative error)
        assert_f32_close(dr, r, 0.05, &alloc::format!("R of ({r},{g},{b})"));
        assert_f32_close(dg, g, 0.05, &alloc::format!("G of ({r},{g},{b})"));
        assert_f32_close(db, b, 0.05, &alloc::format!("B of ({r},{g},{b})"));
    }
}

#[test]
fn rgbe_black_pixel() {
    let hdr = build_hdr_from_f32(1, 1, &[(0.0, 0.0, 0.0)]);
    let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
    let (r, g, b) = pixel_f32(decoded.pixels(), 0);
    assert_eq!(r, 0.0);
    assert_eq!(g, 0.0);
    assert_eq!(b, 0.0);
}

// ══════════════════════════════════════════════════════════════════════
// Header parsing
// ══════════════════════════════════════════════════════════════════════

#[test]
fn header_radiance_magic() {
    let hdr = build_hdr_from_f32(1, 1, &[(1.0, 1.0, 1.0)]);
    assert!(decode_hdr(&hdr, Unstoppable).is_ok());
}

#[test]
fn header_rgbe_magic() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"#?RGBE\n");
    buf.extend_from_slice(b"FORMAT=32-bit_rle_rgbe\n");
    buf.extend_from_slice(b"\n");
    buf.extend_from_slice(b"-Y 1 +X 1\n");
    buf.extend_from_slice(&f32_to_rgbe(0.5, 0.5, 0.5));
    let decoded = decode_hdr(&buf, Unstoppable).unwrap();
    assert_eq!(decoded.width, 1);
    assert_eq!(decoded.height, 1);
}

#[test]
fn header_with_extra_fields() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"#?RADIANCE\n");
    buf.extend_from_slice(b"FORMAT=32-bit_rle_rgbe\n");
    buf.extend_from_slice(b"EXPOSURE=1.0\n");
    buf.extend_from_slice(b"SOFTWARE=test\n");
    buf.extend_from_slice(b"# This is a comment line\n");
    buf.extend_from_slice(b"PRIMARIES=0.640 0.330 0.300 0.600 0.150 0.060 0.313 0.329\n");
    buf.extend_from_slice(b"\n");
    buf.extend_from_slice(b"-Y 1 +X 2\n");
    buf.extend_from_slice(&f32_to_rgbe(1.0, 0.0, 0.0));
    buf.extend_from_slice(&f32_to_rgbe(0.0, 1.0, 0.0));
    let decoded = decode_hdr(&buf, Unstoppable).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 1);
}

#[test]
fn header_wrong_magic() {
    let buf = b"#?UNKNOWN\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 1\n\x00\x00\x00\x00";
    assert!(decode_hdr(buf, Unstoppable).is_err());
}

#[test]
fn header_missing_resolution() {
    let buf = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n";
    assert!(decode_hdr(buf, Unstoppable).is_err());
}

#[test]
fn header_bad_resolution_format() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"#?RADIANCE\n\n");
    buf.extend_from_slice(b"+Y 1 +X 1\n"); // unsupported orientation
    buf.extend_from_slice(&[0; 4]);
    assert!(decode_hdr(&buf, Unstoppable).is_err());
}

#[test]
fn header_non_numeric_dimensions() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"#?RADIANCE\n\n");
    buf.extend_from_slice(b"-Y abc +X 10\n");
    assert!(decode_hdr(&buf, Unstoppable).is_err());
}

// ══════════════════════════════════════════════════════════════════════
// Flat (uncompressed) path — width < 8
// ══════════════════════════════════════════════════════════════════════

#[test]
fn flat_1x1() {
    let hdr = build_hdr_from_f32(1, 1, &[(0.75, 0.5, 0.25)]);
    let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
    let (r, g, b) = pixel_f32(decoded.pixels(), 0);
    assert_f32_close(r, 0.75, 0.02, "R");
    assert_f32_close(g, 0.5, 0.02, "G");
    assert_f32_close(b, 0.25, 0.02, "B");
}

#[test]
fn flat_3x2() {
    // 3 pixels wide → uses flat path (width < 8)
    let pixels = [
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 0.0, 1.0),
        (0.5, 0.5, 0.5),
        (2.0, 2.0, 2.0),
        (0.0, 0.0, 0.0),
    ];
    let hdr = build_hdr_from_f32(3, 2, &pixels);
    let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    for (i, &(er, eg, eb)) in pixels.iter().enumerate() {
        let (r, g, b) = pixel_f32(decoded.pixels(), i);
        assert_f32_close(r, er, 0.02, &alloc::format!("px{i} R"));
        assert_f32_close(g, eg, 0.02, &alloc::format!("px{i} G"));
        assert_f32_close(b, eb, 0.02, &alloc::format!("px{i} B"));
    }
}

#[test]
fn flat_7x1() {
    // 7 pixels → flat path (< 8)
    let pixels: Vec<(f32, f32, f32)> = (0..7).map(|i| (i as f32 * 0.1, 0.5, 1.0)).collect();
    let hdr = build_hdr_from_f32(7, 1, &pixels);
    let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
    assert_eq!(decoded.width, 7);
}

// ══════════════════════════════════════════════════════════════════════
// New-style RLE path — encode/decode roundtrip
// ══════════════════════════════════════════════════════════════════════

#[test]
fn rle_roundtrip_8x1() {
    // 8 pixels wide → triggers RLE path
    let pixels: Vec<u8> = (0..8)
        .flat_map(|i| {
            let v = (i as f32) * 0.1 + 0.1;
            v.to_le_bytes()
                .iter()
                .chain(v.to_le_bytes().iter())
                .chain((v * 0.5).to_le_bytes().iter())
                .copied()
                .collect::<Vec<u8>>()
        })
        .collect();
    let encoded = encode_hdr(&pixels, 8, 1, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 8);
    // Verify each pixel is close
    for i in 0..8 {
        let (r, _g, _b) = pixel_f32(decoded.pixels(), i);
        let expected = (i as f32) * 0.1 + 0.1;
        assert_f32_close(r, expected, 0.03, &alloc::format!("px{i}"));
    }
}

#[test]
fn rle_roundtrip_uniform_scanline() {
    // All pixels identical → should compress to runs
    let v = 1.0f32;
    let pixels: Vec<u8> = (0..16)
        .flat_map(|_| {
            let mut p = Vec::new();
            p.extend_from_slice(&v.to_le_bytes());
            p.extend_from_slice(&v.to_le_bytes());
            p.extend_from_slice(&v.to_le_bytes());
            p
        })
        .collect();
    let encoded = encode_hdr(&pixels, 16, 1, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    for i in 0..16 {
        let (r, g, b) = pixel_f32(decoded.pixels(), i);
        assert_f32_close(r, 1.0, 0.02, &alloc::format!("px{i} R"));
        assert_f32_close(g, 1.0, 0.02, &alloc::format!("px{i} G"));
        assert_f32_close(b, 1.0, 0.02, &alloc::format!("px{i} B"));
    }
}

#[test]
fn rle_roundtrip_varied_scanline() {
    // All distinct values → should encode as literals
    let pixels: Vec<u8> = (0..10)
        .flat_map(|i| {
            let r = (i as f32 + 1.0) * 0.1;
            let g = (i as f32 + 1.0) * 0.2;
            let b = (i as f32 + 1.0) * 0.05;
            let mut p = Vec::new();
            p.extend_from_slice(&r.to_le_bytes());
            p.extend_from_slice(&g.to_le_bytes());
            p.extend_from_slice(&b.to_le_bytes());
            p
        })
        .collect();
    let encoded = encode_hdr(&pixels, 10, 1, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    for i in 0..10 {
        let (r, g, b) = pixel_f32(decoded.pixels(), i);
        let er = (i as f32 + 1.0) * 0.1;
        let eg = (i as f32 + 1.0) * 0.2;
        let eb = (i as f32 + 1.0) * 0.05;
        assert_f32_close(r, er, 0.03, &alloc::format!("px{i} R"));
        assert_f32_close(g, eg, 0.03, &alloc::format!("px{i} G"));
        assert_f32_close(b, eb, 0.03, &alloc::format!("px{i} B"));
    }
}

#[test]
fn rle_roundtrip_multi_row() {
    // Multi-row: 10x5
    let pixels: Vec<u8> = (0..50)
        .flat_map(|i| {
            let v = (i as f32) * 0.02 + 0.1;
            let mut p = Vec::new();
            p.extend_from_slice(&v.to_le_bytes());
            p.extend_from_slice(&(v * 0.5).to_le_bytes());
            p.extend_from_slice(&(v * 0.25).to_le_bytes());
            p
        })
        .collect();
    let encoded = encode_hdr(&pixels, 10, 5, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 10);
    assert_eq!(decoded.height, 5);
    // Spot check a few pixels
    let (r, _, _) = pixel_f32(decoded.pixels(), 0);
    assert_f32_close(r, 0.1, 0.03, "first pixel");
    let (r, _, _) = pixel_f32(decoded.pixels(), 49);
    let expected = 49.0 * 0.02 + 0.1;
    assert_f32_close(r, expected, 0.03, "last pixel");
}

#[test]
fn rle_roundtrip_large() {
    // 100x50 — exercises multi-row RLE and cancellation boundary (row%16)
    let pixels: Vec<u8> = (0..5000)
        .flat_map(|i| {
            let v = ((i % 256) as f32) / 255.0;
            let mut p = Vec::new();
            p.extend_from_slice(&v.to_le_bytes());
            p.extend_from_slice(&v.to_le_bytes());
            p.extend_from_slice(&v.to_le_bytes());
            p
        })
        .collect();
    let encoded = encode_hdr(&pixels, 100, 50, PixelLayout::RgbF32, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    assert_eq!(decoded.width, 100);
    assert_eq!(decoded.height, 50);
    // Verify the encoded file starts with the right header
    assert!(encoded.starts_with(b"#?RADIANCE\n"));
}

// ══════════════════════════════════════════════════════════════════════
// RLE edge cases (hand-crafted encoded data)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn rle_all_runs() {
    // 8x1, all same RGBE value → single run per channel
    let rgbe = f32_to_rgbe(1.0, 0.5, 0.25);
    let mut pixel_data = Vec::new();
    // RLE marker
    pixel_data.extend_from_slice(&[2, 2, 0, 8]); // width=8
    // Each channel: run of 8 identical values
    for ch in 0..4 {
        pixel_data.push(128 + 8); // run of 8
        pixel_data.push(rgbe[ch]);
    }
    let hdr = build_hdr(8, 1, &pixel_data);
    let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
    assert_eq!(decoded.width, 8);
    for i in 0..8 {
        let (r, g, b) = pixel_f32(decoded.pixels(), i);
        assert_f32_close(r, 1.0, 0.02, &alloc::format!("px{i} R"));
        assert_f32_close(g, 0.5, 0.02, &alloc::format!("px{i} G"));
        assert_f32_close(b, 0.25, 0.02, &alloc::format!("px{i} B"));
    }
}

#[test]
fn rle_all_literals() {
    // 8x1, all distinct RGBE values → literal runs
    let rgbe_vals: Vec<[u8; 4]> = (0..8)
        .map(|i| f32_to_rgbe((i as f32 + 1.0) * 0.1, 0.5, 0.5))
        .collect();
    let mut pixel_data = Vec::new();
    pixel_data.extend_from_slice(&[2, 2, 0, 8]); // RLE marker, width=8
    for ch in 0..4 {
        pixel_data.push(8); // literal of 8
        for v in &rgbe_vals {
            pixel_data.push(v[ch]);
        }
    }
    let hdr = build_hdr(8, 1, &pixel_data);
    let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
    assert_eq!(decoded.width, 8);
}

#[test]
fn rle_mixed_runs_and_literals() {
    // 10x1: 4 identical + 6 varied → run of 4 + literal of 6
    let same = f32_to_rgbe(1.0, 1.0, 1.0);
    let varied: Vec<[u8; 4]> = (0..6)
        .map(|i| f32_to_rgbe((i as f32 + 1.0) * 0.1, 0.5, 0.3))
        .collect();

    let mut pixel_data = Vec::new();
    pixel_data.extend_from_slice(&[2, 2, 0, 10]); // RLE marker, width=10
    for ch in 0..4 {
        // Run of 4
        pixel_data.push(128 + 4);
        pixel_data.push(same[ch]);
        // Literal of 6
        pixel_data.push(6);
        for v in &varied {
            pixel_data.push(v[ch]);
        }
    }
    let hdr = build_hdr(10, 1, &pixel_data);
    let decoded = decode_hdr(&hdr, Unstoppable).unwrap();
    assert_eq!(decoded.width, 10);
    // First 4 pixels should be ~(1, 1, 1)
    for i in 0..4 {
        let (r, g, b) = pixel_f32(decoded.pixels(), i);
        assert_f32_close(r, 1.0, 0.02, &alloc::format!("run px{i} R"));
        assert_f32_close(g, 1.0, 0.02, &alloc::format!("run px{i} G"));
        assert_f32_close(b, 1.0, 0.02, &alloc::format!("run px{i} B"));
    }
}

// ══════════════════════════════════════════════════════════════════════
// RLE error conditions
// ══════════════════════════════════════════════════════════════════════

#[test]
fn rle_run_overflows_scanline() {
    // width=8, but channel run says 10
    let mut pixel_data = Vec::new();
    pixel_data.extend_from_slice(&[2, 2, 0, 8]); // width=8
    pixel_data.push(128 + 10); // run of 10 > 8
    pixel_data.push(42);
    let hdr = build_hdr(8, 1, &pixel_data);
    assert!(decode_hdr(&hdr, Unstoppable).is_err());
}

#[test]
fn rle_literal_overflows_scanline() {
    // width=8, but literal says 10
    let mut pixel_data = Vec::new();
    pixel_data.extend_from_slice(&[2, 2, 0, 8]); // width=8
    pixel_data.push(10); // literal of 10 > 8
    pixel_data.extend_from_slice(&[0; 10]);
    let hdr = build_hdr(8, 1, &pixel_data);
    assert!(decode_hdr(&hdr, Unstoppable).is_err());
}

#[test]
fn rle_width_mismatch() {
    // Header says width=8 but RLE marker says width=10
    let mut pixel_data = Vec::new();
    pixel_data.extend_from_slice(&[2, 2, 0, 10]); // marker says width=10
    let hdr = build_hdr(8, 1, &pixel_data); // header says width=8
    assert!(decode_hdr(&hdr, Unstoppable).is_err());
}

#[test]
fn rle_truncated_data() {
    let mut pixel_data = Vec::new();
    pixel_data.extend_from_slice(&[2, 2, 0, 8]); // RLE marker
    pixel_data.push(128 + 8); // run of 8
    // Missing the value byte
    let hdr = build_hdr(8, 1, &pixel_data);
    assert!(decode_hdr(&hdr, Unstoppable).is_err());
}

#[test]
fn rle_zero_length_literal() {
    // Literal count of 0 is invalid
    let mut pixel_data = Vec::new();
    pixel_data.extend_from_slice(&[2, 2, 0, 8]);
    pixel_data.push(0); // literal of 0 — invalid
    let hdr = build_hdr(8, 1, &pixel_data);
    assert!(decode_hdr(&hdr, Unstoppable).is_err());
}

// ══════════════════════════════════════════════════════════════════════
// Encode from Rgb8
// ══════════════════════════════════════════════════════════════════════

#[test]
fn encode_rgb8_roundtrip() {
    let pixels = vec![255u8, 0, 0, 0, 255, 0, 0, 0, 255];
    let encoded = encode_hdr(&pixels, 3, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    let (r, g, b) = pixel_f32(decoded.pixels(), 0);
    assert_f32_close(r, 1.0, 0.02, "red R");
    assert_f32_close(g, 0.0, 0.02, "red G");
    assert_f32_close(b, 0.0, 0.02, "red B");
    let (r, g, b) = pixel_f32(decoded.pixels(), 1);
    assert_f32_close(r, 0.0, 0.02, "green R");
    assert_f32_close(g, 1.0, 0.02, "green G");
}

#[test]
fn encode_rgb8_black() {
    let pixels = vec![0u8, 0, 0];
    let encoded = encode_hdr(&pixels, 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
    let decoded = decode_hdr(&encoded, Unstoppable).unwrap();
    let (r, g, b) = pixel_f32(decoded.pixels(), 0);
    assert_eq!(r, 0.0);
    assert_eq!(g, 0.0);
    assert_eq!(b, 0.0);
}

// ══════════════════════════════════════════════════════════════════════
// Limits and cancellation
// ══════════════════════════════════════════════════════════════════════

#[test]
fn limits_max_width_hdr() {
    let hdr = build_hdr_from_f32(100, 1, &vec![(1.0, 1.0, 1.0); 100]);
    let limits = Limits {
        max_width: Some(50),
        ..Default::default()
    };
    assert!(decode_hdr_with_limits(&hdr, &limits, Unstoppable).is_err());
}

#[test]
fn limits_max_height_hdr() {
    let hdr = build_hdr_from_f32(1, 100, &vec![(1.0, 1.0, 1.0); 100]);
    let limits = Limits {
        max_height: Some(50),
        ..Default::default()
    };
    assert!(decode_hdr_with_limits(&hdr, &limits, Unstoppable).is_err());
}

#[test]
fn limits_max_memory_hdr() {
    let hdr = build_hdr_from_f32(3, 3, &vec![(1.0, 1.0, 1.0); 9]);
    let limits = Limits {
        max_memory_bytes: Some(10), // 9 pixels * 12 bytes = 108, way over 10
        ..Default::default()
    };
    assert!(decode_hdr_with_limits(&hdr, &limits, Unstoppable).is_err());
}

#[test]
fn cancellation_decode_hdr() {
    struct AlreadyStopped;
    impl enough::Stop for AlreadyStopped {
        fn check(&self) -> Result<(), enough::StopReason> {
            Err(enough::StopReason::Cancelled)
        }
    }

    let hdr = build_hdr_from_f32(1, 32, &vec![(1.0, 0.5, 0.25); 32]);
    assert!(matches!(
        decode_hdr(&hdr, AlreadyStopped),
        Err(BitmapError::Cancelled(_))
    ));
}

#[test]
fn cancellation_encode_hdr() {
    struct AlreadyStopped;
    impl enough::Stop for AlreadyStopped {
        fn check(&self) -> Result<(), enough::StopReason> {
            Err(enough::StopReason::Cancelled)
        }
    }

    let pixels: Vec<u8> = (0..32)
        .flat_map(|_| {
            let mut p = Vec::new();
            p.extend_from_slice(&1.0f32.to_le_bytes());
            p.extend_from_slice(&0.5f32.to_le_bytes());
            p.extend_from_slice(&0.25f32.to_le_bytes());
            p
        })
        .collect();
    assert!(matches!(
        encode_hdr(&pixels, 1, 32, PixelLayout::RgbF32, AlreadyStopped),
        Err(BitmapError::Cancelled(_))
    ));
}

// ══════════════════════════════════════════════════════════════════════
// Unsupported formats
// ══════════════════════════════════════════════════════════════════════

#[test]
fn encode_unsupported_gray8() {
    assert!(matches!(
        encode_hdr(&[0], 1, 1, PixelLayout::Gray8, Unstoppable),
        Err(BitmapError::UnsupportedVariant(_))
    ));
}

#[test]
fn encode_unsupported_rgba8() {
    assert!(matches!(
        encode_hdr(&[0; 4], 1, 1, PixelLayout::Rgba8, Unstoppable),
        Err(BitmapError::UnsupportedVariant(_))
    ));
}

#[test]
fn encode_buffer_too_small() {
    // Claim 2x2 RgbF32 (48 bytes) but only provide 12
    assert!(matches!(
        encode_hdr(&[0; 12], 2, 2, PixelLayout::RgbF32, Unstoppable),
        Err(BitmapError::BufferTooSmall { .. })
    ));
}
