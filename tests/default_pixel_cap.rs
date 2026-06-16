//! End-to-end regression for zenbitmaps#13: the default decode path (no
//! explicit `Limits`) must enforce a 120 MP pixel cap, not silently admit a
//! huge low-bpp image just because its declared bytes fit under the 1 GiB
//! output-byte cap.
//!
//! `decode_bmp(data, stop)` calls `bmp::decode(data, None, ..)` — the exact
//! untrusted-input path with no limits supplied. Before this fix, a crafted
//! header declaring e.g. 225 MP would pass the dimension stage (no default
//! pixel cap) and proceed to allocate. Now the default `DEFAULT_MAX_PIXELS`
//! (120 MP) fires at the pre-flight dimension check, rejecting the header
//! before any large allocation. The byte cap is unchanged and still applies.
//!
//! These build a tiny synthetic BMP header (~58 bytes) — they never allocate
//! a real multi-megapixel buffer. The dimension check runs immediately after
//! header parse, before pixel decode, so a truncated pixel-data region is
//! fine: rejection is on the pixel count, not on EOF.
#![cfg(feature = "bmp")]

use enough::Unstoppable;
use zenbitmaps::{BitmapError, Limits, decode_bmp, decode_bmp_with_limits};

/// Build a minimal uncompressed 24-bit BMP header declaring `width`x`height`.
///
/// The pixel-data region is intentionally tiny/truncated — the dimension cap
/// is checked before any pixel bytes are read, so this never allocates the
/// declared image.
fn make_24bit_bmp_header(width: i32, height: i32) -> Vec<u8> {
    let mut buf = Vec::new();
    // ── File header (14 bytes) ──
    buf.extend_from_slice(b"BM");
    buf.extend_from_slice(&58u32.to_le_bytes()); // (nominal) file size
    buf.extend_from_slice(&[0u8; 4]); // reserved
    buf.extend_from_slice(&54u32.to_le_bytes()); // data offset

    // ── DIB header (BITMAPINFOHEADER, 40 bytes) ──
    buf.extend_from_slice(&40u32.to_le_bytes()); // header size
    buf.extend_from_slice(&width.to_le_bytes()); // width
    buf.extend_from_slice(&height.to_le_bytes()); // height (positive = bottom-up)
    buf.extend_from_slice(&1u16.to_le_bytes()); // planes
    buf.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
    buf.extend_from_slice(&0u32.to_le_bytes()); // compression (BI_RGB, uncompressed)
    buf.extend_from_slice(&0u32.to_le_bytes()); // image data size (0 = derive)
    buf.extend_from_slice(&2835u32.to_le_bytes()); // X pixels per meter (~72 DPI)
    buf.extend_from_slice(&2835u32.to_le_bytes()); // Y pixels per meter
    buf.extend_from_slice(&0u32.to_le_bytes()); // colors used
    buf.extend_from_slice(&0u32.to_le_bytes()); // important colors

    // A few stray pixel bytes (truncated on purpose).
    buf.extend_from_slice(&[0xFF, 0x00, 0x00, 0x00]);
    buf
}

/// 15000 × 15000 = 225 MP — over the 120 MP default but only ~675 MB at
/// 24-bit (well under the 1 GiB byte cap), so the byte cap alone would NOT
/// catch it. The default pixel cap must.
const OVER_W: i32 = 15_000;
const OVER_H: i32 = 15_000;

/// 10000 × 10000 = 100 MP — under the 120 MP default; the header parses and
/// the dimension check passes (decode then fails later on truncated data, but
/// NOT with a pixel-count limit error).
const UNDER_W: i32 = 10_000;
const UNDER_H: i32 = 10_000;

fn is_pixel_count_limit_err(
    r: &Result<zenbitmaps::DecodeOutput<'_>, zenbitmaps::At<BitmapError>>,
) -> bool {
    matches!(
        r.as_ref().map_err(|e| e.error()),
        Err(BitmapError::LimitExceeded(msg)) if msg.contains("pixel count")
    )
}

#[test]
fn default_path_rejects_over_120mp() {
    let bmp = make_24bit_bmp_header(OVER_W, OVER_H);
    let r = decode_bmp(&bmp, Unstoppable);
    assert!(
        is_pixel_count_limit_err(&r),
        "decode_bmp with NO explicit limits must reject a {OVER_W}x{OVER_H} \
         ({} MP) header via the default 120 MP pixel cap; got {r:?}",
        (OVER_W as u64 * OVER_H as u64) / 1_000_000
    );
}

#[test]
fn default_path_does_not_apply_pixel_cap_under_120mp() {
    // A 100 MP header must clear the dimension stage. Decode ultimately fails
    // on the truncated pixel region, but it must NOT be a pixel-count cap
    // rejection — proving the default ceiling does not over-reject valid sizes.
    let bmp = make_24bit_bmp_header(UNDER_W, UNDER_H);
    let r = decode_bmp(&bmp, Unstoppable);
    assert!(
        !is_pixel_count_limit_err(&r),
        "a {UNDER_W}x{UNDER_H} (100 MP) header must pass the 120 MP default \
         pixel cap (it may fail later for other reasons); got {r:?}"
    );
}

#[test]
fn explicit_unlimited_opts_out_of_default_cap() {
    // The documented opt-out: max_pixels = Some(u64::MAX). The >120 MP header
    // must then clear the dimension stage (decode fails later on truncation,
    // but NOT with a pixel-count cap error).
    let bmp = make_24bit_bmp_header(OVER_W, OVER_H);
    let unlimited = Limits {
        max_pixels: Some(u64::MAX),
        ..Limits::default()
    };
    let r = decode_bmp_with_limits(&bmp, &unlimited, Unstoppable);
    assert!(
        !is_pixel_count_limit_err(&r),
        "max_pixels: Some(u64::MAX) must opt out of the default 120 MP pixel \
         cap so the {OVER_W}x{OVER_H} header is admitted; got {r:?}"
    );
}

#[test]
fn explicit_lower_cap_overrides_default() {
    // A tighter explicit cap (1 MP) must reject the 100-MP-but-under-default
    // header — proving callers can still set a lower ceiling than the default.
    let bmp = make_24bit_bmp_header(UNDER_W, UNDER_H);
    let tight = Limits {
        max_pixels: Some(1_000_000),
        ..Limits::default()
    };
    let r = decode_bmp_with_limits(&bmp, &tight, Unstoppable);
    assert!(
        is_pixel_count_limit_err(&r),
        "an explicit 1 MP cap must reject a 100 MP header; got {r:?}"
    );
}
