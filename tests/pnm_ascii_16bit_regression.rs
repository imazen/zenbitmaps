//! Regression for fuzz zenpipe#51: ASCII PNM with `maxval > 255` (a 16-bit
//! layout such as Gray16) must decode to a buffer whose byte count matches the
//! layout, and must not panic in the `DecodeOutput::pixels()` / `as_slice()`
//! path. The pre-fix `decode_ascii_samples` emitted a single downscaled `u8`
//! per 16-bit sample — half the bytes the layout declared — so the declared
//! `stride·height` overran the short data (OOB panic in `PixelBuffer::as_slice`,
//! reached via `zencodecs::push_decode`), and 16-bit precision was silently lost.
//!
//! Exercises the always-on zencodec decode layer (`PnmDecoderConfig`).

use std::borrow::Cow;

use zenbitmaps::{PixelLayout, PnmDecoderConfig, Unstoppable};
use zencodec::decode::{Decode, DecodeJob, DecoderConfig};

#[test]
fn ascii_pnm_16bit_no_oob_panic_51() {
    // The exact fuzz repro: ASCII P2 (PGM), maxval 4444 → Gray16, 2×1.
    let data: &[u8] = &[
        80, 50, 50, 13, 49, 32, 13, 32, 52, 52, 52, 52, 80, 50, 50, 13, 49, 32, 13, 52, 50, 58, 55,
    ];
    // The crash path: decode through the zencodec layer, then borrow the buffer
    // (`pixels()` → `as_slice()`). Either a consistent `Ok` or a clean `Err` is
    // acceptable — it must not panic.
    if let Ok(decoder) = PnmDecoderConfig::new()
        .job()
        .decoder(Cow::Borrowed(data), &[])
        && let Ok(out) = decoder.decode()
    {
        // `pixels()` borrows the buffer via `PixelBuffer::as_slice`, which
        // panicked pre-fix (declared stride·height overran the short data).
        // Reaching past it means the buffer is self-consistent.
        let px = out.pixels();
        assert!(px.width() >= 1 && px.rows() >= 1);
    }
}

#[test]
fn ascii_pnm_16bit_preserves_precision_51() {
    // A clean P2 1×1 with maxval 1000 and sample 500 must round-trip the full
    // 16-bit value, NOT downscale it to a single u8 (the pre-fix lossy path).
    let data = b"P2 1 1 1000 500 ";
    let d = zenbitmaps::decode(data, Unstoppable).expect("must decode");
    assert_eq!(d.layout, PixelLayout::Gray16, "maxval>255 → 16-bit layout");
    assert_eq!(d.pixels().len(), 2, "one Gray16 sample = 2 bytes");
    assert_eq!(
        u16::from_ne_bytes([d.pixels()[0], d.pixels()[1]]),
        500,
        "16-bit sample value must be preserved, not downscaled to u8"
    );
}
