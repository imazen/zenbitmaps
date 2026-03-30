//! Benchmarks for all zenbitmaps codecs.
//!
//! Run: `cargo bench --bench codecs --all-features`

use enough::Unstoppable;
use zenbench::{Throughput, black_box};

const W: u32 = 1000;
const H: u32 = 1000;

fn make_rgb8() -> Vec<u8> {
    (0..W * H)
        .flat_map(|i| {
            [
                (i % 256) as u8,
                ((i * 3) % 256) as u8,
                ((i * 7) % 256) as u8,
            ]
        })
        .collect()
}

fn make_rgba16() -> Vec<u8> {
    (0..W * H)
        .flat_map(|i| {
            let v = (i % 65536) as u16;
            let mut p = [0u8; 8];
            p[0..2].copy_from_slice(&v.to_ne_bytes());
            p[2..4].copy_from_slice(&v.wrapping_mul(3).to_ne_bytes());
            p[4..6].copy_from_slice(&v.wrapping_mul(7).to_ne_bytes());
            p[6..8].copy_from_slice(&65535u16.to_ne_bytes());
            p
        })
        .collect()
}

fn make_rgbf32() -> Vec<u8> {
    (0..W * H)
        .flat_map(|i| {
            let v = (i % 1000) as f32 / 1000.0;
            let mut p = [0u8; 12];
            p[0..4].copy_from_slice(&v.to_le_bytes());
            p[4..8].copy_from_slice(&(v * 0.5).to_le_bytes());
            p[8..12].copy_from_slice(&(v * 0.25).to_le_bytes());
            p
        })
        .collect()
}

zenbench::main!(|suite| {
    let throughput = Throughput::Bytes(W as u64 * H as u64 * 3);

    // ── Decode comparison ────────────────────────────────────────────
    suite.compare("decode_1mpx", |g| {
        g.throughput(Throughput::Bytes(W as u64 * H as u64 * 3));

        let ppm = zenbitmaps::encode_ppm(
            &make_rgb8(),
            W,
            H,
            zenbitmaps::PixelLayout::Rgb8,
            Unstoppable,
        )
        .unwrap();
        g.bench("ppm", move |b| {
            b.iter(|| {
                let _ = black_box(zenbitmaps::decode(&ppm, Unstoppable).unwrap());
            })
        });

        let ff = zenbitmaps::encode_farbfeld(
            &make_rgba16(),
            W,
            H,
            zenbitmaps::PixelLayout::Rgba16,
            Unstoppable,
        )
        .unwrap();
        g.bench("farbfeld", move |b| {
            b.iter(|| {
                let _ = black_box(zenbitmaps::decode_farbfeld(&ff, Unstoppable).unwrap());
            })
        });

        let bmp = zenbitmaps::encode_bmp(
            &make_rgb8(),
            W,
            H,
            zenbitmaps::PixelLayout::Rgb8,
            Unstoppable,
        )
        .unwrap();
        g.bench("bmp", move |b| {
            b.iter(|| {
                let _ = black_box(zenbitmaps::decode_bmp(&bmp, Unstoppable).unwrap());
            })
        });

        let qoi = zenbitmaps::encode_qoi(
            &make_rgb8(),
            W,
            H,
            zenbitmaps::PixelLayout::Rgb8,
            Unstoppable,
        )
        .unwrap();
        g.bench("qoi", move |b| {
            b.iter(|| {
                let _ = black_box(zenbitmaps::decode_qoi(&qoi, Unstoppable).unwrap());
            })
        });

        let tga = zenbitmaps::encode_tga(
            &make_rgb8(),
            W,
            H,
            zenbitmaps::PixelLayout::Rgb8,
            Unstoppable,
        )
        .unwrap();
        g.bench("tga", move |b| {
            b.iter(|| {
                let _ = black_box(zenbitmaps::decode_tga(&tga, Unstoppable).unwrap());
            })
        });

        let hdr = zenbitmaps::encode_hdr(
            &make_rgbf32(),
            W,
            H,
            zenbitmaps::PixelLayout::RgbF32,
            Unstoppable,
        )
        .unwrap();
        g.bench("hdr", move |b| {
            b.iter(|| {
                let _ = black_box(zenbitmaps::decode_hdr(&hdr, Unstoppable).unwrap());
            })
        });
    });

    // ── Encode comparison ────────────────────────────────────────────
    suite.compare("encode_1mpx", |g| {
        g.throughput(Throughput::Bytes(W as u64 * H as u64 * 3));

        let px = make_rgb8();
        g.bench("ppm", move |b| {
            b.iter(|| {
                black_box(
                    zenbitmaps::encode_ppm(&px, W, H, zenbitmaps::PixelLayout::Rgb8, Unstoppable)
                        .unwrap(),
                )
            })
        });

        let px16 = make_rgba16();
        g.bench("farbfeld", move |b| {
            b.iter(|| {
                black_box(
                    zenbitmaps::encode_farbfeld(
                        &px16,
                        W,
                        H,
                        zenbitmaps::PixelLayout::Rgba16,
                        Unstoppable,
                    )
                    .unwrap(),
                )
            })
        });

        let px = make_rgb8();
        g.bench("bmp", move |b| {
            b.iter(|| {
                black_box(
                    zenbitmaps::encode_bmp(&px, W, H, zenbitmaps::PixelLayout::Rgb8, Unstoppable)
                        .unwrap(),
                )
            })
        });

        let px = make_rgb8();
        g.bench("qoi", move |b| {
            b.iter(|| {
                black_box(
                    zenbitmaps::encode_qoi(&px, W, H, zenbitmaps::PixelLayout::Rgb8, Unstoppable)
                        .unwrap(),
                )
            })
        });

        let px = make_rgb8();
        g.bench("tga", move |b| {
            b.iter(|| {
                black_box(
                    zenbitmaps::encode_tga(&px, W, H, zenbitmaps::PixelLayout::Rgb8, Unstoppable)
                        .unwrap(),
                )
            })
        });

        let pxf = make_rgbf32();
        g.bench("hdr", move |b| {
            b.iter(|| {
                black_box(
                    zenbitmaps::encode_hdr(
                        &pxf,
                        W,
                        H,
                        zenbitmaps::PixelLayout::RgbF32,
                        Unstoppable,
                    )
                    .unwrap(),
                )
            })
        });
    });
});
