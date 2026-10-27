//! Incremental (sans-I/O) decoding of PNM, PFM, farbfeld, HDR, BMP and TGA
//! against each format's own one-shot decoder: every file, every chunk
//! size, every driver, byte-identical pixels and descriptors; truncation
//! fails exactly where the one-shot decoder fails; and the testkit's
//! incremental contract holds.

#![cfg(all(feature = "zencodec", feature = "std"))]

use std::borrow::Cow;
use std::io::Cursor;

use enough::Unstoppable;
use zenbitmaps::PixelLayout;
use zencodec::decode::{Decode, DecodeJob, DecoderConfig, StepError};
use zencodec::source::{ReadAt, SliceReadAt, SourceError};
use zencodec_io::{
    BufferPolicy, BufferedReadAt, CollectSink, DriveError, DriveOptions, PumpError,
    drive_from_read_at, drive_from_slice, pump_read_to_sink,
};
use zenpixels::PixelDescriptor;

/// What a one-shot decode produced: packed rows and their descriptor.
#[derive(Debug, PartialEq)]
struct Pixels {
    width: u32,
    height: u32,
    descriptor: PixelDescriptor,
    bytes: Vec<u8>,
}

fn one_shot<C: DecoderConfig>(cfg: &C, data: &[u8]) -> Option<Pixels> {
    let out = cfg
        .clone()
        .job()
        .decoder(Cow::Borrowed(data), &[])
        .ok()?
        .decode()
        .ok()?;
    let s = out.pixels();
    let row = s.width() as usize * s.descriptor().bytes_per_pixel();
    let mut bytes = Vec::new();
    for y in 0..s.rows() {
        bytes.extend_from_slice(&s.row(y)[..row]);
    }
    Some(Pixels {
        width: s.width(),
        height: s.rows(),
        descriptor: s.descriptor(),
        bytes,
    })
}

fn of_sink(sink: &CollectSink) -> Pixels {
    Pixels {
        width: sink.width(),
        height: sink.height(),
        descriptor: sink.descriptor().expect("sink began"),
        bytes: sink.bytes().to_vec(),
    }
}

/// A `ReadAt` that hides `as_slice`, so drivers take the read path.
struct Opaque<'a>(SliceReadAt<'a>);

impl ReadAt for Opaque<'_> {
    type Error = SourceError;
    fn len(&mut self) -> Result<u64, SourceError> {
        self.0.len()
    }
    fn read_exact_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), SourceError> {
        self.0.read_exact_at(offset, out)
    }
    fn read_at(&mut self, offset: u64, out: &mut [u8]) -> Result<usize, SourceError> {
        self.0.read_at(offset, out)
    }
}

/// A reader that hands out at most `chunk` bytes per call.
struct Dribble<'a> {
    data: &'a [u8],
    pos: usize,
    chunk: usize,
}

impl std::io::Read for Dribble<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = (self.data.len() - self.pos).min(buf.len()).min(self.chunk);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

fn incremental<C: DecoderConfig>(cfg: &C) -> Box<zencodec::decode::DynIncrementalDecoder> {
    cfg.clone()
        .job()
        .dyn_incremental_decoder(&[])
        .expect("declares incremental")
}

/// Decode `data` every way and require the one-shot pixels each time.
fn check_file<C: DecoderConfig>(name: &str, cfg: C, data: &[u8]) {
    assert!(C::capabilities().incremental(), "{name}: not declared");
    let want = one_shot(&cfg, data).unwrap_or_else(|| panic!("{name}: one-shot failed"));

    for window in [1usize, 3, 7, 64, 4096, data.len().max(1)] {
        let mut dec = incremental(&cfg);
        let mut sink = CollectSink::new();
        let (info, _) = drive_from_slice(&mut *dec, data, &mut sink, None, window)
            .unwrap_or_else(|e| panic!("{name} slice window {window}: {e}"));
        assert_eq!(
            (info.width, info.height),
            (want.width, want.height),
            "{name}"
        );
        assert_eq!(of_sink(&sink), want, "{name} slice window {window}");
        assert!(sink.finished());
        let i = dec.info().expect("info after Done");
        assert_eq!((i.width, i.height), (want.width, want.height), "{name}");
    }

    for read_size in [1usize, 13, 64 * 1024] {
        let mut dec = incremental(&cfg);
        let mut sink = CollectSink::new();
        let mut src = Opaque(SliceReadAt::new(data));
        drive_from_read_at(
            &mut *dec,
            &mut src,
            &mut sink,
            None,
            DriveOptions::new(0, read_size),
        )
        .unwrap_or_else(|e| panic!("{name} read_at {read_size}: {e}"));
        assert_eq!(of_sink(&sink), want, "{name} read_at {read_size}");
    }

    // A stream: sequential bodies pump directly; a random-access body says
    // so, and a spooled source serves it.
    for chunk in [1usize, 5, 4096] {
        let mut dec = incremental(&cfg);
        let mut sink = CollectSink::new();
        let reader = Dribble {
            data,
            pos: 0,
            chunk,
        };
        match pump_read_to_sink(reader, &mut *dec, &mut sink, None, 512, 64 << 20) {
            Ok(_) => assert_eq!(of_sink(&sink), want, "{name} pump {chunk}"),
            Err(PumpError::NeedsRandomAccess(_)) => {
                assert_eq!(
                    dec.access_pattern(),
                    zencodec::source::AccessPattern::RandomAccess,
                    "{name}: a sequential decoder asked for random access"
                );
                let mut dec = incremental(&cfg);
                let mut sink = CollectSink::new();
                let mut src = BufferedReadAt::new(
                    Cursor::new(data),
                    BufferPolicy::Spool {
                        max_bytes: 64 << 20,
                    },
                );
                drive_from_read_at(
                    &mut *dec,
                    &mut src,
                    &mut sink,
                    None,
                    DriveOptions::default(),
                )
                .unwrap_or_else(|e| panic!("{name} spooled: {e}"));
                assert_eq!(of_sink(&sink), want, "{name} spooled");
            }
            Err(e) => panic!("{name} pump {chunk}: {e}"),
        }
    }

    match zencodec_testkit::check_incremental_capability_honesty(cfg.clone(), data) {
        Ok(()) => {}
        // The testkit requires truncation at 1/4 and 1/2 to fail. Some
        // one-shot decoders render a truncated body instead (BMP RLE leaves
        // the missing pixels zero); the incremental decoder must then do the
        // same, which check_truncation verifies at every cut.
        Err(e) if format!("{e:?}").contains("decoded without error") => {
            let accepted = [data.len() / 4, data.len() / 2]
                .iter()
                .any(|&cut| one_shot(&cfg, &data[..cut]).is_some());
            assert!(accepted, "{name}: {e:?}");
        }
        Err(e) => panic!("{name}: {e:?}"),
    }

    check_truncation(name, &cfg, data);
}

/// Every cut fails incrementally exactly when the one-shot decoder fails on
/// it, and agrees on the pixels when both succeed.
fn check_truncation<C: DecoderConfig>(name: &str, cfg: &C, data: &[u8]) {
    let cuts: Vec<usize> = if data.len() <= 600 {
        (0..data.len()).collect()
    } else {
        let mut v: Vec<usize> = (0..64).collect();
        v.extend((0..40).map(|i| data.len() * i / 40));
        v.extend(data.len() - 16..data.len());
        v
    };
    for cut in cuts {
        let part = &data[..cut];
        let oneshot = one_shot(cfg, part);
        let mut dec = incremental(cfg);
        let mut sink = CollectSink::new();
        match (
            drive_from_slice(&mut *dec, part, &mut sink, None, 37),
            oneshot,
        ) {
            (Ok(_), Some(want)) => assert_eq!(of_sink(&sink), want, "{name} cut {cut}"),
            (Err(DriveError::Step(StepError::Codec(_))), None) => {}
            (got, want) => panic!(
                "{name} cut {cut} of {}: incremental {:?}, one-shot ok={}",
                data.len(),
                got.map(|(i, _)| (i.width, i.height)),
                want.is_some()
            ),
        }
    }
}

// ── fixtures ─────────────────────────────────────────────────────────────

fn noise(n: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect()
}

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn bmp_fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/bmp-fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn pnm_header(magic: &str, w: u32, h: u32, maxval: Option<u32>) -> Vec<u8> {
    let mut v = format!("{magic}\n# a comment line\n{w} {h}\n").into_bytes();
    if let Some(m) = maxval {
        v.extend_from_slice(format!("{m}\n").as_bytes());
    }
    v
}

#[test]
fn pnm_every_variant() {
    let cfg = zenbitmaps::PnmDecoderConfig::new();
    // Binary 8-bit, maxval 255 (borrowed path).
    for (w, h) in [(1u32, 1u32), (13, 7), (64, 33)] {
        let gray = noise((w * h) as usize, w);
        let rgb = noise((w * h * 3) as usize, h);
        let rgba = noise((w * h * 4) as usize, w + h);
        let pgm = zenbitmaps::encode_pgm(&gray, w, h, PixelLayout::Gray8, Unstoppable).unwrap();
        let ppm = zenbitmaps::encode_ppm(&rgb, w, h, PixelLayout::Rgb8, Unstoppable).unwrap();
        let pam = zenbitmaps::encode_pam(&rgba, w, h, PixelLayout::Rgba8, Unstoppable).unwrap();
        check_file(&format!("P5 {w}x{h}"), cfg.clone(), &pgm);
        check_file(&format!("P6 {w}x{h}"), cfg.clone(), &ppm);
        check_file(&format!("P7 {w}x{h}"), cfg.clone(), &pam);
    }
    // Binary with a transform: maxval 100 (8-bit), maxval 1000 and 65535 (16-bit).
    let (w, h) = (11u32, 6u32);
    let mut p5_100 = pnm_header("P5", w, h, Some(100));
    p5_100.extend(noise((w * h) as usize, 3).iter().map(|v| v % 101));
    check_file("P5 maxval 100", cfg.clone(), &p5_100);
    let mut p5_1000 = pnm_header("P5", w, h, Some(1000));
    for v in noise((w * h * 2) as usize, 4).chunks(2) {
        let s = u16::from_be_bytes([v[0], v[1]]) % 1001;
        p5_1000.extend_from_slice(&s.to_be_bytes());
    }
    check_file("P5 maxval 1000", cfg.clone(), &p5_1000);
    let mut p6_16 = pnm_header("P6", w, h, Some(65535));
    p6_16.extend(noise((w * h * 6) as usize, 5));
    check_file("P6 16-bit", cfg.clone(), &p6_16);
    let mut pam16 =
        format!("P7\nWIDTH {w}\nHEIGHT {h}\nDEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\nENDHDR\n")
            .into_bytes();
    pam16.extend(noise((w * h * 2) as usize, 6));
    check_file("P7 gray16", cfg.clone(), &pam16);
    // Bit-packed P4, a width that is not a multiple of 8.
    let mut p4 = pnm_header("P4", 13, 9, None);
    p4.extend(noise(2 * 9, 7));
    check_file("P4 13x9", cfg.clone(), &p4);
    // PFM is stored bottom-up: decoded from the end with range requests.
    for (layout, ch) in [(PixelLayout::RgbF32, 3usize), (PixelLayout::GrayF32, 1)] {
        let floats: Vec<u8> = noise(17 * 9 * ch, 8)
            .iter()
            .flat_map(|&v| (v as f32 / 255.0).to_ne_bytes())
            .collect();
        let pfm = zenbitmaps::encode_pfm(&floats, 17, 9, layout, Unstoppable).unwrap();
        check_file(&format!("PFM {layout:?}"), cfg.clone(), &pfm);
    }
    // ASCII variants: whole-file path.
    for f in [
        "p1_ascii.pbm",
        "p2_ascii.pgm",
        "p3_ascii.ppm",
        "p4_binary.pbm",
        "p5_binary.pgm",
        "p6_binary.ppm",
    ] {
        check_file(f, cfg.clone(), &fixture(f));
    }
}

#[test]
fn farbfeld_files() {
    let cfg = zenbitmaps::FarbfeldDecoderConfig::new();
    for (w, h) in [(1u32, 1u32), (9, 5), (70, 40)] {
        let mut f = b"farbfeld".to_vec();
        f.extend_from_slice(&w.to_be_bytes());
        f.extend_from_slice(&h.to_be_bytes());
        f.extend(noise((w * h * 8) as usize, w * 31 + h));
        check_file(&format!("farbfeld {w}x{h}"), cfg.clone(), &f);
    }
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_flat_and_rle() {
    let cfg = zenbitmaps::HdrDecoderConfig::new();
    // Width 5 is written flat; width 37 and 300 use new-style RLE.
    for (w, h) in [(5u32, 4u32), (37, 9), (300, 3)] {
        let floats: Vec<u8> = noise((w * h * 3) as usize, w)
            .iter()
            .enumerate()
            .flat_map(|(i, &v)| {
                // Runs and literals both: flat bands every few pixels.
                let v = if (i / 3) % 7 < 3 { 128 } else { v };
                (v as f32 / 64.0).to_ne_bytes()
            })
            .collect();
        let hdr = zenbitmaps::encode_hdr(&floats, w, h, PixelLayout::RgbF32, Unstoppable).unwrap();
        check_file(&format!("HDR {w}x{h}"), cfg.clone(), &hdr);
    }
    check_file("float.hdr", cfg, &fixture("float.hdr"));
}

#[cfg(feature = "bmp")]
#[test]
fn bmp_files() {
    let cfg = zenbitmaps::BmpDecoderConfig::new();
    for f in [
        "pal1.bmp",
        "pal4.bmp",
        "pal4rle.bmp",
        "pal8.bmp",
        "pal8os2.bmp",
        "pal8rle.bmp",
        "pal8topdown.bmp",
        "rgb16-565.bmp",
        "rgb24.bmp",
        "rgb32.bmp",
        "rgba32abf.bmp",
    ] {
        check_file(f, cfg.clone(), &bmp_fixture(f));
    }
    let rgb = noise(23 * 11 * 3, 9);
    let bmp = zenbitmaps::encode_bmp(&rgb, 23, 11, PixelLayout::Rgb8, Unstoppable).unwrap();
    check_file("encoded rgb24", cfg, &bmp);
}

#[cfg(feature = "tga")]
#[test]
fn tga_files() {
    let cfg = zenbitmaps::TgaDecoderConfig::new();
    for f in ["gray8.tga", "rgb24.tga"] {
        check_file(f, cfg.clone(), &fixture(f));
    }
    let rgba = noise(19 * 7 * 4, 10);
    let tga = zenbitmaps::encode_tga(&rgba, 19, 7, PixelLayout::Rgba8, Unstoppable).unwrap();
    check_file("encoded rgba", cfg, &tga);
}

#[test]
fn input_limit_matches_one_shot() {
    let gray = noise(64 * 64, 11);
    let pgm = zenbitmaps::encode_pgm(&gray, 64, 64, PixelLayout::Gray8, Unstoppable).unwrap();
    let limits = zencodec::ResourceLimits::none().with_max_input_bytes(100);
    let cfg = zenbitmaps::PnmDecoderConfig::new();
    assert!(
        cfg.clone()
            .job()
            .with_limits(limits)
            .decoder(Cow::Borrowed(&pgm), &[])
            .is_err()
    );
    let mut dec = cfg
        .job()
        .with_limits(limits)
        .dyn_incremental_decoder(&[])
        .unwrap();
    let mut sink = CollectSink::new();
    let err = drive_from_slice(&mut *dec, &pgm, &mut sink, None, 64).unwrap_err();
    assert!(
        matches!(err, DriveError::Step(StepError::Codec(_))),
        "{err:?}"
    );
}

// ── streaming encode ─────────────────────────────────────────────────────

use zencodec::encode::{EncodeJob, Encoder, EncoderConfig, OutputTiming};
use zenpixels::PixelSlice;

/// Push `pixels` in strips, draining `pending_output` after each when
/// `declare` (`begin` called, and its answer must be `timing`). Returns the
/// file and the most bytes ever pending.
fn stream_encode<C: EncoderConfig>(
    cfg: &C,
    pixels: &[u8],
    w: u32,
    h: u32,
    desc: PixelDescriptor,
    strip: u32,
    declare: Option<OutputTiming>,
) -> (Vec<u8>, usize)
where
    <C::Job as EncodeJob>::Enc: Encoder,
{
    let stride = w as usize * desc.bytes_per_pixel();
    let mut enc = cfg.clone().job().encoder().unwrap();
    if let Some(timing) = declare {
        assert_eq!(enc.begin(w, h, desc).unwrap(), timing);
    }
    let mut out = Vec::new();
    let mut peak = 0;
    let mut y = 0;
    while y < h {
        let n = strip.min(h - y);
        let s = PixelSlice::new(
            &pixels[y as usize * stride..(y + n) as usize * stride],
            w,
            n,
            stride,
            desc,
        )
        .unwrap();
        enc.push_rows(s).unwrap();
        let pending = enc.pending_output();
        peak = peak.max(pending.len());
        out.extend_from_slice(pending);
        let n_out = pending.len();
        enc.consume_output(n_out);
        y += n;
    }
    out.extend_from_slice(&enc.finish().unwrap().into_vec());
    (out, peak)
}

fn check_stream<C: EncoderConfig>(
    name: &str,
    cfg: C,
    pixels: &[u8],
    w: u32,
    h: u32,
    desc: PixelDescriptor,
    expected: &[u8],
) where
    <C::Job as EncodeJob>::Enc: Encoder,
{
    assert!(C::capabilities().streaming_output(), "{name}");
    for strip in [1u32, 3, h] {
        let streams = Some(OutputTiming::streaming(0));
        let (out, peak) = stream_encode(&cfg, pixels, w, h, desc, strip, streams);
        assert_eq!(out, expected, "{name} strip {strip} with begin");
        let row_out = expected.len() / h as usize + 1;
        assert!(
            peak <= 256 + strip as usize * row_out * 2,
            "{name} strip {strip}: {peak} bytes pending"
        );
        let (out, peak) = stream_encode(&cfg, pixels, w, h, desc, strip, None);
        assert_eq!(out, expected, "{name} strip {strip} without begin");
        assert_eq!(peak, 0, "{name}: nothing streams without begin");
    }
    // Strided strips (rows padded apart) are written a row at a time, with
    // no gathering copy; the bytes are the same.
    let row_bytes = w as usize * desc.bytes_per_pixel();
    // Three pixels of padding: a stride must hold whole pixels.
    let padded_stride = row_bytes + 3 * desc.bytes_per_pixel();
    let mut padded = vec![0xEEu8; padded_stride * h as usize];
    for y in 0..h as usize {
        padded[y * padded_stride..][..row_bytes]
            .copy_from_slice(&pixels[y * row_bytes..][..row_bytes]);
    }
    for strip in [1u32, 3, h] {
        let mut enc = cfg.clone().job().encoder().unwrap();
        enc.begin(w, h, desc).unwrap();
        let mut out = Vec::new();
        let mut y = 0;
        loop {
            out.extend_from_slice(enc.pending_output());
            let n = enc.pending_output().len();
            enc.consume_output(n);
            if y == h {
                break;
            }
            let n = strip.min(h - y);
            let s = PixelSlice::new(
                &padded[y as usize * padded_stride..],
                w,
                n,
                padded_stride,
                desc,
            )
            .unwrap();
            enc.push_rows(s).unwrap();
            y += n;
        }
        out.extend_from_slice(enc.finish().unwrap().data());
        assert_eq!(out, expected, "{name} strided strips of {strip}");
    }
    // Declared height is enforced.
    let stride = w as usize * desc.bytes_per_pixel();
    let mut enc = cfg.clone().job().encoder().unwrap();
    enc.begin(w, h - 1, desc).unwrap();
    let all = PixelSlice::new(pixels, w, h, stride, desc).unwrap();
    assert!(
        enc.push_rows(all).is_err(),
        "{name}: more rows than declared"
    );
}

#[test]
fn pnm_streams_byte_identical() {
    let (w, h) = (23u32, 9u32);
    let cfg = zenbitmaps::PnmEncoderConfig::new();
    let rgb = noise((w * h * 3) as usize, 1);
    let ppm = zenbitmaps::encode_ppm(&rgb, w, h, PixelLayout::Rgb8, Unstoppable).unwrap();
    check_stream(
        "P6",
        cfg.clone(),
        &rgb,
        w,
        h,
        PixelDescriptor::RGB8_SRGB,
        &ppm,
    );
    let rgba = noise((w * h * 4) as usize, 2);
    let pam = zenbitmaps::encode_pam(&rgba, w, h, PixelLayout::Rgba8, Unstoppable).unwrap();
    check_stream(
        "P7",
        cfg.clone(),
        &rgba,
        w,
        h,
        PixelDescriptor::RGBA8_SRGB,
        &pam,
    );
    let gray = noise((w * h) as usize, 3);
    let pgm = zenbitmaps::encode_pgm(&gray, w, h, PixelLayout::Gray8, Unstoppable).unwrap();
    check_stream(
        "P5",
        cfg.clone(),
        &gray,
        w,
        h,
        PixelDescriptor::GRAY8_SRGB,
        &pgm,
    );
    // PFM is bottom-up: rows wait for finish and match `encode`.
    let floats: Vec<u8> = noise((w * h * 3) as usize, 4)
        .iter()
        .flat_map(|&v| (v as f32 / 255.0).to_ne_bytes())
        .collect();
    let desc = PixelDescriptor::RGBF32_LINEAR;
    let one = cfg
        .clone()
        .job()
        .encoder()
        .unwrap()
        .encode(PixelSlice::new(&floats, w, h, w as usize * 12, desc).unwrap())
        .unwrap()
        .into_vec();
    let at_finish = Some(OutputTiming::AT_FINISH);
    let (out, peak) = stream_encode(&cfg, &floats, w, h, desc, 4, at_finish);
    assert_eq!(out, one);
    assert_eq!(peak, 0);
    zencodec_testkit::check_streaming_output_honesty(
        cfg,
        zenbitmaps::PnmDecoderConfig::new(),
        &zencodec_testkit::TestImage::rgba8_gradient(40, 24),
    )
    .unwrap();
}

#[test]
fn farbfeld_streams_byte_identical() {
    let (w, h) = (17u32, 6u32);
    let cfg = zenbitmaps::FarbfeldEncoderConfig::new();
    let rgba = noise((w * h * 4) as usize, 5);
    let ff = zenbitmaps::encode_farbfeld(&rgba, w, h, PixelLayout::Rgba8, Unstoppable).unwrap();
    check_stream(
        "farbfeld",
        cfg.clone(),
        &rgba,
        w,
        h,
        PixelDescriptor::RGBA8_SRGB,
        &ff,
    );
    zencodec_testkit::check_streaming_output_honesty(
        cfg,
        zenbitmaps::FarbfeldDecoderConfig::new(),
        &zencodec_testkit::TestImage::rgba8_gradient(40, 24),
    )
    .unwrap();
}

#[cfg(feature = "hdr")]
#[test]
fn hdr_streams_byte_identical() {
    let cfg = zenbitmaps::HdrEncoderConfig::new();
    for (w, h) in [(5u32, 4u32), (37, 9)] {
        let floats: Vec<u8> = noise((w * h * 3) as usize, w)
            .iter()
            .flat_map(|&v| (v as f32 / 64.0).to_ne_bytes())
            .collect();
        let hdr = zenbitmaps::encode_hdr(&floats, w, h, PixelLayout::RgbF32, Unstoppable).unwrap();
        check_stream(
            &format!("HDR {w}x{h}"),
            cfg.clone(),
            &floats,
            w,
            h,
            PixelDescriptor::RGBF32_LINEAR,
            &hdr,
        );
    }
    zencodec_testkit::check_streaming_output_honesty(
        cfg,
        zenbitmaps::HdrDecoderConfig::new(),
        &zencodec_testkit::TestImage::rgb8_gradient(40, 24),
    )
    .unwrap();
}

/// TGA stores rows bottom-up: `begin` reports `AT_FINISH`, nothing is
/// pending before `finish`, and the file equals the one-shot encode. BMP
/// has no row-level encode; its `begin` keeps the trait default.
#[cfg(all(feature = "bmp", feature = "tga"))]
#[test]
fn bottom_up_formats_report_at_finish() {
    let (w, h) = (13u32, 7u32);
    let rgba = noise((w * h * 4) as usize, 9);
    let desc = PixelDescriptor::RGBA8_SRGB;
    let tga = zenbitmaps::TgaEncoderConfig::new();
    let at_finish = Some(OutputTiming::AT_FINISH);
    let (out, peak) = stream_encode(&tga, &rgba, w, h, desc, 3, at_finish);
    assert_eq!(peak, 0);
    let s = PixelSlice::new(&rgba, w, h, w as usize * 4, desc).unwrap();
    let one = tga.clone().job().encoder().unwrap().encode(s).unwrap();
    assert_eq!(out, one.into_vec());
    let mut bmp = zenbitmaps::BmpEncoderConfig::new().job().encoder().unwrap();
    assert_eq!(bmp.begin(w, h, desc).unwrap(), OutputTiming::AT_FINISH);
    assert!(bmp.pending_output().is_empty());
}

/// BMP rows accumulate and `finish` runs the one-shot encode on them: the
/// same bytes for strided strips, including the dead-alpha narrowing the
/// one-shot path applies. A declaration the rows then contradict refuses.
#[cfg(feature = "bmp")]
#[test]
fn bmp_rows_match_one_shot_and_hold_to_the_declaration() {
    let (w, h) = (13u32, 7u32);
    let row = w as usize * 4;
    // Two pixels of padding: PixelSlice strides are pixel-aligned.
    let stride = row + 8;
    let mut opaque = noise(stride * h as usize, 21);
    for y in 0..h as usize {
        for x in 0..w as usize {
            opaque[y * stride + x * 4 + 3] = 255;
        }
    }
    for (what, data) in [
        ("noise", noise(stride * h as usize, 22)),
        ("opaque", opaque),
    ] {
        let desc = PixelDescriptor::RGBA8_SRGB;
        let all = PixelSlice::new(&data, w, h, stride, desc).unwrap();
        let cfg = zenbitmaps::BmpEncoderConfig::new();
        let one = cfg.clone().job().encoder().unwrap().encode(all).unwrap();
        for declare in [false, true] {
            for strip in [1u32, 3, 7] {
                let mut enc = cfg.clone().job().encoder().unwrap();
                if declare {
                    assert_eq!(enc.begin(w, h, desc).unwrap(), OutputTiming::AT_FINISH);
                }
                let mut y = 0;
                while y < h {
                    let n = strip.min(h - y);
                    let rows =
                        PixelSlice::new(&data[y as usize * stride..], w, n, stride, desc).unwrap();
                    enc.push_rows(rows).unwrap();
                    assert!(enc.pending_output().is_empty());
                    y += n;
                }
                let out = enc.finish().unwrap();
                assert_eq!(
                    out.data(),
                    one.data(),
                    "{what} declare={declare} strip={strip}"
                );
            }
        }
    }

    let data = noise(row * h as usize, 23);
    let desc = PixelDescriptor::RGBA8_SRGB;
    let rows = |n: u32| PixelSlice::new(&data, w, n, row, desc).unwrap();
    let encoder = || zenbitmaps::BmpEncoderConfig::new().job().encoder().unwrap();
    let mut twice = encoder();
    twice.begin(w, h, desc).unwrap();
    assert!(twice.begin(w, h, desc).is_err(), "begin twice");
    let mut late = encoder();
    late.push_rows(rows(1)).unwrap();
    assert!(late.begin(w, h, desc).is_err(), "begin after rows");
    let mut wide = encoder();
    wide.begin(w + 1, h, desc).unwrap();
    assert!(wide.push_rows(rows(1)).is_err(), "width differs from begin");
    let mut long = encoder();
    long.begin(w, 2, desc).unwrap();
    assert!(long.push_rows(rows(3)).is_err(), "more rows than declared");
    let mut short = encoder();
    short.begin(w, h, desc).unwrap();
    short.push_rows(rows(2)).unwrap();
    assert!(short.finish().is_err(), "fewer rows than declared");
    let gray = PixelDescriptor::GRAY8_SRGB;
    assert!(encoder().begin(w, h, gray).is_err(), "BMP stores no gray");
    assert!(encoder().finish().is_err(), "finish without rows");
}

/// `max_output_bytes` bounds the whole file on every path, streamed bytes
/// already handed out included, and the usage paths stay exclusive.
#[test]
fn output_limit_and_exclusive_paths() {
    let (w, h) = (64u32, 64u32);
    let rgb = noise((w * h * 3) as usize, 7);
    let cfg = zenbitmaps::PnmEncoderConfig::new();
    let total = zenbitmaps::encode_ppm(&rgb, w, h, PixelLayout::Rgb8, Unstoppable)
        .unwrap()
        .len() as u64;
    let limited = |max: u64| {
        cfg.clone()
            .job()
            .with_limits(zencodec::ResourceLimits::none().with_max_output(max))
            .encoder()
            .unwrap()
    };
    let stream = |max: u64| -> Result<(), Box<dyn std::error::Error>> {
        let mut enc = limited(max);
        enc.begin(w, h, PixelDescriptor::RGB8_SRGB)?;
        for y in 0..h as usize {
            let n = enc.pending_output().len();
            enc.consume_output(n);
            let s = PixelSlice::new(
                &rgb[y * 192..(y + 1) * 192],
                w,
                1,
                192,
                PixelDescriptor::RGB8_SRGB,
            )?;
            enc.push_rows(s)?;
        }
        enc.finish()?;
        Ok(())
    };
    assert!(stream(total).is_ok());
    assert!(
        stream(total - 1).is_err(),
        "streamed bytes count toward the limit"
    );
    let whole = PixelSlice::new(&rgb, w, h, 192, PixelDescriptor::RGB8_SRGB).unwrap();
    assert!(limited(total).encode(whole.clone()).is_ok());
    assert!(
        limited(total - 1).encode(whole.clone()).is_err(),
        "one-shot too"
    );
    // encode after begin, and a second begin, are refused.
    let mut enc = cfg.clone().job().encoder().unwrap();
    enc.begin(w, h, PixelDescriptor::RGB8_SRGB).unwrap();
    assert!(enc.begin(w, h, PixelDescriptor::RGB8_SRGB).is_err());
    let mut enc = cfg.job().encoder().unwrap();
    enc.begin(w, h, PixelDescriptor::RGB8_SRGB).unwrap();
    assert!(enc.encode(whole).is_err());
}
