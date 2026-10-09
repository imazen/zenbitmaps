//! Cross-checks the inventories against independent tools. Run through
//! `just inventory-oracle`, which sets:
//!
//! - `INVENTORY_ORACLE_EXIFTOOL`: the `exiftool` binary. `exiftool -v3` prints
//!   the length of every PNM header and the size of each BMP info header and
//!   embedded ICC profile; each must match the inventory's part.
//! - `INVENTORY_ORACLE_MAGICK`: the ImageMagick `magick` binary. It decodes
//!   the whole file and the file cut at the end of our ImageData and the two
//!   must give the same pixels: an independent decoder confirms that nothing
//!   the walker left out is needed.
//!
//! Both variables must be set; the tests fail loudly when they are not.
#![cfg(all(feature = "zencodec", feature = "bmp", not(target_arch = "wasm32")))]

use std::path::{Path, PathBuf};
use std::process::Command;

use zenbitmaps::{BmpDecoderConfig, FarbfeldDecoderConfig, PnmDecoderConfig};
use zencodec::decode::{DecodeJob, DecoderConfig};
use zencodec::inventory::{Disposition, Inventory, PartKind, PartTag};

fn tool(var: &str) -> String {
    std::env::var(var)
        .unwrap_or_else(|_| panic!("{var} must name the tool binary (use `just inventory-oracle`)"))
}

fn corpus_files(sub: &str) -> Vec<PathBuf> {
    let corpus = codec_corpus::Corpus::new().expect("codec-corpus");
    let root = corpus
        .get(sub)
        .unwrap_or_else(|e| panic!("corpus {sub}: {e}"));
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .is_some_and(|x| !matches!(x.to_str(), Some("md" | "py" | "txt" | "json")))
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn inventory_of<C: DecoderConfig>(cfg: C, data: &[u8]) -> Inventory {
    let inv = cfg.job().inventory(data).unwrap().unwrap();
    inv.validate().unwrap();
    inv
}

fn exiftool_v3(path: &Path) -> String {
    let out = Command::new(tool("INVENTORY_ORACLE_EXIFTOOL"))
        .arg("-v3")
        .arg(path)
        .output()
        .expect("run exiftool");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `N` from the first line like `PPM header (13 bytes):`.
fn pnm_header_len(report: &str) -> Option<u64> {
    report.lines().find_map(|l| {
        let (_, rest) = l.split_once(" header (")?;
        rest.split_once(" bytes")?.0.parse().ok()
    })
}

#[test]
#[ignore = "needs exiftool and ImageMagick: run `just inventory-oracle`"]
fn exiftool_agrees_on_pnm_header_lengths() {
    let mut rows = Vec::new();
    let (mut checked, mut unread) = (0, Vec::new());
    for dir in ["pnm-conformance/valid", "pnm-conformance/edge-cases"] {
        for f in corpus_files(dir) {
            let data = std::fs::read(&f).unwrap();
            let Some(n) = pnm_header_len(&exiftool_v3(&f)) else {
                unread.push(f.file_name().unwrap().to_string_lossy().into_owned());
                continue;
            };
            let inv = inventory_of(PnmDecoderConfig::new(), &data);
            let header = inv
                .parts()
                .iter()
                .find(|p| p.kind == PartKind::Header)
                .expect("header part");
            // The decoder's header ends one byte after the last number; exiftool
            // reports the same length.
            rows.push(format!(
                "{:<48} exiftool {:>5}  inventory {:>5}  {}",
                f.file_name().unwrap().to_string_lossy(),
                n,
                header.range.end,
                if n == header.range.end {
                    "match"
                } else {
                    "DIFFERS"
                }
            ));
            assert_eq!(header.range, 0..n, "{}\n{inv}", f.display());
            checked += 1;
        }
    }
    eprintln!("{}\nexiftool could not read: {unread:?}", rows.join("\n"));
    assert!(checked >= 20, "only {checked} PNM files compared");
}

#[test]
#[ignore = "needs exiftool and ImageMagick: run `just inventory-oracle`"]
fn exiftool_agrees_on_bmp_headers_and_profiles() {
    let mut rows = Vec::new();
    let mut checked = 0;
    for dir in ["bmp-conformance/valid", "bmp-conformance/non-conformant"] {
        for f in corpus_files(dir) {
            let data = std::fs::read(&f).unwrap();
            let report = exiftool_v3(&f);
            let Some(k) = report.lines().find_map(|l| {
                let rest = l
                    .trim_start_matches([' ', '|'])
                    .strip_prefix("+ [BinaryData directory, ")?;
                rest.split_once(" bytes")?.0.parse::<u64>().ok()
            }) else {
                continue;
            };
            let inv = inventory_of(BmpDecoderConfig::new(), &data);
            let dib = inv
                .parts()
                .iter()
                .find(|p| p.kind == PartKind::Header && matches!(p.tag, PartTag::Code(_)))
                .expect("info header part");
            let mut line = format!(
                "{:<36} info header exiftool {:>3} inventory {:>3}",
                f.file_name().unwrap().to_string_lossy(),
                k,
                dib.range.end - dib.range.start
            );
            assert_eq!(dib.range, 14..14 + k, "{}\n{inv}", f.display());
            // Embedded profile: ProfileDataOffset / ProfileSize and the byte count
            // of the ICC_Profile tag.
            let field = |name: &str| {
                report.lines().find_map(|l| {
                    l.trim_start_matches([' ', '|'])
                        .strip_prefix(name)?
                        .strip_prefix(" = ")?
                        .trim()
                        .parse::<u64>()
                        .ok()
                })
            };
            let icc_tag = report.lines().find_map(|l| {
                let rest = l.trim().strip_prefix("- Tag 'ICC_Profile' (")?;
                rest.split_once(" bytes")?.0.parse::<u64>().ok()
            });
            if let (Some(off), Some(size), Some(tag_len)) =
                (field("ProfileDataOffset"), field("ProfileSize"), icc_tag)
            {
                assert_eq!(
                    size,
                    tag_len,
                    "{}: exiftool's own ProfileSize vs tag",
                    f.display()
                );
                let profile = inv
                    .parts()
                    .iter()
                    .find(|p| matches!(&p.tag, PartTag::Name(n) if n == "icc-profile"));
                match profile {
                    Some(p) => {
                        assert_eq!(p.range, 14 + off..14 + off + size, "{}\n{inv}", f.display());
                        line.push_str(&format!(
                            "  profile {}..{} match",
                            p.range.start, p.range.end
                        ));
                    }
                    None => {
                        // Out-of-file or overlapping profile: the inventory must say why.
                        let note = inv
                            .parts()
                            .iter()
                            .any(|p| p.detail.as_deref().is_some_and(|d| d.contains("profile")));
                        assert!(
                            14 + off + size > data.len() as u64 || note,
                            "{}: profile at {}+{} missing\n{inv}",
                            f.display(),
                            14 + off,
                            size
                        );
                        line.push_str("  profile outside the file or overlapping (explained)");
                    }
                }
            }
            rows.push(line);
            checked += 1;
        }
    }
    eprintln!("{}", rows.join("\n"));
    assert!(checked >= 20, "only {checked} BMP files compared");
}

/// Decodes `data` with ImageMagick to raw 16-bit RGBA bytes.
fn magick_pixels(data: &[u8], ext: &str, scratch: &Path, tag: &str) -> Option<Vec<u8>> {
    let src = scratch.join(format!("{tag}.{ext}"));
    std::fs::write(&src, data).unwrap();
    let out = Command::new(tool("INVENTORY_ORACLE_MAGICK"))
        .arg(format!("{}[0]", src.display())) // first image only
        .args(["-depth", "16", "rgba:-"])
        .output()
        .expect("run magick");
    out.status.success().then_some(out.stdout)
}

fn scratch_dir(name: &str) -> PathBuf {
    let d = PathBuf::from(std::env::var("HOME").unwrap())
        .join("tmp")
        .join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn pixel_end(inv: &Inventory) -> Option<u64> {
    inv.parts()
        .iter()
        .filter(|p| p.parent.is_none() && p.disposition == Disposition::ImageData)
        .map(|p| p.range.end)
        .max()
}

fn magick_extent_check(
    sub: &[&str],
    ext: &str,
    cfg: impl DecoderConfig,
    scratch: &str,
) -> (usize, usize) {
    let dir = scratch_dir(scratch);
    let (mut agreed, mut skipped) = (0, 0);
    for s in sub {
        for f in corpus_files(s) {
            let data = std::fs::read(&f).unwrap();
            let Some(full) = magick_pixels(&data, ext, &dir, "full") else {
                skipped += 1; // ImageMagick rejects it too
                continue;
            };
            let inv = inventory_of(cfg.clone(), &data);
            let Some(end) = pixel_end(&inv) else {
                // ImageMagick reads a file whose structure zenbitmaps rejects.
                eprintln!(
                    "  {}: ImageMagick decodes it, zenbitmaps has no pixel data to report",
                    f.display()
                );
                skipped += 1;
                continue;
            };
            let end = end as usize;
            let cut = magick_pixels(&data[..end], ext, &dir, "cut").unwrap_or_else(|| {
                panic!(
                    "{}: ImageMagick rejects the file cut at the ImageData end {end}\n{inv}",
                    f.display()
                )
            });
            assert_eq!(
                cut,
                full,
                "{}: ImageMagick decodes the cut file differently\n{inv}",
                f.display()
            );
            agreed += 1;
        }
    }
    (agreed, skipped)
}

#[test]
#[ignore = "needs exiftool and ImageMagick: run `just inventory-oracle`"]
fn imagemagick_decodes_the_pixel_extent_alone() {
    let (a, s) = magick_extent_check(
        &[
            "farbfeld-conformance/valid",
            "farbfeld-conformance/edge-cases",
        ],
        "ff",
        FarbfeldDecoderConfig::new(),
        "inv-oracle-ff",
    );
    eprintln!("farbfeld: {a} agreed, {s} ImageMagick-rejected");
    assert!(a >= 15);
    let (a, s) = magick_extent_check(
        &["pnm-conformance/valid", "pnm-conformance/edge-cases"],
        "pnm",
        PnmDecoderConfig::new(),
        "inv-oracle-pnm",
    );
    eprintln!("pnm: {a} agreed, {s} ImageMagick-rejected");
    assert!(a >= 30);
    let (a, s) = magick_extent_check(
        &["bmp-conformance/valid"],
        "bmp",
        BmpDecoderConfig::new(),
        "inv-oracle-bmp",
    );
    eprintln!("bmp: {a} agreed, {s} ImageMagick-rejected");
    assert!(a >= 20);
}
