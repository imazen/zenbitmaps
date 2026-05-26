//! Fuzz crash regression suite (DEDUP-J template, ported from zenwebp).
//!
//! Runs every file in `fuzz/regression/` through every decoder entry point that
//! has a fuzz target. Each seed file is a previously-found crash that has been
//! fixed; this test ensures none of them re-introduce a panic.
//!
//! Reproduces what the `fuzz_decode` and `fuzz_roundtrip` fuzz targets do, but
//! as a regular `cargo test` — no nightly toolchain needed. Failures here mean
//! a regression of a previously-fixed bug.
//!
//! To add a new seed: drop the (preferably minimized) crash file into
//! `fuzz/regression/` (or a per-target subdir under it), no other action
//! required.

use std::fs;
use std::path::PathBuf;

fn regression_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fuzz/regression")
}

/// Recursively collect every regular file under `dir`. Skips dotfiles and
/// silently tolerates a missing directory.
fn collect_seeds(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    let read = match fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return,
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        match entry.file_type() {
            Ok(t) if t.is_file() => out.push(path),
            Ok(t) if t.is_dir() => collect_seeds(&path, out),
            _ => {}
        }
    }
}

fn run_decode(input: &[u8]) {
    // Mirrors fuzz_targets/fuzz_decode.rs.
    let _ = zenbitmaps::decode(input, enough::Unstoppable);
    #[cfg(feature = "bmp")]
    {
        let _ = zenbitmaps::decode_bmp(input, enough::Unstoppable);
    }
    let _ = zenbitmaps::decode_farbfeld(input, enough::Unstoppable);
}

fn run_roundtrip(input: &[u8]) {
    use zenbitmaps::{decode, encode_pam};
    // Mirrors fuzz_targets/fuzz_roundtrip.rs (subset: PNM + BMP — drop the
    // strict assertions used in the fuzz target; the regression harness only
    // checks "doesn't panic").
    if let Ok(decoded) = decode(input, enough::Unstoppable) {
        let _ = encode_pam(
            decoded.pixels(),
            decoded.width,
            decoded.height,
            decoded.layout,
            enough::Unstoppable,
        );
    }
    #[cfg(feature = "bmp")]
    {
        use zenbitmaps::{decode_bmp, encode_bmp, encode_bmp_rgba, PixelLayout};
        if let Ok(decoded) = decode_bmp(input, enough::Unstoppable) {
            if decoded.layout == PixelLayout::Rgba8 {
                let _ = encode_bmp_rgba(
                    decoded.pixels(),
                    decoded.width,
                    decoded.height,
                    decoded.layout,
                    enough::Unstoppable,
                );
            } else {
                let _ = encode_bmp(
                    decoded.pixels(),
                    decoded.width,
                    decoded.height,
                    decoded.layout,
                    enough::Unstoppable,
                );
            }
        }
    }
}

#[test]
fn fuzz_regression_seeds_do_not_panic() {
    let dir = regression_dir();
    let mut seeds = Vec::new();
    collect_seeds(&dir, &mut seeds);

    if seeds.is_empty() {
        eprintln!(
            "note: no regression seeds found under {} — nothing to check",
            dir.display()
        );
        return;
    }

    for path in seeds {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unnamed>")
            .to_owned();
        let input = fs::read(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));

        // Each entry point may return Err but must not panic. If any panics,
        // the test fails with the seed name in the unwind message.
        run_decode(&input);
        run_roundtrip(&input);

        eprintln!("ok: {name} ({} bytes)", input.len());
    }
}
