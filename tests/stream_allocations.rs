//! A strip pushed into a drained streaming encode is written straight into
//! the encoder's output buffer by the format's row writer. Once that buffer
//! has grown to a strip, a packed strip allocates nothing: no per-strip
//! temporary file, no header to cut off, no copy into the output.
//!
//! The counting allocator is process-wide, so this file holds one test.
#![cfg(feature = "zencodec")]

use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use std::alloc::System;

use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
use zenpixels::{PixelDescriptor, PixelSlice};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn noise(n: usize) -> Vec<u8> {
    let mut x = 0x2545_f491u32;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

/// Allocations and reallocations made by the third push of a strip into a
/// stream drained after every push.
fn third_push_allocs<C: EncoderConfig>(cfg: C, desc: PixelDescriptor) -> usize
where
    <C::Job as EncodeJob>::Enc: Encoder,
{
    let (w, strip, h) = (512u32, 8u32, 32u32);
    let stride = w as usize * desc.bytes_per_pixel();
    let px = noise(stride * h as usize);
    let mut enc = cfg.job().encoder().unwrap();
    enc.begin(w, h, desc).unwrap();
    let drain = |enc: &mut <C::Job as EncodeJob>::Enc| {
        let n = enc.pending_output().len();
        enc.consume_output(n);
    };
    drain(&mut enc);
    let mut counted = 0;
    for i in 0..3u32 {
        let rows = &px[(i * strip) as usize * stride..][..strip as usize * stride];
        let slice = PixelSlice::new(rows, w, strip, stride, desc).unwrap();
        let region = Region::new(GLOBAL);
        enc.push_rows(slice).unwrap();
        let change = region.change();
        counted = change.allocations + change.reallocations;
        drain(&mut enc);
    }
    counted
}

#[test]
fn a_packed_strip_into_a_drained_stream_allocates_nothing() {
    use zenbitmaps::{FarbfeldEncoderConfig, PnmEncoderConfig};
    let cases: [(&str, usize); 5] = [
        (
            "PPM",
            third_push_allocs(PnmEncoderConfig::new(), PixelDescriptor::RGB8_SRGB),
        ),
        (
            "PGM",
            third_push_allocs(PnmEncoderConfig::new(), PixelDescriptor::GRAY8_SRGB),
        ),
        (
            "PAM",
            third_push_allocs(PnmEncoderConfig::new(), PixelDescriptor::RGBA8_SRGB),
        ),
        (
            "PPM from BGRA",
            third_push_allocs(PnmEncoderConfig::new(), PixelDescriptor::BGRA8_SRGB),
        ),
        (
            "farbfeld",
            third_push_allocs(FarbfeldEncoderConfig::new(), PixelDescriptor::RGBA8_SRGB),
        ),
    ];
    for (name, n) in cases {
        assert_eq!(
            n, 0,
            "{name}: a strip into a drained stream made {n} allocations"
        );
    }
    // HDR converts each scanline to RGBE in one scratch row per strip: that
    // is its only allocation.
    #[cfg(feature = "hdr")]
    {
        let n = third_push_allocs(
            zenbitmaps::HdrEncoderConfig::new(),
            PixelDescriptor::RGB8_SRGB,
        );
        assert!(
            n <= 1,
            "HDR: a strip into a drained stream made {n} allocations"
        );
    }
}
