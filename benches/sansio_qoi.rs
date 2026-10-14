//! QOI through the sans-I/O contract versus the one-shot paths.
//!
//! Decode: `decode_qoi` (one-shot) against the feed/step decoder driven
//! from a slice in one piece, in 64 KiB chunks, and through the stream
//! pump. Encode: `encode_qoi` against `Encoder::encode` and streaming
//! `push_rows` in 16-row and 1-row strips. Transcode: one-shot decode plus
//! one-shot encode against feed/step decode straight into the streaming
//! encoder through `EncoderSink` (no intermediate image).
//!
//! `cargo bench --bench sansio_qoi --features all,zencodec,std`
//!
//! Every group is capped at 2 s and 40 rounds, so the whole grid (27
//! groups, up to 4096x4096) finishes in a few minutes. Narrow it with
//! `-- --group=NAME`.

use enough::Unstoppable;
use zenbench::prelude::*;
use zenbitmaps::{PixelLayout, QoiDecoderConfig, QoiEncoderConfig};
use zencodec::decode::{DecodeJob, DecoderConfig};
use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
use zencodec_io::{CollectSink, EncoderSink, drive_from_slice, pump_read_to_sink};
use zenpixels::{PixelDescriptor, PixelSlice};

#[global_allocator]
static ALLOC: zenbench::AllocProfiler = zenbench::AllocProfiler::system();

fn image(kind: u32, side: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(side as usize * side as usize * 4);
    let mut lcg = 0x1234_5678u32 ^ kind;
    for y in 0..side {
        for x in 0..side {
            lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let px: [u8; 4] = match kind {
                // The codecs.rs pattern: mixed diffs and short runs.
                0 => [(x ^ y) as u8, (x * 3) as u8, (y * 7) as u8, 255],
                // Noise: almost every pixel is a full RGBA opcode.
                1 => [(lcg >> 24) as u8, (lcg >> 16) as u8, (lcg >> 8) as u8, 255],
                // Smooth gradient with light noise: photo-like, mostly LUMA/DIFF ops.
                _ => {
                    let n = (lcg >> 29) as u8;
                    [(x / 4) as u8 ^ n, (y / 4) as u8, ((x + y) / 8) as u8, 255]
                }
            };
            v.extend_from_slice(&px);
        }
    }
    v
}

fn decode_incremental(data: &[u8], chunk: usize) -> CollectSink {
    let mut dec = QoiDecoderConfig::new()
        .job()
        .dyn_incremental_decoder(&[])
        .unwrap();
    let mut sink = CollectSink::new();
    drive_from_slice(
        &mut zencodec_io::Dyn(&mut *dec),
        data,
        &mut sink,
        None,
        chunk,
    )
    .unwrap();
    sink
}

fn decode_pumped(data: &[u8], buffer: usize) -> CollectSink {
    let mut dec = QoiDecoderConfig::new()
        .job()
        .dyn_incremental_decoder(&[])
        .unwrap();
    let mut sink = CollectSink::new();
    pump_read_to_sink(
        std::io::Cursor::new(data),
        &mut zencodec_io::Dyn(&mut *dec),
        &mut sink,
        None,
        buffer,
    )
    .unwrap();
    sink
}

fn encode_streaming(pixels: &[u8], side: u32, strip: u32) -> Vec<u8> {
    let stride = side as usize * 4;
    let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
    let mut y = 0;
    while y < side {
        let rows = (side - y).min(strip);
        let start = y as usize * stride;
        let s = PixelSlice::new(
            &pixels[start..start + rows as usize * stride],
            side,
            rows,
            stride,
            PixelDescriptor::RGBA8,
        )
        .unwrap();
        enc.push_rows(s).unwrap();
        y += rows;
    }
    enc.finish().unwrap().into_vec()
}

fn transcode_streaming(data: &[u8], chunk: usize) -> Vec<u8> {
    let mut dec = QoiDecoderConfig::new()
        .job()
        .dyn_incremental_decoder(&[])
        .unwrap();
    let enc = QoiEncoderConfig::new().job().encoder().unwrap();
    let mut sink = EncoderSink::new(enc);
    drive_from_slice(
        &mut zencodec_io::Dyn(&mut *dec),
        data,
        &mut sink,
        None,
        chunk,
    )
    .unwrap();
    sink.finish().unwrap().into_vec()
}

zenbench::main!(|suite| {
    for (kind, name) in [(0u32, "pattern"), (1, "noise"), (2, "photoish")] {
        for side in [256u32, 1024, 4096] {
            let pixels: &'static [u8] = Box::leak(image(kind, side).into_boxed_slice());
            let count = side as u64 * side as u64;
            let encoded: &'static [u8] = Box::leak(
                zenbitmaps::encode_qoi(pixels, side, side, PixelLayout::Rgba8, Unstoppable)
                    .unwrap()
                    .into_boxed_slice(),
            );
            // Every path must agree before it is timed.
            assert_eq!(decode_incremental(encoded, 64 << 10).bytes(), pixels);
            assert_eq!(decode_pumped(encoded, 64 << 10).bytes(), pixels);
            assert_eq!(encode_streaming(pixels, side, 16), encoded);
            assert_eq!(transcode_streaming(encoded, 64 << 10), encoded);
            let ratio = encoded.len() as f64 / pixels.len() as f64;

            suite.group(
                format!("qoi-decode/{name}/{side} (ratio {ratio:.2})"),
                |g| {
                    g.throughput(Throughput::Elements(count));
                    g.config()
                        .max_time(std::time::Duration::from_secs(2))
                        .max_rounds(40);
                    g.bench("one-shot decode_qoi", move |b| {
                        b.iter(|| black_box(zenbitmaps::decode_qoi(encoded, Unstoppable).unwrap()))
                    });
                    g.bench("feed/step, whole slice", move |b| {
                        b.iter(|| black_box(decode_incremental(encoded, usize::MAX)))
                    });
                    g.bench("feed/step, 64 KiB chunks", move |b| {
                        b.iter(|| black_box(decode_incremental(encoded, 64 << 10)))
                    });
                    g.bench("feed/step, stream pump 64 KiB", move |b| {
                        b.iter(|| black_box(decode_pumped(encoded, 64 << 10)))
                    });
                },
            );

            suite.group(format!("qoi-encode/{name}/{side}"), |g| {
                g.throughput(Throughput::Elements(count));
                g.config()
                    .max_time(std::time::Duration::from_secs(2))
                    .max_rounds(40);
                g.bench("one-shot encode_qoi", move |b| {
                    b.iter(|| {
                        black_box(
                            zenbitmaps::encode_qoi(
                                pixels,
                                side,
                                side,
                                PixelLayout::Rgba8,
                                Unstoppable,
                            )
                            .unwrap(),
                        )
                    })
                });
                g.bench("Encoder::encode", move |b| {
                    b.iter(|| {
                        let enc = QoiEncoderConfig::new().job().encoder().unwrap();
                        let s = PixelSlice::new(
                            pixels,
                            side,
                            side,
                            side as usize * 4,
                            PixelDescriptor::RGBA8,
                        )
                        .unwrap();
                        black_box(enc.encode(s).unwrap())
                    })
                });
                g.bench("push_rows, 16-row strips", move |b| {
                    b.iter(|| black_box(encode_streaming(pixels, side, 16)))
                });
                g.bench("push_rows, 1-row strips", move |b| {
                    b.iter(|| black_box(encode_streaming(pixels, side, 1)))
                });
            });

            suite.group(format!("qoi-transcode/{name}/{side}"), |g| {
                g.throughput(Throughput::Elements(count));
                g.config()
                    .max_time(std::time::Duration::from_secs(2))
                    .max_rounds(40);
                g.bench("one-shot decode + encode", move |b| {
                    b.iter(|| {
                        let out = zenbitmaps::decode_qoi(encoded, Unstoppable).unwrap();
                        black_box(
                            zenbitmaps::encode_qoi(
                                out.pixels(),
                                side,
                                side,
                                PixelLayout::Rgba8,
                                Unstoppable,
                            )
                            .unwrap(),
                        )
                    })
                });
                g.bench("feed/step -> EncoderSink, whole slice", move |b| {
                    b.iter(|| black_box(transcode_streaming(encoded, usize::MAX)))
                });
                g.bench("feed/step -> EncoderSink, 64 KiB chunks", move |b| {
                    b.iter(|| black_box(transcode_streaming(encoded, 64 << 10)))
                });
            });
        }
    }
});
