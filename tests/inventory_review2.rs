//! Review round 2 regression tests: RLE pad bytes, TGA colour-map specification bytes,
//! the Strict `image-size` check, the decoder-rejects convention (job limits, stop token,
//! RLE errors, palette indices, truncation) and the probe()-only metadata details.
#![cfg(all(
    feature = "zencodec",
    feature = "bmp",
    feature = "tga",
    feature = "qoi",
    feature = "hdr"
))]

use std::borrow::Cow;

use zenbitmaps::{
    BmpDecoderConfig, FarbfeldDecoderConfig, HdrDecoderConfig, PnmDecoderConfig, QoiDecoderConfig,
    TgaDecoderConfig,
};
use zencodec::ResourceLimits;
use zencodec::decode::{Decode, DecodeJob, DecodePolicy, DecoderConfig};
use zencodec::inventory::{Disposition, Inventory, Part, PartTag};

fn check(inv: Inventory) -> Inventory {
    inv.validate().unwrap_or_else(|e| panic!("{e}\n{inv}"));
    inv
}

fn inv_of<C: DecoderConfig>(cfg: C, data: &[u8]) -> Inventory {
    check(cfg.job().inventory(data).unwrap().unwrap())
}

fn bmp_inv(data: &[u8], policy: Option<DecodePolicy>) -> Inventory {
    let job = BmpDecoderConfig::new().job();
    let job = match policy {
        Some(p) => job.with_policy(p),
        None => job,
    };
    check(job.inventory(data).unwrap().unwrap())
}

fn decode_with<C: DecoderConfig>(
    cfg: C,
    policy: Option<DecodePolicy>,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let job = cfg.job();
    let job = match policy {
        Some(p) => job.with_policy(p),
        None => job,
    };
    let out = job
        .decoder(Cow::Borrowed(data), &[])
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    Ok(out.pixels().contiguous_bytes().into_owned())
}

fn leaf_at(inv: &Inventory, at: u64) -> &Part {
    inv.parts()
        .iter()
        .filter(|p| p.range.start <= at && at < p.range.end)
        .min_by_key(|p| p.range.end - p.range.start)
        .unwrap()
}

fn named<'a>(inv: &'a Inventory, name: &str) -> &'a Part {
    inv.parts()
        .iter()
        .find(|p| matches!(&p.tag, PartTag::Name(n) if n == name))
        .unwrap_or_else(|| panic!("no part {name}\n{inv}"))
}

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn bmp_head(ihsize: u32, w: i32, h: i32, bpp: u16, comp: u32, clr: u32, off: u32) -> Vec<u8> {
    let mut v = b"BM".to_vec();
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(off));
    v.extend_from_slice(&le32(ihsize));
    v.extend_from_slice(&w.to_le_bytes());
    v.extend_from_slice(&h.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&bpp.to_le_bytes());
    v.extend_from_slice(&le32(comp));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(clr));
    v.extend_from_slice(&le32(0));
    v.resize(14 + ihsize as usize, 0);
    v
}

fn flips_never_matter<C: DecoderConfig + Clone>(
    cfg: C,
    policy: Option<DecodePolicy>,
    data: &[u8],
    at: usize,
) {
    let base = decode_with(cfg.clone(), policy, data).expect("base decodes");
    for x in 1..=255u8 {
        let mut m = data.to_vec();
        m[at] ^= x;
        assert_eq!(
            decode_with(cfg.clone(), policy, &m).as_ref(),
            Ok(&base),
            "byte {at} ^ {x}"
        );
    }
}

// ── F1: RLE pad bytes ────────────────────────────────────────────────

#[test]
fn rle8_absolute_run_pad_bytes_are_padding() {
    let mut v = bmp_head(40, 8, 1, 8, 1, 2, 62);
    v.extend_from_slice(&[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0]);
    v.extend_from_slice(&[0, 3, 1, 0, 1, 0xAB, 0, 5, 0, 1, 0, 1, 0, 0xCD, 0, 1]);
    let inv = bmp_inv(&v, None);
    for at in [67u64, 75] {
        let p = leaf_at(&inv, at);
        assert_eq!(
            (p.range.clone(), p.disposition),
            (at..at + 1, Disposition::Padding),
            "{inv}"
        );
        flips_never_matter(BmpDecoderConfig::new(), None, &v, at as usize);
    }
    assert_eq!(
        named(&inv, "pixel-array").disposition,
        Disposition::ImageData
    );
}

#[test]
fn rle4_absolute_run_pad_bytes_are_padding() {
    // 8x1, 4 bpp RLE4, 16-entry palette; an absolute run of 5 pixels takes 3 bytes + 1 pad.
    let mut v = bmp_head(40, 8, 1, 4, 2, 16, 54 + 64);
    for i in 0..16u8 {
        v.extend_from_slice(&[i * 8, i * 4, i * 2, 0]);
    }
    let pad = v.len() as u64 + 5;
    v.extend_from_slice(&[0, 5, 0x12, 0x34, 0x50, 0xEE, 0, 1]);
    let inv = bmp_inv(&v, None);
    let p = leaf_at(&inv, pad);
    assert_eq!(
        (p.range.clone(), p.disposition),
        (pad..pad + 1, Disposition::Padding),
        "{inv}"
    );
    flips_never_matter(BmpDecoderConfig::new(), None, &v, pad as usize);
}

// ── F2: TGA colour-map specification bytes ───────────────────────────

#[test]
fn tga_unused_colour_map_spec_is_dropped() {
    let v = [
        0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0x20, 1, 2, 3, 4, 5, 6,
    ];
    let inv = inv_of(TgaDecoderConfig::new(), &v);
    let f = named(&inv, "colour-map-spec");
    assert_eq!(
        (f.range.clone(), f.disposition),
        (3..8, Disposition::Dropped),
        "{inv}"
    );
    for at in 3..8 {
        flips_never_matter(TgaDecoderConfig::new(), None, &v, at);
    }
    // Map present but unused (truecolor image): only the first-entry index is unused.
    let mut w = vec![0u8, 1, 2, 0, 0, 2, 0, 24, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0x20];
    w.extend_from_slice(&[9; 6]); // two 24-bit map entries
    w.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
    let inv = inv_of(TgaDecoderConfig::new(), &w);
    let f = named(&inv, "first-entry-index");
    assert_eq!(
        (f.range.clone(), f.disposition),
        (3..5, Disposition::Dropped),
        "{inv}"
    );
    for at in 3..5 {
        flips_never_matter(TgaDecoderConfig::new(), None, &w, at);
    }
}

// ── F3: Strict image-size only for uncompressed RGB ──────────────────

#[test]
fn strict_image_size_is_structure_only_for_uncompressed_rgb() {
    let strict = Some(DecodePolicy::none().with_strict(true));
    let mut bf = bmp_head(40, 2, 1, 16, 3, 0, 66);
    bf[34..38].copy_from_slice(&le32(4));
    bf.extend_from_slice(&le32(0xF800));
    bf.extend_from_slice(&le32(0x07E0));
    bf.extend_from_slice(&le32(0x001F));
    bf.extend_from_slice(&[0x1F, 0, 0xE0, 0x07]);
    let inv = bmp_inv(&bf, strict);
    assert_eq!(leaf_at(&inv, 34).disposition, Disposition::Dropped, "{inv}");
    for at in 34..38 {
        flips_never_matter(BmpDecoderConfig::new(), strict, &bf, at);
    }
    let mut rgb = bmp_head(40, 1, 1, 24, 0, 0, 54);
    rgb[34..38].copy_from_slice(&le32(4));
    rgb.extend_from_slice(&[1, 2, 3, 0]);
    let inv = bmp_inv(&rgb, strict);
    assert_eq!(
        leaf_at(&inv, 34).disposition,
        Disposition::Structure,
        "{inv}"
    );
    let mut m = rgb.clone();
    m[34] = 9;
    assert!(decode_with(BmpDecoderConfig::new(), strict, &m).is_err());
}

// ── F4: metadata only probe() reports ────────────────────────────────

#[test]
fn probe_only_metadata_says_so() {
    let mut v = bmp_head(40, 1, 1, 24, 0, 0, 54);
    v[38..42].copy_from_slice(&le32(2835));
    v.extend_from_slice(&[1, 2, 3, 0]);
    let inv = bmp_inv(&v, None);
    assert!(
        named(&inv, "resolution")
            .detail
            .as_deref()
            .unwrap()
            .contains("probe() only")
    );
    let mut q = b"qoif".to_vec();
    q.extend_from_slice(&1u32.to_be_bytes());
    q.extend_from_slice(&1u32.to_be_bytes());
    q.extend_from_slice(&[3, 1, 0xFE, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 1]);
    let inv = inv_of(QoiDecoderConfig::new(), &q);
    assert!(
        named(&inv, "colorspace")
            .detail
            .as_deref()
            .unwrap()
            .contains("probe() only")
    );
}

// ── F5: files the decoder rejects ────────────────────────────────────

fn small_files() -> Vec<(&'static str, Vec<u8>)> {
    let mut bmp = bmp_head(40, 2, 1, 24, 0, 0, 54);
    bmp.extend_from_slice(&[1, 2, 3, 4, 5, 6, 0, 0]);
    let tga = vec![
        0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0x20, 1, 2, 3, 4, 5, 6,
    ];
    let pnm = b"P6\n2 1\n255\n\x01\x02\x03\x04\x05\x06".to_vec();
    let mut ff = b"farbfeld".to_vec();
    ff.extend_from_slice(&2u32.to_be_bytes());
    ff.extend_from_slice(&1u32.to_be_bytes());
    ff.extend_from_slice(&[7; 16]);
    let mut qoi = b"qoif".to_vec();
    qoi.extend_from_slice(&2u32.to_be_bytes());
    qoi.extend_from_slice(&1u32.to_be_bytes());
    qoi.extend_from_slice(&[3, 0, 0xFE, 1, 2, 3, 0xC0, 0, 0, 0, 0, 0, 0, 0, 1]);
    let mut hdr = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 2\n".to_vec();
    hdr.extend_from_slice(&[128, 128, 128, 129, 64, 64, 64, 128]);
    vec![
        ("bmp", bmp),
        ("tga", tga),
        ("pnm", pnm),
        ("farbfeld", ff),
        ("qoi", qoi),
        ("hdr", hdr),
    ]
}

fn says_rejected(inv: &Inventory) -> bool {
    inv.parts().iter().any(|p| {
        p.disposition == Disposition::ImageData
            && p.detail
                .as_deref()
                .is_some_and(|d| d.contains("rejects this file under the job's limits"))
    })
}

fn with_limits<C: DecoderConfig>(cfg: C, limits: ResourceLimits, data: &[u8]) -> (Inventory, bool) {
    let inv = check(
        cfg.clone()
            .job()
            .with_limits(limits)
            .inventory(data)
            .unwrap()
            .unwrap(),
    );
    let rejected = cfg
        .job()
        .with_limits(limits)
        .decoder(Cow::Borrowed(data), &[])
        .and_then(|d| d.decode())
        .is_err();
    (inv, rejected)
}

#[test]
fn job_limits_mark_the_file_as_rejected() {
    for limits in [
        ResourceLimits::none().with_max_pixels(1),
        ResourceLimits::none().with_max_input_bytes(10),
    ] {
        for (name, data) in small_files() {
            let (inv, rejected) = match name {
                "bmp" => with_limits(BmpDecoderConfig::new(), limits, &data),
                "tga" => with_limits(TgaDecoderConfig::new(), limits, &data),
                "pnm" => with_limits(PnmDecoderConfig::new(), limits, &data),
                "farbfeld" => with_limits(FarbfeldDecoderConfig::new(), limits, &data),
                "qoi" => with_limits(QoiDecoderConfig::new(), limits, &data),
                _ => with_limits(HdrDecoderConfig::new(), limits, &data),
            };
            assert!(rejected, "{name}: the decoder accepts it under {limits:?}");
            assert!(says_rejected(&inv), "{name} under {limits:?}:\n{inv}");
        }
    }
    // Without limits nothing is marked.
    for (name, data) in small_files() {
        let inv = match name {
            "bmp" => inv_of(BmpDecoderConfig::new(), &data),
            "tga" => inv_of(TgaDecoderConfig::new(), &data),
            "pnm" => inv_of(PnmDecoderConfig::new(), &data),
            "farbfeld" => inv_of(FarbfeldDecoderConfig::new(), &data),
            "qoi" => inv_of(QoiDecoderConfig::new(), &data),
            _ => inv_of(HdrDecoderConfig::new(), &data),
        };
        assert!(!says_rejected(&inv), "{name}\n{inv}");
    }
}

struct Cancelled;
impl enough::Stop for Cancelled {
    fn check(&self) -> Result<(), enough::StopReason> {
        Err(enough::StopReason::Cancelled)
    }
}

#[test]
fn a_fired_stop_token_fails_the_inventory() {
    let data = &small_files()[2].1;
    let job = PnmDecoderConfig::new()
        .job()
        .with_stop(zencodec::StopToken::new(Cancelled));
    assert!(job.inventory(data).is_err());
}

#[test]
fn rle_errors_are_malformed_outside_permissive() {
    // 2x1 RLE8 with a 2-entry palette; an encoded run of 9 pixels overruns the picture.
    let mut v = bmp_head(40, 2, 1, 8, 1, 2, 62);
    v.extend_from_slice(&[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0]);
    v.extend_from_slice(&[9, 1, 0, 1]);
    assert!(decode_with(BmpDecoderConfig::new(), None, &v).is_err());
    let inv = bmp_inv(&v, None);
    let px = named(&inv, "pixel-array");
    assert_eq!(px.disposition, Disposition::Malformed, "{inv}");
    assert!(
        px.detail
            .as_deref()
            .unwrap()
            .contains("RLE position overrun")
    );
    let perm = Some(DecodePolicy::permissive());
    assert!(decode_with(BmpDecoderConfig::new(), perm, &v).is_ok());
    let inv = bmp_inv(&v, perm);
    assert_ne!(
        named(&inv, "pixel-array").disposition,
        Disposition::Malformed,
        "{inv}"
    );
}

#[test]
fn out_of_range_palette_index_is_malformed() {
    let mut v = bmp_head(40, 4, 1, 8, 0, 2, 62);
    v.extend_from_slice(&[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0]);
    v.extend_from_slice(&[0, 1, 5, 1]);
    assert!(decode_with(BmpDecoderConfig::new(), None, &v).is_err());
    let inv = bmp_inv(&v, None);
    let px = named(&inv, "pixel-array");
    assert_eq!(px.disposition, Disposition::Malformed, "{inv}");
    assert!(px.detail.as_deref().unwrap().contains("palette index 5"));
}

#[test]
fn truncations_follow_the_decoder() {
    // 24 bpp: the decoder rejects a short row.
    let mut v = bmp_head(40, 2, 2, 24, 0, 0, 54);
    v.extend_from_slice(&[1, 2, 3, 4, 5, 6, 0, 0, 7, 8]);
    assert!(decode_with(BmpDecoderConfig::new(), None, &v).is_err());
    assert_eq!(
        named(&bmp_inv(&v, None), "pixel-array").disposition,
        Disposition::Malformed
    );
    // 32 bpp: zero-filled, and a partial last pixel is not read (F6).
    let mut v = bmp_head(40, 2, 2, 32, 0, 0, 54);
    v.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
    assert!(decode_with(BmpDecoderConfig::new(), None, &v).is_ok());
    let inv = bmp_inv(&v, None);
    let px = named(&inv, "pixel-array");
    assert_eq!(px.disposition, Disposition::ImageData, "{inv}");
    assert!(
        px.detail
            .as_deref()
            .unwrap()
            .contains("partial last pixel is not read")
    );
    // farbfeld, QOI, PNM and TGA reject every truncation.
    for (name, data) in small_files() {
        if matches!(name, "bmp" | "hdr") {
            continue;
        }
        // QOI ends in an 8-byte marker the decoder never reads: cut inside the ops.
        let cut = if name == "qoi" {
            &data[..17]
        } else {
            &data[..data.len() - 3]
        };
        let inv = match name {
            "tga" => inv_of(TgaDecoderConfig::new(), cut),
            "pnm" => inv_of(PnmDecoderConfig::new(), cut),
            "farbfeld" => inv_of(FarbfeldDecoderConfig::new(), cut),
            _ => inv_of(QoiDecoderConfig::new(), cut),
        };
        assert!(
            inv.parts()
                .iter()
                .all(|p| p.disposition != Disposition::ImageData),
            "{name}\n{inv}"
        );
    }
}
