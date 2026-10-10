//! Memory shape of the walkers on flood inputs. Run one test at a time under
//! `/usr/bin/time -v` with `INVENTORY_MEM_N` set (default: a small count so the
//! plain test run stays cheap); `baseline_input_only` gives the input-only cost.
#![cfg(all(feature = "zencodec", feature = "hdr"))]
use zencodec::decode::{DecodeJob, DecoderConfig};

fn n() -> usize {
    std::env::var("INVENTORY_MEM_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000)
}

fn pnm_flood() -> Vec<u8> {
    let mut v = b"P5\n".to_vec();
    for _ in 0..n() {
        v.extend_from_slice(b"#\n");
    }
    v.extend_from_slice(b"1 1 255\n\x00");
    v
}

fn hdr_flood() -> Vec<u8> {
    let mut v = b"#?RADIANCE\n".to_vec();
    for _ in 0..n() {
        v.extend_from_slice(b"a\n");
    }
    v.extend_from_slice(b"\n-Y 1 +X 1\n\x80\x80\x80\x81");
    v
}

#[test]
fn baseline_input_only() {
    std::hint::black_box(pnm_flood());
}

#[test]
fn pnm_many_comments() {
    let v = pnm_flood();
    let r = zenbitmaps::PnmDecoderConfig::new().job().inventory(&v);
    eprintln!(
        "pnm {} bytes: {:?}",
        v.len(),
        r.as_ref()
            .map(|o| o.as_ref().map(|i| i.parts().len()))
            .map_err(|e| e.to_string())
    );
    if n() > 1 << 20 {
        assert!(
            r.is_err(),
            "past the part cap the walk must stop with an error"
        );
    }
}

#[test]
fn hdr_many_lines() {
    let v = hdr_flood();
    let r = zenbitmaps::HdrDecoderConfig::new().job().inventory(&v);
    eprintln!(
        "hdr {} bytes: {:?}",
        v.len(),
        r.as_ref()
            .map(|o| o.as_ref().map(|i| i.parts().len()))
            .map_err(|e| e.to_string())
    );
}
