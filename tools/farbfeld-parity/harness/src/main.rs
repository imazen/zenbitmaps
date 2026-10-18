//! Farbfeld Rgba8 encode parity: a git-archived baseline (`old`) against a
//! candidate revision (`new`), and both against an oracle written from the
//! farbfeld format definition. `run.py` generates the crate layout and runs
//! one group per invocation: `core`, `mutants`, `f6`, `write-oracle <dir>`.

#![forbid(unsafe_code)]

use enough::{Stop, StopReason, Unstoppable};
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting {
    calls: AtomicUsize,
    cancel_at: usize,
}

impl Counting {
    fn new(cancel_at: usize) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            cancel_at,
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl Stop for Counting {
    fn check(&self) -> Result<(), StopReason> {
        let i = self.calls.fetch_add(1, Ordering::Relaxed);
        if i >= self.cancel_at {
            Err(StopReason::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// (bytes, capacity), or the error's Debug with source roots normalized.
type Outcome = Result<(Vec<u8>, usize), String>;

/// The two archived source roots, from the generated layout
/// `<scratch>/{harness,old,new}`.
fn source_roots() -> [String; 2] {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("harness lives inside the scratch directory");
    ["old", "new"].map(|d| format!("{}/", scratch.join(d).display()))
}

/// Replaces only the archived source roots, so error kinds, messages and
/// `at!` file:line locations are still compared exactly.
fn norm(s: String) -> String {
    let [old, new] = source_roots();
    s.replace(&old, "<SRC>/").replace(&new, "<SRC>/")
}

fn run_old(px: &[u8], w: u32, h: u32, stop: &Counting) -> Outcome {
    old::encode_farbfeld(px, w, h, old::PixelLayout::Rgba8, stop)
        .map(|v| {
            let c = v.capacity();
            (v, c)
        })
        .map_err(|e| norm(format!("{e:?}")))
}

fn run_new(px: &[u8], w: u32, h: u32, stop: &Counting) -> Outcome {
    new::encode_farbfeld(px, w, h, new::PixelLayout::Rgba8, stop)
        .map(|v| {
            let c = v.capacity();
            (v, c)
        })
        .map_err(|e| norm(format!("{e:?}")))
}

/// Farbfeld as specified: magic `farbfeld`, width and height as big-endian
/// u32, then RGBA with each sample a big-endian u16. An 8-bit sample `v`
/// widens to `v * 257` (the byte repeated), which maps 0 to 0 and 255 to 65535.
/// Written from the format definition; it calls nothing in zenbitmaps.
fn spec(px: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut v = b"farbfeld".to_vec();
    v.extend(w.to_be_bytes());
    v.extend(h.to_be_bytes());
    for &b in &px[..w as usize * h as usize * 4] {
        v.extend((u16::from(b) * 257).to_be_bytes());
    }
    v
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

const SEED: u64 = 0x9e37_79b9_7f4a_7c15;

/// Every channel takes all 256 values over 256 pixels.
fn all_values() -> Vec<u8> {
    (0..=255u8)
        .flat_map(|v| [v, 255 - v, v.rotate_left(1), v ^ 0x5a])
        .collect()
}

fn core() {
    let mut rng = Rng(SEED);

    // Valid inputs: old == new == spec, equal capacity, equal check count.
    let widths = [
        1u32, 2, 3, 4, 5, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 255, 256, 257, 1000, 1023,
        4096,
    ];
    let heights = [0u32, 1, 2, 15, 16, 17, 31, 32, 33, 47, 48, 49, 100];
    let mut cases = 0usize;
    for &w in &widths {
        for &h in &heights {
            for tail in [0usize, 1, 3, 4, 7, 13, 17] {
                let px = rng.bytes(w as usize * h as usize * 4 + tail);
                let so = Counting::new(usize::MAX);
                let sn = Counting::new(usize::MAX);
                let o = run_old(&px, w, h, &so);
                let nw = run_new(&px, w, h, &sn);
                assert_eq!(o, nw, "old/new differ at {w}x{h} tail {tail}");
                let (bytes, cap) = nw.unwrap();
                assert_eq!(
                    bytes,
                    spec(&px, w, h),
                    "spec mismatch at {w}x{h} tail {tail}"
                );
                // No slack and no reallocation: the encoder sizes its output once.
                assert_eq!(cap, bytes.len(), "capacity != len at {w}x{h}");
                assert_eq!(so.calls(), sn.calls(), "check count differs at {w}x{h}");
                // Pins the baseline's poll schedule (one check before the
                // pixel loop, then every 16th row), which the candidate must keep.
                assert_eq!(
                    sn.calls(),
                    1 + (h as usize).div_ceil(16),
                    "check schedule at {w}x{h}"
                );
                cases += 1;
            }
        }
    }
    println!("valid random cases: {cases} (old == new == spec, capacity == len, check counts)");

    let all = all_values();
    for (w, h) in [(256u32, 1u32), (1, 256), (16, 16), (128, 2)] {
        let so = Counting::new(usize::MAX);
        let sn = Counting::new(usize::MAX);
        let o = run_old(&all, w, h, &so);
        let nw = run_new(&all, w, h, &sn);
        assert_eq!(o, nw, "all-values {w}x{h}");
        assert_eq!(nw.unwrap().0, spec(&all, w, h), "all-values spec {w}x{h}");
    }
    println!("all-256-values cases: 4 (old == new == spec)");

    // Invalid inputs: identical errors, `at!` locations included.
    let invalid: [(u32, u32, usize); 10] = [
        (0, 0, 0),
        (0, 1, 64),
        (0, 100, 0),
        (1, 1, 3),
        (3, 17, 3 * 17 * 4 - 1),
        (4096, 100, 0),
        (u32::MAX, u32::MAX, 16),
        (u32::MAX, 2, 16),
        (2, u32::MAX, 16),
        (1 << 30, 1 << 30, 16),
    ];
    let mut sample = None;
    for (w, h, n) in invalid {
        let px = rng.bytes(n);
        let so = Counting::new(usize::MAX);
        let sn = Counting::new(usize::MAX);
        let o = run_old(&px, w, h, &so);
        let nw = run_new(&px, w, h, &sn);
        assert!(nw.is_err(), "expected an error at {w}x{h} len {n}");
        assert_eq!(o, nw, "error differs at {w}x{h} len {n}");
        assert_eq!(
            so.calls(),
            sn.calls(),
            "check count differs at {w}x{h} len {n}"
        );
        sample.get_or_insert(nw.unwrap_err());
    }
    println!("invalid-input cases: 10 (identical error Debug incl. location, check counts)");
    println!("sample normalized error: {}", sample.unwrap());

    // Cancellation at every check: identical outcome, error and call count.
    let mut cancel_cases = 0usize;
    for (w, h) in [
        (1u32, 0u32),
        (1, 1),
        (3, 15),
        (3, 16),
        (3, 17),
        (17, 33),
        (5, 100),
        (64, 257),
    ] {
        let px = rng.bytes(w as usize * h as usize * 4 + 5);
        let total = 1 + (h as usize).div_ceil(16);
        for cancel_at in 0..=total + 1 {
            let so = Counting::new(cancel_at);
            let sn = Counting::new(cancel_at);
            let o = run_old(&px, w, h, &so);
            let nw = run_new(&px, w, h, &sn);
            assert_eq!(
                o, nw,
                "cancel outcome differs at {w}x{h} cancel_at {cancel_at}"
            );
            assert_eq!(
                so.calls(),
                sn.calls(),
                "cancel calls differ at {w}x{h} cancel_at {cancel_at}"
            );
            assert_eq!(
                nw.is_err(),
                cancel_at < total,
                "{w}x{h} cancel_at {cancel_at} of {total}"
            );
            cancel_cases += 1;
        }
    }
    println!("cancellation cases: {cancel_cases} (identical Ok/Err, error Debug, check counts)");

    for (w, h) in [(1920u32, 1080u32), (4096, 4096), (4095, 4097)] {
        let px = rng.bytes(w as usize * h as usize * 4 + 11);
        let s = Counting::new(usize::MAX);
        let o = run_old(&px, w, h, &s);
        let nw = run_new(&px, w, h, &s);
        assert_eq!(o, nw, "large {w}x{h}");
        assert_eq!(nw.unwrap().0, spec(&px, w, h), "large spec {w}x{h}");
    }
    println!("large cases: 3 (1920x1080, 4096x4096, 4095x4097)");
    println!("core: PASS");
}

/// Candidate output plus input-derived expectations for an external decoder.
fn write_oracle(dir: &Path) {
    let mut rng = Rng(SEED ^ 0x6f72_6163_6c65);
    let all = all_values();
    for (w, h) in [(1u32, 256u32), (257, 3), (17, 33), (640, 480)] {
        let n = w as usize * h as usize * 4;
        let px = if n == all.len() {
            all.clone()
        } else {
            rng.bytes(n)
        };
        let s = Counting::new(usize::MAX);
        let enc = run_new(&px, w, h, &s).expect("valid input encodes").0;
        let stem = dir.join(format!("rgba8_{w}x{h}"));
        std::fs::write(stem.with_extension("ff"), &enc).unwrap();
        // Expectations come from the input, never from the encoder.
        std::fs::write(stem.with_extension("expect16be"), &spec(&px, w, h)[16..]).unwrap();
        std::fs::write(stem.with_extension("input8"), &px).unwrap();
        println!("wrote {}", stem.with_extension("ff").display());
    }
}

/// Open bug probe: zero height with a huge width. The pixel-count checks
/// pass (`w * 0 == 0`), but the per-row stride `w * bytes_per_pixel`
/// overflows a 32-bit `usize`. Correct behavior is a 16-byte header-only
/// file or a typed error; a panic fails this probe on any target.
fn f6() -> ExitCode {
    std::panic::set_hook(Box::new(|_| {}));
    let widths = [1u32 << 29, 1 << 30, 1 << 31, 3 << 30, u32::MAX];
    let mut rows: Vec<(String, String)> = Vec::new();
    macro_rules! probe {
        ($krate:ident) => {
            for (name, layout) in [
                ("Rgba8", $krate::PixelLayout::Rgba8),
                ("Rgba16", $krate::PixelLayout::Rgba16),
                ("Rgb8", $krate::PixelLayout::Rgb8),
                ("Bgra8", $krate::PixelLayout::Bgra8),
                ("Bgrx8", $krate::PixelLayout::Bgrx8),
                ("Bgr8", $krate::PixelLayout::Bgr8),
                ("Gray8", $krate::PixelLayout::Gray8),
            ] {
                for w in widths {
                    let r = std::panic::catch_unwind(|| {
                        $krate::encode_farbfeld(&[], w, 0, layout, Unstoppable)
                    });
                    let outcome = match r {
                        Ok(Ok(v)) if v == spec(&[], w, 0) => "ok header-only".to_string(),
                        Ok(Ok(v)) => format!("WRONG OUTPUT len {}", v.len()),
                        Ok(Err(e)) => format!("err {:?}", e.error()),
                        Err(p) => {
                            let m = p
                                .downcast_ref::<&str>()
                                .map(|s| s.to_string())
                                .or_else(|| p.downcast_ref::<String>().cloned())
                                .unwrap_or_default();
                            format!("PANIC {m}")
                        }
                    };
                    let key = format!("{:<6} w={:#010x} h=0", name, w);
                    println!("{} {key} -> {outcome}", stringify!($krate));
                    rows.push((key, outcome));
                }
            }
        };
    }
    probe!(old);
    let old_len = rows.len();
    probe!(new);
    let (old_rows, new_rows) = rows.split_at(old_len);
    let same = old_rows == new_rows;
    let bad = |r: &[(String, String)]| {
        r.iter()
            .filter(|(_, o)| o.starts_with("PANIC") || o.starts_with("WRONG"))
            .count()
    };
    println!("usize bits: {}", usize::BITS);
    println!(
        "old and new outcomes identical: {}",
        if same { "yes" } else { "NO" }
    );
    println!(
        "failing cases: old {} of {}, new {} of {}",
        bad(old_rows),
        old_rows.len(),
        bad(new_rows),
        new_rows.len()
    );
    if bad(new_rows) > 0 {
        println!("f6: FAIL (open bug reproduced: zero-height wide-image stride overflow)");
        return ExitCode::FAILURE;
    }
    if !same {
        println!("f6: FAIL (candidate behavior differs from the baseline)");
        return ExitCode::FAILURE;
    }
    println!("f6: PASS");
    ExitCode::SUCCESS
}

/// An Rgba8 encoder under test: (pixels, width, height) -> farbfeld file.
type Encoder<'a> = dyn Fn(&[u8], u32, u32) -> Vec<u8> + 'a;

/// Maps (row, column, pixel) to its 8 output bytes; `None` leaves them zero.
type PixelMap<'a> = dyn Fn(usize, usize, &[u8; 4]) -> Option<[u8; 8]> + 'a;

/// Fixed excerpt of the candidate's in-repo test
/// `rgba8_matches_native_rgba16_expansion_with_odd_rows_and_input_tail`
/// (b76f4229), with the Rgba8 encoder replaced by `enc`. Frozen here so the
/// replay measures that test's power, not whatever the test becomes later.
fn excerpt_passes(enc: &Encoder<'_>) -> bool {
    for (width, height) in [(1u32, 256u32), (3, 17), (17, 33), (64, 1), (257, 3)] {
        let count = width as usize * height as usize;
        let mut rgba: Vec<u8> = (0..count)
            .flat_map(|i| {
                let v = i as u8;
                [v, v.wrapping_mul(73), v.rotate_left(3), v.wrapping_add(109)]
            })
            .collect();
        let words: Vec<u8> = rgba
            .iter()
            .flat_map(|&v| (u16::from(v) * 257).to_ne_bytes())
            .collect();
        rgba.extend_from_slice(&[0xa5; 13]);
        let actual = enc(&rgba, width, height);
        let from_words =
            new::encode_farbfeld(&words, width, height, new::PixelLayout::Rgba16, Unstoppable)
                .unwrap();
        if actual != from_words {
            return false;
        }
        let decoded = new::decode_farbfeld(&actual, Unstoppable).unwrap();
        if decoded.pixels() != &words[..] {
            return false;
        }
    }
    true
}

/// Re-encodes with a per-pixel map.
fn mutant(px: &[u8], w: u32, h: u32, use_tail: bool, f: &PixelMap<'_>) -> Vec<u8> {
    let mut v = spec(&[], w, h);
    let expected = w as usize * h as usize * 4;
    let src = if use_tail { px } else { &px[..expected] };
    for (r, row) in src.chunks_exact(w as usize * 4).enumerate() {
        for (c, p) in row.as_chunks::<4>().0.iter().enumerate() {
            v.extend(f(r, c, p).unwrap_or([0; 8]));
        }
    }
    v
}

fn doubled(p: &[u8; 4]) -> [u8; 8] {
    [p[0], p[0], p[1], p[1], p[2], p[2], p[3], p[3]]
}

fn mutants() -> ExitCode {
    let real = |px: &[u8], w, h| {
        new::encode_farbfeld(px, w, h, new::PixelLayout::Rgba8, Unstoppable).unwrap()
    };
    assert!(excerpt_passes(&real), "candidate must pass the excerpt");
    assert!(
        excerpt_passes(&|px, w, h| mutant(px, w, h, false, &|_, _, p| Some(doubled(p)))),
        "the unmutated map must pass the excerpt"
    );
    let cases: Vec<(&str, Box<Encoder<'static>>)> = vec![
        (
            "value v, not v*257 ([0,v])",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|_, _, p| {
                    Some([0, p[0], 0, p[1], 0, p[2], 0, p[3]])
                })
            }),
        ),
        (
            "value v*256 ([v,0])",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|_, _, p| {
                    Some([p[0], 0, p[1], 0, p[2], 0, p[3], 0])
                })
            }),
        ),
        (
            "planar interleave [r,g,b,a,r,g,b,a]",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|_, _, p| {
                    Some([p[0], p[1], p[2], p[3], p[0], p[1], p[2], p[3]])
                })
            }),
        ),
        (
            "R<->B swap",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|_, _, p| {
                    Some(doubled(&[p[2], p[1], p[0], p[3]]))
                })
            }),
        ),
        (
            "G<->A swap",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|_, _, p| {
                    Some(doubled(&[p[0], p[3], p[2], p[1]]))
                })
            }),
        ),
        (
            "alpha forced 0xffff",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|_, _, p| {
                    Some(doubled(&[p[0], p[1], p[2], 255]))
                })
            }),
        ),
        (
            "last pixel of each row unwritten",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|_, c, p| {
                    (c + 1 != w as usize).then(|| doubled(p))
                })
            }),
        ),
        (
            "first pixel of each row unwritten",
            Box::new(|px, w, h| mutant(px, w, h, false, &|_, c, p| (c != 0).then(|| doubled(p)))),
        ),
        (
            "every 16th row unwritten",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|r, _, p| {
                    (r % 16 != 15).then(|| doubled(p))
                })
            }),
        ),
        (
            "trailing input consumed as rows",
            Box::new(|px, w, h| mutant(px, w, h, true, &|_, _, p| Some(doubled(p)))),
        ),
        (
            "pixels reversed within each row",
            Box::new(|px, w, h| {
                let mut v = spec(&[], w, h);
                for row in px[..w as usize * h as usize * 4].chunks_exact(w as usize * 4) {
                    for p in row.as_chunks::<4>().0.iter().rev() {
                        v.extend(doubled(p));
                    }
                }
                v
            }),
        ),
        (
            "last row unwritten",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &move |r, _, p| {
                    (r + 1 != h as usize).then(|| doubled(p))
                })
            }),
        ),
        (
            "only pixel 0 of the image unwritten",
            Box::new(|px, w, h| {
                mutant(px, w, h, false, &|r, c, p| {
                    (r != 0 || c != 0).then(|| doubled(p))
                })
            }),
        ),
    ];
    let mut survived = 0;
    for (name, m) in &cases {
        let passes = excerpt_passes(m.as_ref());
        println!(
            "  mutant {name:<40} {}",
            if passes { "SURVIVED" } else { "killed" }
        );
        survived += usize::from(passes);
    }
    println!(
        "excerpt replay: {} of {} mutants killed",
        cases.len() - survived,
        cases.len()
    );
    if survived > 0 {
        println!("mutants: FAIL");
        return ExitCode::FAILURE;
    }
    println!("mutants: PASS");
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["core"] => {
            core();
            ExitCode::SUCCESS
        }
        ["mutants"] => mutants(),
        ["f6"] => f6(),
        ["write-oracle", dir] => {
            write_oracle(Path::new(dir));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: farbfeld-parity core | mutants | f6 | write-oracle <existing-dir>");
            ExitCode::from(2)
        }
    }
}
