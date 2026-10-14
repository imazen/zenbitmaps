//! QOI on real photos at several sizes, through every path: the one-shot
//! functions, and zencodec's sans-I/O contract (feed/step decode from a
//! slice in one piece and in 64 KiB windows, the stream pump, streaming
//! `push_rows` encode, and a decode-to-encode transcode with no image
//! buffer).
//!
//! Inputs: every `*.png` in `$ZENBITMAPS_PHOTO_DIR` (required; the
//! justfile recipe fetches imazen-26 camera photos into it). Each photo is
//! decoded with zenpng and resampled with zenresize (Mitchell-Netravali,
//! RGB8_SRGB format) to widths 64, 256, 1024 and 2048, keeping the aspect
//! ratio, plus the original size; never upscaled. One iteration processes
//! every photo at that size, so each group reports the corpus total.
//!
//! `just bench-qoi-photos`

use std::path::PathBuf;
use std::time::Duration;

use enough::Unstoppable;
use zenbench::prelude::*;
use zenbitmaps::{PixelLayout, QoiDecoderConfig, QoiEncoderConfig};
use zencodec::decode::{DecodeJob, DecoderConfig};
use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
use zencodec_io::{CollectSink, EncoderSink, drive_from_slice, pump_read_to_sink};
use zenpixels::{PixelDescriptor, PixelSlice};

#[global_allocator]
static ALLOC: zenbench::AllocProfiler = zenbench::AllocProfiler::system();

struct Photo {
    name: String,
    width: u32,
    height: u32,
    channels: usize,
    pixels: Vec<u8>,
    qoi: Vec<u8>,
}

fn layout(channels: usize) -> PixelLayout {
    if channels == 4 {
        PixelLayout::Rgba8
    } else {
        PixelLayout::Rgb8
    }
}

fn desc(channels: usize) -> PixelDescriptor {
    if channels == 4 {
        PixelDescriptor::RGBA8
    } else {
        PixelDescriptor::RGB8
    }
}

/// Decode a PNG to tightly packed RGB8/RGBA8 sRGB bytes.
fn load_png(path: &PathBuf) -> (u32, u32, usize, Vec<u8>) {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let out = zenpng::PngDecoderConfig::new()
        .decode(&data)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let ps = out.pixels();
    let d = ps.descriptor();
    let channels = d.bytes_per_pixel();
    assert!(
        channels == 3 || channels == 4,
        "{}: expected 8-bit RGB or RGBA, got {d:?}",
        path.display()
    );
    let mut bytes = Vec::with_capacity(ps.width() as usize * ps.rows() as usize * channels);
    for y in 0..ps.rows() {
        bytes.extend_from_slice(&ps.row(y)[..ps.width() as usize * channels]);
    }
    (ps.width(), ps.rows(), channels, bytes)
}

fn resample(w: u32, h: u32, channels: usize, pixels: &[u8], out_w: u32) -> (u32, u32, Vec<u8>) {
    let out_h = ((h as u64 * out_w as u64 + w as u64 / 2) / w as u64).max(1) as u32;
    let format = if channels == 4 {
        zenresize::PixelDescriptor::RGBA8_SRGB
    } else {
        zenresize::PixelDescriptor::RGB8_SRGB
    };
    let config = zenresize::ResizeConfig::builder(w, h, out_w, out_h)
        .filter(zenresize::Filter::Mitchell)
        .format(format)
        .build();
    let out = zenresize::Resizer::new(&config).resize(pixels);
    (out_w, out_h, out)
}

fn photos_at(sources: &[(String, u32, u32, usize, Vec<u8>)], width: Option<u32>) -> Vec<Photo> {
    sources
        .iter()
        .filter(|(_, w, ..)| width.is_none_or(|t| t <= *w))
        .map(|(name, w, h, ch, px)| {
            let (pw, ph, pixels) = match width {
                Some(t) if t < *w => resample(*w, *h, *ch, px, t),
                _ => (*w, *h, px.clone()),
            };
            let qoi = zenbitmaps::encode_qoi(&pixels, pw, ph, layout(*ch), Unstoppable).unwrap();
            Photo {
                name: name.clone(),
                width: pw,
                height: ph,
                channels: *ch,
                pixels,
                qoi,
            }
        })
        .collect()
}

fn decode_feed_step(p: &Photo, chunk: usize) -> CollectSink {
    let mut dec = QoiDecoderConfig::new()
        .job()
        .dyn_incremental_decoder(&[])
        .unwrap();
    let mut sink = CollectSink::new();
    drive_from_slice(
        &mut zencodec_io::Dyn(&mut *dec),
        &p.qoi,
        &mut sink,
        None,
        chunk,
    )
    .unwrap();
    sink
}

fn decode_pumped(p: &Photo) -> CollectSink {
    let mut dec = QoiDecoderConfig::new()
        .job()
        .dyn_incremental_decoder(&[])
        .unwrap();
    let mut sink = CollectSink::new();
    pump_read_to_sink(
        std::io::Cursor::new(&p.qoi),
        &mut zencodec_io::Dyn(&mut *dec),
        &mut sink,
        None,
        64 << 10,
    )
    .unwrap();
    sink
}

fn encode_streaming(p: &Photo, strip: u32) -> Vec<u8> {
    let stride = p.width as usize * p.channels;
    let mut enc = QoiEncoderConfig::new().job().encoder().unwrap();
    let mut y = 0;
    while y < p.height {
        let rows = (p.height - y).min(strip);
        let start = y as usize * stride;
        let s = PixelSlice::new(
            &p.pixels[start..start + rows as usize * stride],
            p.width,
            rows,
            stride,
            desc(p.channels),
        )
        .unwrap();
        enc.push_rows(s).unwrap();
        y += rows;
    }
    enc.finish().unwrap().into_vec()
}

fn transcode_streaming(p: &Photo, chunk: usize) -> Vec<u8> {
    let mut dec = QoiDecoderConfig::new()
        .job()
        .dyn_incremental_decoder(&[])
        .unwrap();
    let enc = QoiEncoderConfig::new().job().encoder().unwrap();
    let mut sink = EncoderSink::new(enc);
    drive_from_slice(
        &mut zencodec_io::Dyn(&mut *dec),
        &p.qoi,
        &mut sink,
        None,
        chunk,
    )
    .unwrap();
    sink.finish().unwrap().into_vec()
}

zenbench::main!(|suite| {
    let dir = std::env::var_os("ZENBITMAPS_PHOTO_DIR")
        .map(PathBuf::from)
        .expect(
            "set ZENBITMAPS_PHOTO_DIR to a directory of PNG photos (see `just bench-qoi-photos`)",
        );
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no PNGs in {}", dir.display());
    let sources: Vec<(String, u32, u32, usize, Vec<u8>)> = paths
        .iter()
        .map(|p| {
            let (w, h, ch, px) = load_png(p);
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            eprintln!("[qoi_photos] {name}: {w}x{h}x{ch}");
            (name, w, h, ch, px)
        })
        .collect();

    for width in [Some(64u32), Some(256), Some(1024), Some(2048), None] {
        let photos: &'static [Photo] = Box::leak(photos_at(&sources, width).into_boxed_slice());
        let label = width.map_or("original".to_string(), |w| format!("w{w}"));
        let pixels: u64 = photos
            .iter()
            .map(|p| p.width as u64 * p.height as u64)
            .sum();
        let raw: usize = photos.iter().map(|p| p.pixels.len()).sum();
        let qoi: usize = photos.iter().map(|p| p.qoi.len()).sum();
        let ratio = qoi as f64 / raw as f64;
        // Every path agrees before anything is timed.
        for p in photos {
            assert_eq!(
                decode_feed_step(p, usize::MAX).bytes(),
                &p.pixels[..],
                "{}",
                p.name
            );
            assert_eq!(
                decode_feed_step(p, 64 << 10).bytes(),
                &p.pixels[..],
                "{}",
                p.name
            );
            assert_eq!(decode_pumped(p).bytes(), &p.pixels[..], "{}", p.name);
            assert_eq!(encode_streaming(p, 16), p.qoi, "{}", p.name);
            assert_eq!(transcode_streaming(p, 64 << 10), p.qoi, "{}", p.name);
        }
        let budget = if width.is_some_and(|w| w <= 1024) {
            2
        } else {
            8
        };
        let tag = format!(
            "{label} ({} photos, {:.1} MP, ratio {ratio:.2})",
            photos.len(),
            pixels as f64 / 1e6
        );

        suite.group(format!("photo-decode/{tag}"), |g| {
            g.throughput(Throughput::Elements(pixels));
            g.config()
                .max_time(Duration::from_secs(budget))
                .max_rounds(40);
            g.bench("one-shot decode_qoi", move |b| {
                b.iter(|| {
                    for p in photos {
                        black_box(zenbitmaps::decode_qoi(&p.qoi, Unstoppable).unwrap());
                    }
                })
            });
            g.bench("feed/step, whole slice", move |b| {
                b.iter(|| {
                    for p in photos {
                        black_box(decode_feed_step(p, usize::MAX));
                    }
                })
            });
            g.bench("feed/step, 64 KiB windows", move |b| {
                b.iter(|| {
                    for p in photos {
                        black_box(decode_feed_step(p, 64 << 10));
                    }
                })
            });
            g.bench("feed/step, stream pump 64 KiB", move |b| {
                b.iter(|| {
                    for p in photos {
                        black_box(decode_pumped(p));
                    }
                })
            });
        });

        suite.group(format!("photo-encode/{tag}"), |g| {
            g.throughput(Throughput::Elements(pixels));
            g.config()
                .max_time(Duration::from_secs(budget))
                .max_rounds(40);
            g.bench("one-shot encode_qoi", move |b| {
                b.iter(|| {
                    for p in photos {
                        black_box(
                            zenbitmaps::encode_qoi(
                                &p.pixels,
                                p.width,
                                p.height,
                                layout(p.channels),
                                Unstoppable,
                            )
                            .unwrap(),
                        );
                    }
                })
            });
            g.bench("Encoder::encode", move |b| {
                b.iter(|| {
                    for p in photos {
                        let enc = QoiEncoderConfig::new().job().encoder().unwrap();
                        let s = PixelSlice::new(
                            &p.pixels,
                            p.width,
                            p.height,
                            p.width as usize * p.channels,
                            desc(p.channels),
                        )
                        .unwrap();
                        black_box(enc.encode(s).unwrap());
                    }
                })
            });
            g.bench("push_rows, 16-row strips", move |b| {
                b.iter(|| {
                    for p in photos {
                        black_box(encode_streaming(p, 16));
                    }
                })
            });
        });

        suite.group(format!("photo-transcode/{tag}"), |g| {
            g.throughput(Throughput::Elements(pixels));
            g.config()
                .max_time(Duration::from_secs(budget))
                .max_rounds(40);
            g.bench("one-shot decode + encode", move |b| {
                b.iter(|| {
                    for p in photos {
                        let out = zenbitmaps::decode_qoi(&p.qoi, Unstoppable).unwrap();
                        black_box(
                            zenbitmaps::encode_qoi(
                                out.pixels(),
                                p.width,
                                p.height,
                                layout(p.channels),
                                Unstoppable,
                            )
                            .unwrap(),
                        );
                    }
                })
            });
            g.bench("feed/step -> EncoderSink, 64 KiB windows", move |b| {
                b.iter(|| {
                    for p in photos {
                        black_box(transcode_streaming(p, 64 << 10));
                    }
                })
            });
        });
    }
});
