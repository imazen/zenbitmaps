//! QOI on zencodec's sans-I/O contract: the incremental decoder against
//! the one-shot decoder at every chunk size, the streaming encoder against
//! the one-shot encoder byte for byte (with and without `begin`, draining
//! output as rows arrive), truncation, limits, and a zero-copy
//! decode-to-encode transcode with bounded output.

#![cfg(all(feature = "qoi", feature = "zencodec", feature = "std"))]

use std::borrow::Cow;

use enough::Unstoppable;
use zenbitmaps::{PixelLayout, QoiDecoderConfig, QoiEncoderConfig};
use zencodec::decode::{Decode, DecodeJob, DecoderConfig, Event, Input, StepError};
use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
use zencodec_io::{CollectSink, DriveError, EncoderSink, drive_from_slice, pump_read_to_sink};
use zenpixels::{PixelDescriptor, PixelSlice};

/// Deterministic images with runs that cross row boundaries, flat areas,
/// noise and gradients: everything QOI's opcodes react to.
fn image(kind: u32, w: u32, h: u32, channels: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(w as usize * h as usize * channels);
    let mut lcg = 0x1234_5678u32 ^ kind;
    for y in 0..h {
        for x in 0..w {
            lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let px: [u8; 4] = match kind {
                0 => [(x ^ y) as u8, (x * 3) as u8, (y * 7) as u8, 255], // codecs.rs pattern
                1 => [
                    (lcg >> 24) as u8,
                    (lcg >> 16) as u8,
                    (lcg >> 8) as u8,
                    lcg as u8,
                ], // noise
                2 => [(x / 16) as u8 * 16, 40, (y / 16) as u8 * 16, 255], // flat blocks: long runs
                3 => [0, 0, 0, 255],                                     // one run per image
                _ => [(x as u8).wrapping_add(y as u8), 128, 64, 200],
            };
            v.extend_from_slice(&px[..channels]);
        }
    }
    v
}

fn encode_oneshot(pixels: &[u8], w: u32, h: u32, channels: usize) -> Vec<u8> {
    let layout = if channels == 4 {
        PixelLayout::Rgba8
    } else {
        PixelLayout::Rgb8
    };
    zenbitmaps::encode_qoi(pixels, w, h, layout, Unstoppable).unwrap()
}

fn desc(channels: usize) -> PixelDescriptor {
    if channels == 4 {
        PixelDescriptor::RGBA8
    } else {
        PixelDescriptor::RGB8
    }
}

fn cases() -> Vec<(u32, u32, u32, usize)> {
    let mut v = Vec::new();
    for kind in 0..5 {
        for &(w, h) in &[
            (1u32, 1u32),
            (3, 2),
            (17, 9),
            (64, 64),
            (200, 33),
            (256, 256),
        ] {
            for ch in [3usize, 4] {
                v.push((kind, w, h, ch));
            }
        }
    }
    v
}

#[test]
fn streaming_encoder_is_byte_identical_to_one_shot() {
    for (kind, w, h, ch) in cases() {
        let pixels = image(kind, w, h, ch);
        let expected = encode_oneshot(&pixels, w, h, ch);
        let stride = w as usize * ch;
        for strip in [1u32, 2, 7, 16, h] {
            let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
            let mut y = 0;
            while y < h {
                let rows = (h - y).min(strip);
                let start = y as usize * stride;
                let s = PixelSlice::new(
                    &pixels[start..start + rows as usize * stride],
                    w,
                    rows,
                    stride,
                    desc(ch),
                )
                .unwrap();
                enc.push_rows(s).unwrap();
                y += rows;
            }
            let out = enc.finish().unwrap().into_vec();
            assert_eq!(out, expected, "kind {kind} {w}x{h}x{ch} strip {strip}");
        }
        // The one-shot zencodec path may narrow opaque RGBA to RGB (its
        // load-bearing reduction); its bytes decode to the same pixels.
        let one = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::new(&pixels, w, h, stride, desc(ch)).unwrap())
            .unwrap()
            .into_vec();
        let (_, _, och, opix) = decode_oneshot(&one);
        if och == ch {
            assert_eq!(opix, pixels);
        } else {
            assert_eq!(och, 3, "narrowing only drops dead alpha");
            let rgb: Vec<u8> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| p[..3].to_vec())
                .collect();
            assert_eq!(opix, rgb);
        }
    }
}

#[test]
fn streaming_encoder_swizzles_bgra() {
    let (w, h) = (37u32, 11u32);
    let rgba = image(1, w, h, 4);
    let mut bgra = rgba.clone();
    for p in bgra.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    let expected = encode_oneshot(&rgba, w, h, 4);
    let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
    let s = PixelSlice::new(&bgra, w, h, w as usize * 4, PixelDescriptor::BGRA8_SRGB).unwrap();
    enc.push_rows(s).unwrap();
    assert_eq!(enc.finish().unwrap().into_vec(), expected);
}

fn decode_oneshot(data: &[u8]) -> (u32, u32, usize, Vec<u8>) {
    let out = zenbitmaps::decode_qoi(data, Unstoppable).unwrap();
    let ch = out.layout.bytes_per_pixel();
    (out.width, out.height, ch, out.pixels().to_vec())
}

#[test]
fn incremental_decoder_matches_one_shot_at_every_chunk_size() {
    for (kind, w, h, ch) in cases() {
        let pixels = image(kind, w, h, ch);
        let data = encode_oneshot(&pixels, w, h, ch);
        let (ow, oh, och, opix) = decode_oneshot(&data);
        assert_eq!((ow, oh, och), (w, h, ch));
        assert_eq!(opix, pixels);
        for chunk in [1usize, 7, 64, 4096, data.len()] {
            let mut dec = QoiDecoderConfig::new()
                .job()
                .dyn_incremental_decoder(&[])
                .unwrap();
            let mut sink = CollectSink::new();
            let (info, stats) = drive_from_slice(&mut *dec, &data, &mut sink, None, chunk)
                .unwrap_or_else(|e| panic!("kind {kind} {w}x{h}x{ch} chunk {chunk}: {e}"));
            assert_eq!((info.width, info.height), (w, h));
            assert_eq!(sink.descriptor(), Some(desc(ch)));
            assert!(sink.finished());
            assert_eq!(
                sink.bytes(),
                &pixels[..],
                "kind {kind} {w}x{h}x{ch} chunk {chunk}"
            );
            assert_eq!(sink.rows_written(), h as u64, "{stats:?}");
            assert_eq!(dec.info().map(|i| (i.width, i.height)), Some((w, h)));
        }
        // Through the sequential stream pump too.
        let mut dec = QoiDecoderConfig::new()
            .job()
            .dyn_incremental_decoder(&[])
            .unwrap();
        let mut sink = CollectSink::new();
        pump_read_to_sink(
            std::io::Cursor::new(&data),
            &mut *dec,
            &mut sink,
            None,
            512,
            1 << 20,
        )
        .unwrap();
        assert_eq!(sink.bytes(), &pixels[..]);
    }
}

#[test]
fn direct_strips_are_used_when_input_suffices() {
    // 64x64 RGB flat blocks: compressed input is tiny, so after the
    // whole file is fed every strip goes straight into the sink in
    // 64-row strips (one `Rows` event of 64 rows).
    let (w, h, ch) = (64u32, 64u32, 3usize);
    let pixels = image(2, w, h, ch);
    let data = encode_oneshot(&pixels, w, h, ch);
    let mut dec = QoiDecoderConfig::new()
        .job()
        .dyn_incremental_decoder(&[])
        .unwrap();
    let mut sink = CollectSink::new();
    let total = data.len() as u64;
    let p = dec
        .step(Input::empty(), &mut sink, &zencodec::NoPulse)
        .unwrap();
    assert_eq!((p.consumed, p.event), (0, Event::Need));
    let p = dec
        .step(
            Input::new(0, &data).with_total_len(total),
            &mut sink,
            &zencodec::NoPulse,
        )
        .unwrap();
    assert_eq!(p.consumed, 14, "the header only");
    assert!(matches!(p.event, Event::Metadata(_)));
    let p = dec
        .step(
            Input::new(14, &data[14..]).with_total_len(total),
            &mut sink,
            &zencodec::NoPulse,
        )
        .unwrap();
    match p.event {
        Event::Rows { rows, .. } => assert_eq!(rows, 64),
        other => panic!("{other:?}"),
    }
    assert!(p.consumed < data.len() - 14, "the end marker is left");
    let rest = 14 + p.consumed;
    let p = dec
        .step(
            Input::new(rest as u64, &data[rest..]).with_total_len(total),
            &mut sink,
            &zencodec::NoPulse,
        )
        .unwrap();
    assert!(p.event.is_done());
    assert_eq!(sink.bytes(), &pixels[..]);
}

#[test]
fn truncated_input_is_an_error_at_every_cut() {
    let (w, h, ch) = (40u32, 24u32, 4usize);
    let pixels = image(1, w, h, ch);
    let data = encode_oneshot(&pixels, w, h, ch);
    for cut in [
        0usize,
        5,
        13,
        14,
        20,
        data.len() / 2,
        data.len() - 9,
        data.len() - 1,
    ] {
        let mut dec = QoiDecoderConfig::new()
            .job()
            .dyn_incremental_decoder(&[])
            .unwrap();
        let mut sink = CollectSink::new();
        let one_shot = zenbitmaps::decode_qoi(&data[..cut], Unstoppable);
        match drive_from_slice(&mut *dec, &data[..cut], &mut sink, None, 64) {
            Err(DriveError::Step(StepError::Codec(_))) => {
                assert!(one_shot.is_err(), "cut {cut}: paths disagree")
            }
            // Cutting inside the 8-byte end marker leaves every pixel; the
            // end marker is not pixel data (the inventory classifies it as
            // skipped), so both paths accept it.
            Ok(_) if cut >= data.len() - 8 => {
                assert!(one_shot.is_ok(), "cut {cut}: paths disagree");
                assert_eq!(sink.bytes(), &pixels[..]);
            }
            other => panic!("cut {cut}: {other:?}"),
        }
    }
}

#[test]
fn testkit_honesty_checks_pass_for_qoi() {
    let (w, h, ch) = (40u32, 24u32, 4usize);
    let pixels = image(0, w, h, ch);
    let data = encode_oneshot(&pixels, w, h, ch);
    zencodec_testkit::check_incremental_capability_honesty(QoiDecoderConfig::new(), &data).unwrap();
    zencodec_testkit::check_seekable_capability_honesty(QoiDecoderConfig::new(), &data).unwrap();
    for img in [
        zencodec_testkit::TestImage::rgba8_gradient(40, 24),
        // One pixel wide (the run edge case), tall enough that the check can
        // verify the lag: it refuses images of fewer than lag + 2 strips.
        zencodec_testkit::TestImage::rgba8_gradient(1, 64),
        zencodec_testkit::TestImage::rgba8_gradient(97, 33),
    ] {
        zencodec_testkit::check_streaming_output_honesty(
            QoiEncoderConfig::new(),
            QoiDecoderConfig::new(),
            &img,
        )
        .unwrap();
    }
}

#[test]
fn input_limit_is_enforced_on_fed_bytes() {
    let (w, h, ch) = (64u32, 64u32, 4usize);
    let pixels = image(1, w, h, ch);
    let data = encode_oneshot(&pixels, w, h, ch);
    let limits = zencodec::ResourceLimits::none().with_max_input_bytes(100);
    let mut dec = QoiDecoderConfig::new()
        .job()
        .with_limits(limits)
        .dyn_incremental_decoder(&[])
        .unwrap();
    let mut sink = CollectSink::new();
    let err = drive_from_slice(&mut *dec, &data, &mut sink, None, 64).unwrap_err();
    assert!(
        matches!(err, DriveError::Step(StepError::Codec(_))),
        "{err:?}"
    );
}

#[test]
fn transcode_through_encoder_sink_matches_one_shot() {
    for (kind, w, h, ch) in cases() {
        let pixels = image(kind, w, h, ch);
        let data = encode_oneshot(&pixels, w, h, ch);
        let mut dec = QoiDecoderConfig::new()
            .job()
            .dyn_incremental_decoder(&[])
            .unwrap();
        let enc = QoiEncoderConfig::new().job().encoder().unwrap();
        let mut sink = EncoderSink::new(enc);
        drive_from_slice(&mut *dec, &data, &mut sink, None, 1024).unwrap();
        assert_eq!(sink.rows_pushed(), h as u64);
        let out = sink.finish().unwrap().into_vec();
        assert_eq!(out, data, "kind {kind} {w}x{h}x{ch}");
    }
}

#[test]
fn one_shot_zencodec_decode_still_agrees() {
    let (w, h, ch) = (33u32, 9u32, 3usize);
    let pixels = image(4, w, h, ch);
    let data = encode_oneshot(&pixels, w, h, ch);
    let out = QoiDecoderConfig::new()
        .job()
        .decoder(Cow::Borrowed(&data), &[])
        .unwrap()
        .decode()
        .unwrap();
    let ps = out.pixels();
    let mut got = Vec::new();
    for y in 0..ps.rows() {
        got.extend_from_slice(ps.row(y));
    }
    assert_eq!(got, pixels);
}

#[test]
fn every_small_chunk_size_is_pixel_exact() {
    // Odd window sizes put the boundary inside every opcode kind (1, 2, 4
    // and 5 bytes) and inside runs that cross rows.
    for (kind, w, h, ch) in [
        (1u32, 13u32, 7u32, 4usize),
        (1, 9, 5, 3),
        (2, 40, 6, 3),
        (4, 17, 9, 4),
        (3, 21, 4, 4),
    ] {
        let pixels = image(kind, w, h, ch);
        let data = encode_oneshot(&pixels, w, h, ch);
        for chunk in 1..=40usize {
            let mut dec = QoiDecoderConfig::new()
                .job()
                .dyn_incremental_decoder(&[])
                .unwrap();
            let mut sink = CollectSink::new();
            drive_from_slice(&mut *dec, &data, &mut sink, None, chunk)
                .unwrap_or_else(|e| panic!("kind {kind} {w}x{h}x{ch} chunk {chunk}: {e}"));
            assert_eq!(
                sink.bytes(),
                &pixels[..],
                "kind {kind} {w}x{h}x{ch} chunk {chunk}"
            );
            let mut dec = QoiDecoderConfig::new()
                .job()
                .dyn_incremental_decoder(&[])
                .unwrap();
            let mut sink = CollectSink::new();
            pump_read_to_sink(
                OneShotReader {
                    data: &data,
                    pos: 0,
                    max: chunk,
                },
                &mut *dec,
                &mut sink,
                None,
                512,
                1 << 20,
            )
            .unwrap_or_else(|e| panic!("pump kind {kind} chunk {chunk}: {e}"));
            assert_eq!(sink.bytes(), &pixels[..], "pump kind {kind} chunk {chunk}");
        }
    }
}

/// A reader that returns at most `max` bytes per call.
struct OneShotReader<'a> {
    data: &'a [u8],
    pos: usize,
    max: usize,
}

impl std::io::Read for OneShotReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = (self.data.len() - self.pos).min(buf.len()).min(self.max);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Push `pixels` in strips of `strip` rows, draining `pending_output` after
/// every strip. Returns the concatenated output and the largest amount of
/// output ever pending.
fn encode_streaming(
    pixels: &[u8],
    w: u32,
    h: u32,
    ch: usize,
    strip: u32,
    declare: bool,
) -> (Vec<u8>, usize) {
    let stride = w as usize * ch;
    let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
    if declare {
        assert_eq!(
            enc.begin(w, h, desc(ch)).unwrap(),
            zencodec::encode::OutputTiming::streaming(0)
        );
    }
    let mut out = Vec::new();
    let mut peak = 0;
    let mut y = 0;
    while y < h {
        let rows = (h - y).min(strip);
        let start = y as usize * stride;
        let s = PixelSlice::new(
            &pixels[start..start + rows as usize * stride],
            w,
            rows,
            stride,
            desc(ch),
        )
        .unwrap();
        enc.push_rows(s).unwrap();
        let pending = enc.pending_output();
        peak = peak.max(pending.len());
        out.extend_from_slice(pending);
        let n = pending.len();
        enc.consume_output(n);
        y += rows;
    }
    out.extend_from_slice(&enc.finish().unwrap().into_vec());
    (out, peak)
}

#[test]
fn declared_encoder_streams_output_and_stays_byte_identical() {
    assert!(QoiEncoderConfig::capabilities().streaming_output());
    assert!(QoiEncoderConfig::capabilities().push_rows());
    for (kind, w, h, ch) in cases() {
        let pixels = image(kind, w, h, ch);
        let expected = encode_oneshot(&pixels, w, h, ch);
        for strip in [1u32, 7, h] {
            let (out, peak) = encode_streaming(&pixels, w, h, ch, strip, true);
            assert_eq!(out, expected, "kind {kind} {w}x{h}x{ch} strip {strip}");
            // A strip's output is at most 5 bytes per pixel plus the header.
            assert!(
                peak <= 14 + (strip as usize * w as usize) * 5,
                "kind {kind} {w}x{h}x{ch} strip {strip}: {peak} bytes pending"
            );
            // Without `begin` the header height is patched at the end, so
            // nothing streams early, and the bytes are the same.
            let (out, peak) = encode_streaming(&pixels, w, h, ch, strip, false);
            assert_eq!(out, expected);
            assert_eq!(peak, 0);
        }
    }
}

#[test]
fn declared_height_is_enforced() {
    let (w, h, ch) = (8u32, 4u32, 4usize);
    let pixels = image(1, w, h, ch);
    let stride = w as usize * ch;
    let strip = |rows: u32| {
        PixelSlice::new(&pixels[..rows as usize * stride], w, rows, stride, desc(ch)).unwrap()
    };

    let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
    enc.begin(w, 2, desc(ch)).unwrap();
    enc.push_rows(strip(2)).unwrap();
    assert!(enc.push_rows(strip(1)).is_err(), "more rows than declared");

    let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
    enc.begin(w, h, desc(ch)).unwrap();
    enc.push_rows(strip(2)).unwrap();
    assert!(enc.finish().is_err(), "fewer rows than declared");

    let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
    enc.push_rows(strip(1)).unwrap();
    assert!(enc.begin(w, h, desc(ch)).is_err(), "begin after rows");

    let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
    enc.begin(w, h, PixelDescriptor::RGBA8).unwrap();
    let bgra =
        PixelSlice::new(&pixels[..stride], w, 1, stride, PixelDescriptor::BGRA8_SRGB).unwrap();
    assert!(enc.push_rows(bgra).is_err(), "layout changed after begin");
}

#[test]
fn streaming_transcode_keeps_output_bounded() {
    // Decode a QOI stream and re-encode it with output drained after every
    // decoded strip: the bytes match, and output never piles up.
    let (w, h, ch) = (256u32, 256u32, 4usize);
    let pixels = image(1, w, h, ch);
    let data = encode_oneshot(&pixels, w, h, ch);
    let mut dec = QoiDecoderConfig::new()
        .job()
        .dyn_incremental_decoder(&[])
        .unwrap();
    let enc = QoiEncoderConfig::new().job().encoder().unwrap();
    let mut sink = EncoderSink::new(enc);
    let mut out = Vec::new();
    let mut peak = 0usize;
    let total = data.len() as u64;
    let mut at = 0usize;
    let mut window = 0usize;
    loop {
        let end = (at + window).min(data.len());
        let input = Input::new(at as u64, &data[at..end]).with_total_len(total);
        let p = dec.step(input, &mut sink, &zencodec::NoPulse).unwrap();
        at += p.consumed;
        window -= p.consumed.min(window);
        peak = peak.max(sink.pending_output().len());
        sink.write_pending(&mut out).unwrap();
        match p.event {
            Event::Done(_) => break,
            Event::Need => {
                let r = dec.requested()[0];
                at = r.offset().unwrap() as usize;
                window = 4096;
            }
            _ => {}
        }
    }
    out.extend_from_slice(&sink.finish().unwrap().into_vec());
    assert_eq!(out, data);
    // One 64-row strip of noise is at most 64 * 256 * 5 bytes encoded.
    assert!(peak <= 64 * 256 * 5 + 14, "{peak} bytes pending");
}
