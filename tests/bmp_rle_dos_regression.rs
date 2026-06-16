//! Regression for a fuzz-farm DoS: a tiny RLE BMP declaring a huge image.
//!
//! A 158-byte RLE4 BMP with width 1 / height ≈ 2.7e8 decoded in ~18 s — the
//! declared output (~134 MB) passed the 1 GiB output cap, so the decoder
//! allocated it and ran per-output-pixel post-processing (palette expansion,
//! vertical flip, format conversion) over 268M pixels. Two fixes: `decode_rle4`
//! now stops at input exhaustion (matching `decode_rle8plus`), and `decode_rle`
//! rejects an output far larger than the compressed stream could encode
//! (decompression-bomb ratio guard). The bomb must now be rejected, not slow.
//!
//! Uses `decode_bmp`, which is gated behind the `bmp` feature.
#![cfg(feature = "bmp")]

use std::time::Instant;

/// The exact fuzz repro (RLE4, width 1, height ~2.7e8).
const BOMB: &[u8] = &[
    66, 77, 80, 1, 0, 0, 0, 0, 0, 0, 107, 0, 0, 0, 64, 0, 0, 0, 1, 0, 0, 0, 0, 2, 0, 16, 1, 0, 4,
    0, 1, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 231, 255, 69, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 57, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 65, 65, 65, 65, 65, 48,
    48, 48, 8, 0, 0, 0, 0, 0, 0, 0, 52, 72, 48, 80, 55, 0, 0, 0, 80, 51, 107, 0, 0, 66, 0, 64, 77,
    0, 0, 0, 1, 0, 52, 35, 66, 166, 1, 0, 65, 65,
];

#[test]
fn rle_decompression_bomb_rejected_fast() {
    let t = Instant::now();
    // A 158-byte file cannot legitimately encode a ~134 MB image: reject it.
    let r = zenbitmaps::decode_bmp(BOMB, enough::Unstoppable);
    assert!(
        r.is_err(),
        "tiny RLE BMP declaring a huge image must be rejected"
    );
    // Must be fast — pre-fix this ran ~18 s. Generous bound to avoid flakiness.
    assert!(
        t.elapsed().as_secs() < 2,
        "decode took {:?} — decompression-bomb guard not effective",
        t.elapsed()
    );
}
