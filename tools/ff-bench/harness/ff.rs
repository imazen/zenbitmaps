//! Farbfeld encode benchmark: same-binary baseline / candidate / AA arms.
//!
//! Built by tools/ff-bench/run.py into a generated package with two immutable
//! zenbitmaps revisions (`old` = main+F6, `new` = row+F6). Every fresh
//! zenbench child process re-runs `register()`, which first executes
//! `ff_bench::preflight()` — complete old==new + spec-oracle assertions on
//! every workload cell (plus the h=0 probe and the decode control) — before
//! any benchmark callback is registered. Artifact export to
//! $FF_BENCH_ARTIFACT_DIR happens here, outside all timers.
//!
//! The whole public call — allocation and destruction included — sits inside
//! `b.iter`. No hashing, comparison, or I/O is in the timed region.

#[path = "../src/common.rs"]
mod ff_bench;

use ff_bench::{CELLS, Cell, Op, fill, preflight};
use std::hint::black_box;
use std::sync::Arc;
use zenbench::prelude::*;

fn cell(suite: &mut Suite, c: &Cell) {
    let pixels = Arc::new(fill(c.w, c.h, c.layout.bpp(), c.seed));
    let group_name = c.name.to_string();
    suite.group(group_name.clone(), |g| {
        g.throughput(Throughput::Bytes(
            (c.w as u64) * (c.h as u64) * c.layout.bpp() as u64,
        ));
        for (arm, use_new) in [("baseline", false), ("candidate", true), ("aa", false)] {
            let px = Arc::clone(&pixels);
            let (lo, ln) = (c.layout.old_layout(), c.layout.new_layout());
            let (w, h) = (c.w, c.h);
            g.bench(arm, move |b| {
                let px = Arc::clone(&px);
                b.iter(move || {
                    // Unwrap per branch: the two libraries' BitmapError are
                    // distinct types; the Vec<u8> outputs unify.
                    let out = if use_new {
                        new::encode_farbfeld(&px[..], w, h, ln, new::Unstoppable).expect("encode")
                    } else {
                        old::encode_farbfeld(&px[..], w, h, lo, old::Unstoppable).expect("encode")
                    };
                    black_box(out)
                })
            });
        }
        g.baseline("baseline");
    });
}

fn decode_control(suite: &mut Suite, c: &Cell) {
    // Unrelated-op control: decode the fixed encoded stream. All three arms
    // decode byte-identical input (asserted in preflight); oldaa repeats the
    // baseline library as the A/A arm. The full DecodeOutput — pixel storage
    // included — is forced through black_box inside the iteration, so decode
    // allocation + destruction are timed and cannot be elided.
    let pixels = fill(c.w, c.h, c.layout.bpp(), c.seed);
    let encoded = new::encode_farbfeld(&pixels, c.w, c.h, c.layout.new_layout(), new::Unstoppable)
        .expect("control input");
    let encoded = Arc::new(encoded);
    suite.group(c.name, |g| {
        g.throughput(Throughput::Bytes(encoded.len() as u64));
        for (arm, use_new) in [("olddecode", false), ("newdecode", true), ("oldaa", false)] {
            let e = Arc::clone(&encoded);
            g.bench(arm, move |b| {
                let e = Arc::clone(&e);
                b.iter(move || {
                    // Materialize the whole output (pixels included) inside
                    // the iteration; only the borrowed slice's len leaves.
                    // DecodeOutput is a distinct type per crate, so the
                    // branches are handled separately.
                    if use_new {
                        let d = black_box(
                            new::decode_farbfeld(&e[..], new::Unstoppable).expect("decode"),
                        );
                        black_box(d.pixels());
                        d.pixels().len()
                    } else {
                        let d = black_box(
                            old::decode_farbfeld(&e[..], old::Unstoppable).expect("decode"),
                        );
                        black_box(d.pixels());
                        d.pixels().len()
                    }
                })
            });
        }
        g.baseline("olddecode");
    });
}

fn register(suite: &mut Suite) {
    // Per-fresh-process parity gate: complete old==new and spec assertions
    // for every cell (and the untimed h=0 probe) BEFORE any group is
    // registered, i.e. before any timing callback can run. Exports the
    // unique inputs/outputs to $FF_BENCH_ARTIFACT_DIR when set.
    let dir = std::path::PathBuf::from(
        std::env::var_os("FF_BENCH_ARTIFACT_DIR")
            .expect("FF_BENCH_ARTIFACT_DIR is required for retained correctness artifacts"),
    );
    for line in preflight(Some(dir)) {
        eprintln!("[ff-bench preflight] {line}");
    }
    for c in CELLS.iter().filter(|c| c.timed) {
        match c.op {
            Op::Encode => cell(suite, c),
            Op::Decode => decode_control(suite, c),
        }
    }
}

zenbench::main!(register);
