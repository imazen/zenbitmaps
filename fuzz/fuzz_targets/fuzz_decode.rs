#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try auto-detect decode (PNM, BMP, farbfeld) — must never panic
    let _ = zenbitmaps::decode(data, enough::Unstoppable);

    // Try each format explicitly — must never panic
    let _ = zenbitmaps::decode_bmp(data, enough::Unstoppable);
    let _ = zenbitmaps::decode_farbfeld(data, enough::Unstoppable);
});
