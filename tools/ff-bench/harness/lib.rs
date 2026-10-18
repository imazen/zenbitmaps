//! Shared workload definitions and out-of-timer validation for ff-bench.
//!
//! Compiled into BOTH the bench target (`benches/ff.rs`) and the standalone
//! `verify` binary so the timed workload cells and the checked cells can never
//! drift apart. `preflight()` runs the complete old==new byte comparison plus
//! an independent spec oracle for every cell — including the h=0 structural
//! probe and the decode control — and optionally exports unique inputs and
//! encoded outputs to a caller-selected artifact directory as SHA256 CAS
//! files. It panics on any mismatch; it is the per-process parity gate that
//! runs inside every fresh zenbench child before registration completes.

#[path = "revisions.rs"]
pub(crate) mod revisions;

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    Rgba8,
    Rgba16,
    Rgb8,
}

impl Layout {
    pub(crate) fn old_layout(self) -> old::PixelLayout {
        match self {
            Layout::Rgba8 => old::PixelLayout::Rgba8,
            Layout::Rgba16 => old::PixelLayout::Rgba16,
            Layout::Rgb8 => old::PixelLayout::Rgb8,
        }
    }
    pub(crate) fn new_layout(self) -> new::PixelLayout {
        match self {
            Layout::Rgba8 => new::PixelLayout::Rgba8,
            Layout::Rgba16 => new::PixelLayout::Rgba16,
            Layout::Rgb8 => new::PixelLayout::Rgb8,
        }
    }
    pub(crate) fn bpp(self) -> usize {
        match self {
            Layout::Rgba8 => 4,
            Layout::Rgba16 => 8,
            Layout::Rgb8 => 3,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Layout::Rgba8 => "rgba8",
            Layout::Rgba16 => "rgba16",
            Layout::Rgb8 => "rgb8",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    /// Time `encode_farbfeld` on `fill()` input.
    Encode,
    /// Time `decode_farbfeld` on the encoded `fill()` stream (control).
    Decode,
}

#[derive(Clone, Copy)]
pub(crate) struct Cell {
    pub(crate) name: &'static str,
    pub(crate) op: Op,
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) layout: Layout,
    pub(crate) seed: u64,
    /// Registered as a timed zenbench group. Untimed cells are checked only.
    pub(crate) timed: bool,
}

pub(crate) const CELLS: &[Cell] = &[
    Cell {
        name: "ff-rgba8-64",
        op: Op::Encode,
        w: 64,
        h: 64,
        layout: Layout::Rgba8,
        seed: 0x1001,
        timed: true,
    },
    Cell {
        name: "ff-rgba8-256",
        op: Op::Encode,
        w: 256,
        h: 256,
        layout: Layout::Rgba8,
        seed: 0x1002,
        timed: true,
    },
    Cell {
        name: "ff-rgba8-1024",
        op: Op::Encode,
        w: 1024,
        h: 1024,
        layout: Layout::Rgba8,
        seed: 0x1003,
        timed: true,
    },
    Cell {
        name: "ff-rgba8-4096",
        op: Op::Encode,
        w: 4096,
        h: 4096,
        layout: Layout::Rgba8,
        seed: 0x1004,
        timed: true,
    },
    Cell {
        name: "ff-rgba8-63x65",
        op: Op::Encode,
        w: 63,
        h: 65,
        layout: Layout::Rgba8,
        seed: 0x1005,
        timed: true,
    },
    Cell {
        name: "ff-rgba8-4097x3",
        op: Op::Encode,
        w: 4097,
        h: 3,
        layout: Layout::Rgba8,
        seed: 0x1006,
        timed: true,
    },
    Cell {
        name: "ff-rgba8-1x1",
        op: Op::Encode,
        w: 1,
        h: 1,
        layout: Layout::Rgba8,
        seed: 0x1007,
        timed: true,
    },
    Cell {
        name: "ff-rgba16-1024",
        op: Op::Encode,
        w: 1024,
        h: 1024,
        layout: Layout::Rgba16,
        seed: 0x1008,
        timed: true,
    },
    Cell {
        name: "ff-rgb8-1024",
        op: Op::Encode,
        w: 1024,
        h: 1024,
        layout: Layout::Rgb8,
        seed: 0x1009,
        timed: true,
    },
    Cell {
        name: "ff-decode-1024",
        op: Op::Decode,
        w: 1024,
        h: 1024,
        layout: Layout::Rgba8,
        seed: 0xdec0de,
        timed: true,
    },
    // Structural probe only: legacy zero-height behavior (F6 territory).
    Cell {
        name: "ff-rgba8-h0",
        op: Op::Encode,
        w: 4096,
        h: 0,
        layout: Layout::Rgba8,
        seed: 0x100a,
        timed: false,
    },
];

/// Deterministic xorshift64 fill: identical bytes for every arm and every
/// fresh child process.
pub(crate) fn fill(w: u32, h: u32, bpp: usize, seed: u64) -> Vec<u8> {
    let n = w as usize * h as usize * bpp;
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s as u8
        })
        .collect()
}

/// Independent spec oracle: the exact expected `encode_farbfeld` output,
/// derived from the farbfeld spec rather than either implementation. Every
/// used layout has an oracle; the check is total, not best-effort.
fn spec_encode(c: &Cell, pixels: &[u8]) -> Vec<u8> {
    let mut out = b"farbfeld".to_vec();
    out.extend_from_slice(&c.w.to_be_bytes());
    out.extend_from_slice(&c.h.to_be_bytes());
    match c.layout {
        Layout::Rgba8 => {
            for px in pixels.as_chunks::<4>().0 {
                for &v in px {
                    out.extend_from_slice(&[v, v]); // (v*257).to_be_bytes()
                }
            }
        }
        Layout::Rgba16 => {
            // Native-endian u16 -> big-endian u16.
            for pair in pixels.as_chunks::<2>().0 {
                out.extend_from_slice(&u16::from_ne_bytes(*pair).to_be_bytes());
            }
        }
        Layout::Rgb8 => {
            for px in pixels.as_chunks::<3>().0 {
                for &v in px {
                    out.extend_from_slice(&[v, v]);
                }
                out.extend_from_slice(&[255, 255]); // alpha = 65535
            }
        }
    }
    out
}

/// Spec oracle for decode output: native-endian u16 of v*257 is [v, v] on
/// little- AND big-endian (v*257 = 0xvvvv is a byte palindrome).
fn spec_decode_pixels(pixels_rgba8: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels_rgba8.len() * 2);
    for &v in pixels_rgba8 {
        out.extend_from_slice(&[v, v]);
    }
    out
}

fn header_ok(out: &[u8], c: &Cell) -> bool {
    out.len() == 16 + c.w as usize * c.h as usize * 8
        && out[..8] == *b"farbfeld"
        && out[8..12] == c.w.to_be_bytes()
        && out[12..16] == c.h.to_be_bytes()
}

// ---------- SHA256 CAS artifact export ----------

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Free bytes on the filesystem containing `dir`, via `df` (no extra deps).
fn free_bytes(dir: &Path) -> Result<u64, String> {
    let out = Command::new("df")
        .args(["-B1", "--output=avail"])
        .arg(dir)
        .output()
        .map_err(|e| format!("df failed: {e}"))?;
    if !out.status.success() {
        return Err(format!("df exited {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .nth(1)
        .and_then(|l| l.trim().parse().ok())
        .ok_or_else(|| format!("unparseable df output: {text:?}"))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    output
        .write_all(bytes)
        .map_err(|e| format!("write {}: {e}", path.display()))
}

/// Write `bytes` to `<dir>/<tag>-<sha256>.bin` iff absent; if a file with
/// that name already exists (another fresh child beat us), read it back and
/// require the exact bytes. Writes go through a pid-unique temp file plus
/// atomic hard_link so an existing object is never overwritten or seen
/// partially written.
fn write_cas(dir: &Path, tag: &str, bytes: &[u8]) -> Result<String, String> {
    let sha = sha256_hex(bytes);
    let dst = dir.join(format!("{tag}-{sha}.bin"));
    if dst.exists() {
        let got = fs::read(&dst).map_err(|e| format!("read {}: {e}", dst.display()))?;
        if got != bytes {
            return Err(format!(
                "CAS mismatch: {} content != expected bytes",
                dst.display()
            ));
        }
        return Ok(sha);
    }
    let tmp = dir.join(format!(".{tag}-{sha}.tmp-{}", std::process::id()));
    write_new(&tmp, bytes)?;
    match fs::hard_link(&tmp, &dst) {
        Ok(()) => {
            fs::remove_file(&tmp).map_err(|e| format!("remove {}: {e}", tmp.display()))?;
        }
        Err(_) if dst.exists() => {
            // Another process linked the identical object first.
            let got = fs::read(&dst).map_err(|e| format!("read {}: {e}", dst.display()))?;
            if got != bytes {
                return Err(format!(
                    "CAS mismatch: {} content != expected bytes",
                    dst.display()
                ));
            }
            fs::remove_file(&tmp).map_err(|e| format!("remove {}: {e}", tmp.display()))?;
        }
        Err(e) => {
            return Err(format!(
                "link {}: {e}; preserved {}",
                dst.display(),
                tmp.display()
            ));
        }
    }
    Ok(sha)
}

fn write_manifest(dir: &Path, body: &str) -> Result<(), String> {
    let dst = dir.join(format!("manifest-{}.json", sha256_hex(body.as_bytes())));
    if dst.exists() {
        let got = fs::read_to_string(&dst).map_err(|e| format!("read {}: {e}", dst.display()))?;
        if got != body {
            return Err(format!(
                "manifest {} differs between processes",
                dst.display()
            ));
        }
        return Ok(());
    }
    let tmp = dir.join(format!(".manifest.tmp-{}", std::process::id()));
    write_new(&tmp, body.as_bytes())?;
    match fs::hard_link(&tmp, &dst) {
        Ok(()) => fs::remove_file(&tmp).map_err(|e| format!("remove tmp: {e}")),
        Err(_) if dst.exists() => {
            let got =
                fs::read_to_string(&dst).map_err(|e| format!("read {}: {e}", dst.display()))?;
            if got != body {
                Err(format!(
                    "manifest {} differs between processes",
                    dst.display()
                ))
            } else {
                fs::remove_file(&tmp).map_err(|e| format!("remove {}: {e}", tmp.display()))
            }
        }
        Err(e) => Err(format!("link manifest: {e}; preserved {}", tmp.display())),
    }
}

/// Run every check for every cell. Panics on the first mismatch — a failed
/// child aborts the whole confirm, which is the point. When `artifact_dir`
/// is `Some`, exports each unique input/output once (CAS dedup across the
/// five fresh children) plus a manifest; export failure is fatal. When
/// `None`, all assertions still run; nothing is written.
pub(crate) fn preflight(artifact_dir: Option<PathBuf>) -> Vec<String> {
    let mut report = Vec::new();
    let mut manifest = String::from("{\n  \"cases\": [\n");
    let mut first = true;
    let mut total_bytes = 0u64;

    if let Some(dir) = &artifact_dir {
        fs::create_dir_all(dir).unwrap_or_else(|e| panic!("artifact dir {}: {e}", dir.display()));
        // Fail before writing anything if disk headroom is insufficient.
        let need: u64 = CELLS
            .iter()
            .map(|c| {
                let mut b = c.w as u64 * c.h as u64 * c.layout.bpp() as u64;
                b += 16 + c.w as u64 * c.h as u64 * 8; // encoded
                if c.op == Op::Decode {
                    b += c.w as u64 * c.h as u64 * 8; // decoded pixels
                }
                b
            })
            .sum();
        let free = free_bytes(dir).unwrap_or_else(|e| panic!("disk check: {e}"));
        assert!(
            free > need + (512 << 20),
            "insufficient disk for artifacts: need {}+512MiB, have {}",
            need,
            free
        );
    }

    for c in CELLS {
        let px = fill(c.w, c.h, c.layout.bpp(), c.seed);
        // The decode cell's timed input is this encoded stream.
        let stream_old =
            old::encode_farbfeld(&px, c.w, c.h, c.layout.old_layout(), old::Unstoppable)
                .expect("old encode");
        let stream_new =
            new::encode_farbfeld(&px, c.w, c.h, c.layout.new_layout(), new::Unstoppable)
                .expect("new encode");
        assert!(
            stream_old == stream_new,
            "{}: old != new output bytes",
            c.name
        );
        assert!(
            header_ok(&stream_old, c),
            "{}: bad header/len {}",
            c.name,
            stream_old.len()
        );
        let spec = spec_encode(c, &px);
        assert!(stream_old == spec, "{}: output != spec oracle", c.name);
        let mut line = format!(
            "PREFLIGHT {} PASS bytes={} sha256={}",
            c.name,
            stream_old.len(),
            sha256_hex(&stream_old)
        );
        total_bytes += stream_old.len() as u64;

        // Decode control: compare complete decoded pixels old vs new and
        // against the spec oracle, outside any timer.
        let decoded_old = if c.op == Op::Decode {
            let d_old = old::decode_farbfeld(&stream_old, old::Unstoppable).expect("old decode");
            let d_new = new::decode_farbfeld(&stream_new, new::Unstoppable).expect("new decode");
            assert!(
                d_old.pixels() == d_new.pixels(),
                "{}: decoded pixels old != new",
                c.name
            );
            let spec_px = spec_decode_pixels(&px);
            assert!(
                d_old.pixels() == spec_px.as_slice(),
                "{}: decoded pixels != spec oracle",
                c.name
            );
            line.push_str(&format!(" decode-bytes={}", d_old.pixels().len()));
            Some(d_old)
        } else {
            None
        };

        let (mut in_sha, mut out_sha) = (String::new(), sha256_hex(&stream_old));

        if let Some(dir) = &artifact_dir {
            match c.op {
                Op::Encode => {
                    in_sha = write_cas(dir, "input", &px).expect("export input");
                    out_sha = write_cas(dir, "output", &stream_old).expect("export output");
                }
                Op::Decode => {
                    in_sha = write_cas(dir, "input", &stream_old).expect("export stream");
                    out_sha = write_cas(dir, "decoded", decoded_old.as_ref().unwrap().pixels())
                        .expect("export decoded");
                }
            }
        }

        if !first {
            manifest.push_str(",\n");
        }
        first = false;
        manifest.push_str(&format!(
            "    {{\"name\":\"{}\",\"op\":\"{}\",\"w\":{},\"h\":{},\"layout\":\"{}\",\"bpp\":{},\"seed\":{},\"timed\":{},\"input_sha256\":\"{}\",\"output_sha256\":\"{}\",\"baseline\":\"{}\",\"candidate\":\"{}\"}}",
            c.name,
            match c.op { Op::Encode => "encode", Op::Decode => "decode" },
            c.w, c.h, c.layout.name(), c.layout.bpp(), c.seed, c.timed,
            in_sha, out_sha,
            revisions::BASELINE, revisions::CANDIDATE,
        ));
        report.push(line);
    }
    manifest.push_str("\n  ]\n}\n");

    if let Some(dir) = &artifact_dir {
        write_manifest(dir, &manifest).expect("export manifest");
        report.push(format!("artifacts: {} dir={}", total_bytes, dir.display()));
    }
    report.push("preflight: all cells old==new and spec-clean".to_string());
    report
}
