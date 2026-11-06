//! Regression for fuzz zenpipe#51: ASCII PNM with `maxval > 255` (a 16-bit
//! layout such as Gray16) must decode to a buffer whose byte count matches the
//! layout, and must not panic in the `DecodeOutput::pixels()` / `as_slice()`
//! path. The pre-fix `decode_ascii_samples` emitted a single downscaled `u8`
//! per 16-bit sample — half the bytes the layout declared — so the declared
//! `stride·height` overran the short data (OOB panic in `PixelBuffer::as_slice`,
//! reached via `zencodecs::push_decode`), and 16-bit precision was silently lost.
//!
//! Exercises the `zencodec` decode layer (`PnmDecoderConfig`), gated behind the
//! `zencodec` feature.
#![cfg(feature = "zencodec")]

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
        32768,
        "half-white must normalize to the full U16 range"
    );
}

#[test]
fn pnm_exact_depth_describes_current_samples() {
    for (maxval, expected) in [(1023, 10), (4095, 12), (1000, 16)] {
        let text = format!("P2 3 1 {maxval} 0 1 {} ", maxval / 2);
        let decoded = PnmDecoderConfig::new()
            .job()
            .decoder(Cow::Borrowed(text.as_bytes()), &[])
            .unwrap()
            .decode()
            .unwrap();
        let pixels = decoded.pixels();
        let evidence = pixels
            .exact_depth()
            .expect("integer samples carry measured evidence");
        assert_eq!(evidence.color_bits(), expected, "maxval {maxval}");
        assert!(evidence.holds_for(&pixels).unwrap());
    }
}

#[test]
fn deep_native_pnm_encoding_preserves_strided_samples() {
    use zencodec::encode::{EncodeJob as _, Encoder as _, EncoderConfig as _};
    use zenpixels::{PixelDescriptor, PixelSlice};
    for descriptor in [
        PixelDescriptor::GRAY16_SRGB,
        PixelDescriptor::RGB16_SRGB,
        PixelDescriptor::RGBA16_SRGB,
    ] {
        let width = 3;
        let row_bytes = width * descriptor.bytes_per_pixel();
        let stride = row_bytes + descriptor.bytes_per_pixel() * 2;
        let mut bytes = vec![0xa5; stride + row_bytes];
        for y in 0..2 {
            for sample in 0..row_bytes / 2 {
                let value = ((y * 631 + sample * 997 + 1) & 65535) as u16;
                bytes[y * stride + sample * 2..y * stride + sample * 2 + 2]
                    .copy_from_slice(&value.to_ne_bytes());
            }
        }
        let slice = PixelSlice::new(&bytes, width as u32, 2, stride, descriptor).unwrap();
        let encoded = zenbitmaps::PnmEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(slice.clone())
            .unwrap();
        let mut streamed = zenbitmaps::PnmEncoderConfig::new().job().encoder().unwrap();
        streamed.begin(width as u32, 2, descriptor).unwrap();
        streamed.push_rows(slice).unwrap();
        assert_eq!(streamed.finish().unwrap().data(), encoded.data());
        let decoded = PnmDecoderConfig::new()
            .job()
            .decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(
            decoded.pixels().descriptor().channel_type(),
            descriptor.channel_type()
        );
        assert_eq!(decoded.pixels().descriptor().layout(), descriptor.layout());
        for y in 0..2 {
            assert_eq!(
                decoded.pixels().row(y),
                &bytes[y as usize * stride..y as usize * stride + row_bytes]
            );
        }
    }
}
