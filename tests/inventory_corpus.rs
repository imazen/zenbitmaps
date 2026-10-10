//! Inventories over the whole bmp/pnm/farbfeld conformance corpus (valid,
//! non-conformant, invalid): coverage must hold for every file, and for every
//! file the decoder accepts the pixel-extent properties must hold too.
#![cfg(all(feature = "zencodec", feature = "bmp", not(target_arch = "wasm32")))]

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use zenbitmaps::{BmpDecoderConfig, FarbfeldDecoderConfig, PnmDecoderConfig};
use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
use zencodec::inventory::{Disposition, Inventory};

static TRUNCATED_BUT_DECODED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

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
    assert!(!out.is_empty(), "no files under {sub}");
    out
}

fn inventory_of<C: DecoderConfig>(cfg: C, data: &[u8], what: &Path) -> Inventory {
    let inv = cfg
        .job()
        .inventory(data)
        .unwrap_or_else(|e| panic!("{}: inventory failed: {e}", what.display()))
        .expect("capability");
    inv.validate()
        .unwrap_or_else(|e| panic!("{}: invalid inventory: {e}\n{inv}", what.display()));
    inv
}

fn decode_pixels<C: DecoderConfig>(cfg: C, data: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let dec = cfg
        .job()
        .decoder(Cow::Borrowed(data), &[])
        .map_err(|e| e.to_string())?;
    let out = dec.decode().map_err(|e| e.to_string())?;
    let px = out.pixels();
    Ok((px.width(), px.rows(), px.contiguous_bytes().into_owned()))
}

fn pixel_end(inv: &Inventory) -> u64 {
    inv.parts()
        .iter()
        .filter(|p| p.parent.is_none() && p.disposition == Disposition::ImageData)
        .map(|p| p.range.end)
        .max()
        .expect("an ImageData part")
}

/// Returns whether the decoder accepted the file.
fn check_file<C: DecoderConfig + Clone>(cfg: C, path: &Path, necessary_slack: u64) -> bool {
    let data = std::fs::read(path).unwrap();
    let inv = inventory_of(cfg.clone(), &data, path);
    let Ok(full) = decode_pixels(cfg.clone(), &data) else {
        return false;
    };
    // A file the decoder accepts although its pixel array is cut short: the
    // decoder zero-fills the missing pixels, so bytes appended to the file are
    // read as pixels. The inventory reports exactly that (ImageData to the end
    // of the file), which the junk check of `check_inventory` rejects by design.
    if inv.parts().iter().any(|p| {
        p.disposition == Disposition::ImageData
            && p.detail
                .as_deref()
                .is_some_and(|d| d.starts_with("truncated"))
    }) {
        TRUNCATED_BUT_DECODED
            .lock()
            .unwrap()
            .push(path.file_name().unwrap().to_string_lossy().into_owned());
        return true;
    }
    zencodec_testkit::check_inventory(cfg.clone(), &data)
        .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
    let end = pixel_end(&inv);
    let cut = decode_pixels(cfg.clone(), &data[..end as usize]).unwrap_or_else(|e| {
        panic!(
            "{}: cutting at the ImageData end ({end}) breaks decoding: {e}\n{inv}",
            path.display()
        )
    });
    assert_eq!(
        cut,
        full,
        "{}: prefix decodes differently\n{inv}",
        path.display()
    );
    let mut junked = data[..end as usize].to_vec();
    junked.extend((0..61u8).map(|i| i.wrapping_mul(29) ^ 0x5C));
    let j = decode_pixels(cfg.clone(), &junked).unwrap_or_else(|e| {
        panic!(
            "{}: junk after the ImageData breaks decoding: {e}",
            path.display()
        )
    });
    assert_eq!(
        j,
        full,
        "{}: junk after the ImageData changes pixels",
        path.display()
    );
    if necessary_slack > 0 && end > necessary_slack {
        let short = &data[..(end - necessary_slack) as usize];
        match decode_pixels(cfg, short) {
            Err(_) => {}
            Ok(d) => assert_ne!(
                d,
                full,
                "{}: the last {necessary_slack} byte(s) of the ImageData are not needed\n{inv}",
                path.display()
            ),
        }
    }
    true
}

#[test]
fn pnm_corpus() {
    let mut accepted = 0;
    let mut total = 0;
    for dir in [
        "pnm-conformance/valid",
        "pnm-conformance/edge-cases",
        "pnm-conformance/invalid",
    ] {
        for f in corpus_files(dir) {
            total += 1;
            // ASCII files end in a token, so one byte may be a digit's tail: the
            // exact-byte necessity holds for binary files only.
            let ascii = std::fs::read(&f)
                .unwrap()
                .get(1)
                .is_some_and(|b| matches!(b, b'1' | b'2' | b'3'));
            accepted += usize::from(check_file(PnmDecoderConfig::new(), &f, u64::from(!ascii)));
        }
    }
    eprintln!("pnm corpus: {accepted} of {total} files decode");
    assert!(accepted >= 40, "decodable pnm files: {accepted}");
}

#[test]
fn farbfeld_corpus() {
    let mut accepted = 0;
    let mut total = 0;
    for dir in [
        "farbfeld-conformance/valid",
        "farbfeld-conformance/edge-cases",
        "farbfeld-conformance/invalid",
    ] {
        for f in corpus_files(dir) {
            total += 1;
            accepted += usize::from(check_file(FarbfeldDecoderConfig::new(), &f, 1));
        }
    }
    eprintln!("farbfeld corpus: {accepted} of {total} files decode");
    assert!(accepted >= 15, "decodable farbfeld files: {accepted}");
}

#[test]
fn bmp_corpus() {
    let mut accepted = 0;
    let mut total = 0;
    for dir in [
        "bmp-conformance/valid",
        "bmp-conformance/non-conformant",
        "bmp-conformance/invalid",
    ] {
        for f in corpus_files(dir) {
            total += 1;
            // Uncompressed rows end in padding the decoder does not insist on,
            // so necessity is only checked for a whole missing row's worth.
            let data = std::fs::read(&f).unwrap();
            let slack = bmp_row_stride(&data);
            accepted += usize::from(check_file(BmpDecoderConfig::new(), &f, slack));
        }
    }
    eprintln!("bmp corpus: {accepted} of {total} files decode");
    let mut t = TRUNCATED_BUT_DECODED.lock().unwrap().clone();
    t.sort();
    eprintln!("decoded despite a truncated pixel array: {t:?}");
    assert_eq!(
        t,
        ["rgb32h52.bmp", "rgba32h56.bmp"],
        "files the 52/56/64-byte header over-read leaves short"
    );
    assert!(accepted >= 80, "decodable bmp files: {accepted}");
}

/// One row of an uncompressed BMP in bytes; 0 for RLE or anything unreadable.
fn bmp_row_stride(data: &[u8]) -> u64 {
    let get = |at: usize, n: usize| -> Option<u64> {
        let b = data.get(at..at + n)?;
        Some(b.iter().rev().fold(0u64, |a, &x| (a << 8) | u64::from(x)))
    };
    let (Some(ihsize), Some(w)) = (get(14, 4), get(18, 4)) else {
        return 0;
    };
    let (bpp, comp) = if ihsize == 12 {
        (get(24, 2), Some(0))
    } else {
        (get(28, 2), get(30, 4))
    };
    match (bpp, comp) {
        (Some(bpp), Some(0 | 3 | 6)) => (w * bpp).div_ceil(32) * 4,
        _ => 0,
    }
}
