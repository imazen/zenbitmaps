//! Review round 1 regression tests: one per finding, plus a randomized byte-flip
//! property (flipping any byte the inventory calls unconsumed must not change the
//! decode result), adapted from the reviewer's probes.
#![cfg(all(
    feature = "zencodec",
    feature = "bmp",
    feature = "tga",
    feature = "qoi",
    feature = "hdr"
))]

use std::borrow::Cow;

use zenbitmaps::{
    BmpDecoderConfig, HdrDecoderConfig, PnmDecoderConfig, QoiDecoderConfig, TgaDecoderConfig,
};
use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
use zencodec::inventory::{Disposition, Inventory};

fn inv_of<C: DecoderConfig>(cfg: C, data: &[u8]) -> Inventory {
    let inv = cfg.job().inventory(data).unwrap().unwrap();
    inv.validate()
        .unwrap_or_else(|e| panic!("invalid inventory: {e}\n{inv}"));
    inv
}

fn decode<C: DecoderConfig>(cfg: C, data: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let dec = cfg
        .job()
        .decoder(Cow::Borrowed(data), &[])
        .map_err(|e| e.to_string())?;
    let out = dec.decode().map_err(|e| e.to_string())?;
    let px = out.pixels();
    Ok((px.width(), px.rows(), px.contiguous_bytes().into_owned()))
}

/// The leaf part containing byte `at`, as "kind tag range disposition".
fn part_at(inv: &Inventory, at: u64) -> String {
    let mut best: Option<&zencodec::inventory::Part> = None;
    for p in inv.parts() {
        if p.range.start <= at && at < p.range.end {
            match best {
                Some(b) if b.range.end - b.range.start <= p.range.end - p.range.start => {}
                _ => best = Some(p),
            }
        }
    }
    let p = best.unwrap();
    format!(
        "{} {} {}..{} {} detail={:?}",
        p.kind.name(),
        p.tag,
        p.range.start,
        p.range.end,
        p.disposition,
        p.detail
    )
}

fn is_consumed_at(inv: &Inventory, at: u64) -> bool {
    let mut best: Option<&zencodec::inventory::Part> = None;
    for p in inv.parts() {
        if p.range.start <= at && at < p.range.end {
            match best {
                Some(b) if b.range.end - b.range.start <= p.range.end - p.range.start => {}
                _ => best = Some(p),
            }
        }
    }
    best.unwrap().disposition.is_consumed()
}

/// Flip byte `at` and report whether decode output changes.
fn byte_matters<C: DecoderConfig + Clone>(cfg: C, data: &[u8], at: usize) -> bool {
    let a = decode(cfg.clone(), data);
    let mut m = data.to_vec();
    m[at] ^= 0xA5;
    let b = decode(cfg, &m);
    a != b
}

/// The newline that ends a skipped header line, or the `#` that starts a skipped
/// comment, belongs to that unit's own framing: changing it merges or splits lines.
fn is_unit_framing(inv: &Inventory, data: &[u8], at: u64) -> bool {
    let leaf = inv
        .parts()
        .iter()
        .filter(|p| p.range.start <= at && at < p.range.end)
        .min_by_key(|p| p.range.end - p.range.start)
        .unwrap();
    let line_like = matches!(leaf.kind, zencodec::inventory::PartKind::Attribute)
        || matches!(&leaf.tag, zencodec::inventory::PartTag::Name(n) if n == "comment");
    line_like
        && ((data[at as usize] == b'\n' && at + 1 == leaf.range.end)
            || (data[at as usize] == b'#' && at == leaf.range.start))
}

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

// ── BMP ─────────────────────────────────────────────────────────────

/// OS/2 1.x (12-byte header), 8 bpp, 4-entry palette, 4x1. The decoder sizes
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Mutating any byte the inventory reports as unconsumed must not change the
/// decode result (pixels or error).
fn check_unconsumed_irrelevant<C: DecoderConfig + Clone>(
    cfg: C,
    data: &[u8],
    what: &str,
    fails: &mut Vec<String>,
) {
    let inv = inv_of(cfg.clone(), data);
    let base = decode(cfg.clone(), data);
    for at in 0..data.len() {
        if is_consumed_at(&inv, at as u64) || is_unit_framing(&inv, data, at as u64) {
            continue;
        }
        let mut m = data.to_vec();
        m[at] ^= 0x5A;
        let got = decode(cfg.clone(), &m);
        let same = match (&base, &got) {
            (Ok(a), Ok(b)) => a == b,
            (Err(_), _) => true,
            // Bytes of a stream the decoder parses but whose pixels it discards may
            // still turn a success into an error.
            (Ok(_), Err(_)) => part_at(&inv, at as u64).contains("parsed for validity"),
        };
        if !same {
            fails.push(format!(
                "{what}: byte {at} ({}) is unconsumed but changes decode: {:?} -> {:?}\nhex: {}\n{inv}",
                part_at(&inv, at as u64),
                base.as_ref().map(|d| d.2.len()),
                got.as_ref().map(|d| d.2.len()),
                data.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join("")
            ));
            return;
        }
    }
}

fn bmp_rle(rng: &mut Rng, depth: u16, w: i32, h: i32) -> Vec<u8> {
    let comp = if depth == 4 { 2u32 } else { 1 };
    let pal = if depth <= 8 { 1u32 << depth } else { 0 };
    let off = 54 + pal * 4;
    let mut v = b"BM".to_vec();
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(off));
    v.extend_from_slice(&le32(40));
    v.extend_from_slice(&w.to_le_bytes());
    v.extend_from_slice(&h.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&depth.to_le_bytes());
    v.extend_from_slice(&le32(comp));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(pal));
    v.extend_from_slice(&le32(0));
    for i in 0..pal {
        v.extend_from_slice(&[i as u8, (i * 3) as u8, (i * 7) as u8, 0]);
    }
    let n = rng.below(40) as usize;
    for _ in 0..n {
        match rng.below(6) {
            0 => v.extend_from_slice(&[0, 0]),
            1 => v.extend_from_slice(&[0, 2, rng.below(3) as u8, rng.below(2) as u8]),
            2 => {
                let k = 3 + rng.below(6) as u8;
                v.extend_from_slice(&[0, k]);
                for _ in 0..k {
                    v.push(rng.byte() & 0x33);
                }
            }
            _ => v.extend_from_slice(&[1 + rng.below(5) as u8, rng.byte() & 0x33]),
        }
    }
    if rng.below(2) == 0 {
        v.extend_from_slice(&[0, 1]);
    }
    for _ in 0..rng.below(8) {
        v.push(rng.byte());
    }
    v
}

fn check_bmp_policy(
    data: &[u8],
    policy: zencodec::decode::DecodePolicy,
    what: &str,
    fails: &mut Vec<String>,
) {
    let job = || BmpDecoderConfig::new().job().with_policy(policy);
    let inv = job().inventory(data).unwrap().unwrap();
    inv.validate()
        .unwrap_or_else(|e| panic!("{what}: invalid inventory: {e}\n{inv}"));
    let dec = |d: &[u8]| -> std::result::Result<Vec<u8>, String> {
        let out = job()
            .decoder(Cow::Borrowed(d), &[])
            .map_err(|e| e.to_string())?
            .decode()
            .map_err(|e| e.to_string())?;
        Ok(out.pixels().contiguous_bytes().into_owned())
    };
    let Ok(base) = dec(data) else { return };
    for at in 0..data.len() {
        if is_consumed_at(&inv, at as u64) || is_unit_framing(&inv, data, at as u64) {
            continue;
        }
        let mut m = data.to_vec();
        m[at] ^= 0x5A;
        let got = dec(&m);
        let ok = got.as_ref() == Ok(&base)
            || (got.is_err() && part_at(&inv, at as u64).contains("parsed for validity"));
        if !ok {
            fails.push(format!(
                "{what}: byte {at} ({}) is unconsumed but changes decode\nhex: {}\n{inv}",
                part_at(&inv, at as u64),
                data.iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join("")
            ));
            return;
        }
    }
}

// ── helpers for the per-finding tests ────────────────────────────────

fn leaf_at(inv: &Inventory, at: u64) -> &zencodec::inventory::Part {
    inv.parts()
        .iter()
        .filter(|p| p.range.start <= at && at < p.range.end)
        .min_by_key(|p| p.range.end - p.range.start)
        .unwrap()
}

fn find<'a>(inv: &'a Inventory, name: &str) -> Option<&'a zencodec::inventory::Part> {
    inv.parts()
        .iter()
        .find(|p| matches!(&p.tag, zencodec::inventory::PartTag::Name(n) if n == name))
}

/// 14-byte file header + info header of `ihsize` bytes (fields beyond the
/// common block zeroed, to be patched by the caller).
fn bmp_head(ihsize: u32, w: i32, h: i32, bpp: u16, comp: u32, clr: u32, off: u32) -> Vec<u8> {
    let mut v = b"BM".to_vec();
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(0));
    v.extend_from_slice(&le32(off));
    v.extend_from_slice(&le32(ihsize));
    if ihsize == 12 {
        v.extend_from_slice(&(w as u16).to_le_bytes());
        v.extend_from_slice(&(h as u16).to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&bpp.to_le_bytes());
        return v;
    }
    v.extend_from_slice(&w.to_le_bytes());
    v.extend_from_slice(&h.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&bpp.to_le_bytes());
    if ihsize >= 40 {
        v.extend_from_slice(&le32(comp));
        v.extend_from_slice(&le32(0));
        v.extend_from_slice(&le32(0));
        v.extend_from_slice(&le32(0));
        v.extend_from_slice(&le32(clr));
        v.extend_from_slice(&le32(0));
    }
    v.resize(14 + ihsize as usize, 0);
    v
}

// ── finding 2: TGA unreachable colour-map entries ────────────────────

#[test]
fn tga_unreachable_colour_map_entries_are_dropped() {
    let n: u16 = 300;
    let mut v = vec![0u8, 1, 1, 0, 0];
    v.extend_from_slice(&n.to_le_bytes());
    v.push(24);
    v.extend_from_slice(&[0, 0, 0, 0]);
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&[8, 0x20]);
    for i in 0..n {
        let b = (i % 251) as u8;
        v.extend_from_slice(&[b, b, b]);
    }
    let secret = 18 + 256 * 3;
    v[secret..secret + 6].copy_from_slice(b"SECRET");
    v.extend_from_slice(&[0, 255]);
    let cfg = TgaDecoderConfig::new;
    let inv = inv_of(cfg(), &v);
    let reachable = leaf_at(&inv, 18);
    assert_eq!(
        (reachable.range.clone(), reachable.disposition),
        (18..18 + 768, Disposition::Structure),
        "{inv}"
    );
    let rest = leaf_at(&inv, secret as u64);
    assert_eq!(rest.disposition, Disposition::Dropped, "{inv}");
    assert_eq!(rest.range, 18 + 768..18 + 900);
    assert!(
        rest.detail
            .as_deref()
            .unwrap()
            .contains("no 8-bit pixel index")
    );
    for at in secret..18 + 900 {
        assert!(!byte_matters(cfg(), &v, at), "byte {at}");
    }
    // A start offset shrinks the reachable prefix: entry k is index k + start.
    let mut w = v.clone();
    w[3..5].copy_from_slice(&200u16.to_le_bytes());
    let inv = inv_of(cfg(), &w);
    assert_eq!(leaf_at(&inv, 18).range, 18..18 + 56 * 3, "{inv}");
    assert_eq!(leaf_at(&inv, 18 + 56 * 3).disposition, Disposition::Dropped);
}

// ── finding 3: V5 profile pointing into the pixel array ──────────────

#[test]
fn bmp_v5_profile_inside_pixels_stays_pixel_data() {
    let mut s = bmp_head(124, 4, 2, 24, 0, 0, 138);
    s[14 + 56..14 + 60].copy_from_slice(&le32(0x4D42_4544));
    s[14 + 112..14 + 116].copy_from_slice(&le32(140 - 14));
    s[14 + 116..14 + 120].copy_from_slice(&le32(8));
    for i in 0..24u8 {
        s.push(i * 9 + 1);
    }
    let cfg = BmpDecoderConfig::new;
    let inv = inv_of(cfg(), &s);
    let px = find(&inv, "pixel-array").unwrap();
    assert_eq!(
        (px.range.clone(), px.disposition),
        (138..162, Disposition::ImageData),
        "{inv}"
    );
    assert!(
        px.detail
            .as_deref()
            .unwrap()
            .contains("overlaps other parts"),
        "{inv}"
    );
    assert!(find(&inv, "icc-profile").is_none());
    for at in 138..162 {
        assert!(is_consumed_at(&inv, at as u64), "byte {at}");
    }
    assert!(byte_matters(cfg(), &s, 143));
}

// ── finding 4: RLE skip that fails near the end of the file ──────────

#[test]
fn bmp_rle_failed_skip_keeps_reading() {
    let mut v = bmp_head(40, 4, 1, 32, 1, 0, 54);
    v.extend_from_slice(&[0, 3, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    v.extend_from_slice(&[0, 0x20]); // absolute run that overflows; skip(8) fails
    v.extend_from_slice(&[1, 0xAA, 0xBB, 0xCC, 0xDD]);
    let cfg = BmpDecoderConfig::new;
    let inv = inv_of(cfg(), &v);
    let at = v.len() - 5;
    let mut m = v.clone();
    m[at] = 0x5B;
    assert_ne!(
        decode(cfg(), &v),
        decode(cfg(), &m),
        "the byte changes the result"
    );
    assert!(
        inv.parts()
            .iter()
            .all(|p| p.disposition != Disposition::Trailing),
        "nothing is trailing:\n{inv}"
    );
    assert_eq!(
        image_data_end(&inv),
        None,
        "RLE without a palette is Dropped, not ImageData\n{inv}"
    );
    let px = find(&inv, "pixel-array").unwrap();
    assert_eq!(px.range.end, v.len() as u64, "{inv}");
}

// ── finding 5: RLE output discarded without a colour table ───────────

#[test]
fn bmp_rle_without_palette_is_dropped() {
    for (bpp, clr, expect) in [
        (32u16, 0u32, Disposition::Dropped),
        (8, 0, Disposition::Dropped),
        (8, 2, Disposition::ImageData),
    ] {
        let mut v = bmp_head(40, 2, 1, bpp, 1, clr, 54 + clr * 4);
        for i in 0..clr {
            v.extend_from_slice(&[0x40 * (i as u8 + 1), 0x30, 0x20, 0]);
        }
        if bpp == 32 {
            v.extend_from_slice(&[2, 0x11, 0x22, 0x33, 0x44, 0, 1]);
        } else {
            v.extend_from_slice(&[2, 1, 0, 1]);
        }
        let inv = inv_of(BmpDecoderConfig::new(), &v);
        let px = find(&inv, "pixel-array").unwrap();
        assert_eq!(px.disposition, expect, "bpp {bpp} clr {clr}\n{inv}");
        if expect == Disposition::Dropped {
            assert!(px.detail.as_deref().unwrap().contains("zero-filled"));
            let out = decode(BmpDecoderConfig::new(), &v).unwrap().2;
            assert!(
                out.iter().all(|&b| b == 0),
                "output is zero-filled: {out:?}"
            );
        }
    }
}

// ── finding 6: OS/2 colour table as the decoder counts it ────────────

#[test]
fn bmp_os2_palette_follows_the_decoder() {
    let mut v = bmp_head(12, 4, 1, 8, 0, 0, 38);
    for c in 0..4u8 {
        v.extend_from_slice(&[c * 10, c * 20, c * 30]);
    }
    v.extend_from_slice(&[0, 1, 2, 3]);
    v.resize(900, 0x11);
    v[794..798].copy_from_slice(&[10, 11, 12, 13]);
    let cfg = BmpDecoderConfig::new;
    let inv = inv_of(cfg(), &v);
    let t = find(&inv, "colour-table").unwrap();
    assert_eq!(
        (t.range.clone(), t.disposition),
        (26..26 + 768, Disposition::Structure),
        "{inv}"
    );
    assert_eq!(find(&inv, "pixel-array").unwrap().range.start, 794);
    assert!(is_consumed_at(&inv, 56));
    assert!(byte_matters(cfg(), &v, 56));
}

#[test]
fn bmp_16_byte_header_palette_has_four_byte_entries() {
    let mut v = bmp_head(16, 4, 1, 8, 0, 0, 100);
    v.resize(30 + 1024 + 8, 0x22);
    let cfg = BmpDecoderConfig::new;
    let inv = inv_of(cfg(), &v);
    let t = find(&inv, "colour-table").unwrap();
    assert_eq!(t.range, 30..30 + 1024, "{inv}");
}

// ── finding 7: paletted + BITFIELDS ──────────────────────────────────

#[test]
fn bmp_paletted_bitfields_reports_the_palette() {
    let mut v = bmp_head(40, 4, 1, 8, 3, 4, 70);
    for c in 0..4u8 {
        v.extend_from_slice(&[c * 10 + 1, c * 20 + 2, c * 30 + 3, 0]);
    }
    v.extend_from_slice(&[0, 1, 2, 3]);
    let cfg = BmpDecoderConfig::new;
    let inv = inv_of(cfg(), &v);
    let t = find(&inv, "colour-table").unwrap();
    assert_eq!(
        (t.range.clone(), t.disposition),
        (54..70, Disposition::Structure),
        "{inv}"
    );
    assert!(find(&inv, "masks").is_none(), "{inv}");
    for at in [55u64, 67] {
        assert!(is_consumed_at(&inv, at));
        assert!(byte_matters(cfg(), &v, at as usize));
    }
}

#[test]
fn inventory_regression_seed_is_a_palette_over_masks() {
    let seed = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fuzz/regression/inventory/bmp-colour-table-overlaps-masks"
    ))
    .unwrap();
    inv_of(BmpDecoderConfig::new(), &seed);
}

// ── finding 8: 52-byte header ────────────────────────────────────────

#[test]
fn bmp_header52_alpha_mask_and_hole_split() {
    let mut v = bmp_head(52, 2, 1, 32, 3, 0, 200);
    v[14 + 40..14 + 52].copy_from_slice(&[0, 0, 0xFF, 0, 0, 0xFF, 0, 0, 0xFF, 0, 0, 0]);
    v.extend_from_slice(&le32(0xFF00_0000)); // 66..70
    v.resize(200, 0x33);
    v.extend_from_slice(&[1, 2, 3, 0x40, 4, 5, 6, 0x80]);
    let cfg = BmpDecoderConfig::new;
    let inv = inv_of(cfg(), &v);
    let a = find(&inv, "alpha-mask").unwrap();
    assert_eq!(
        (a.range.clone(), a.disposition),
        (66..70, Disposition::Structure),
        "{inv}"
    );
    assert_eq!(leaf_at(&inv, 100).disposition, Disposition::Skipped);
    assert_eq!(leaf_at(&inv, 100).range, 70..122, "{inv}");
    let rest = leaf_at(&inv, 150);
    assert_eq!(
        (rest.range.clone(), rest.disposition),
        (122..200, Disposition::Unreferenced),
        "{inv}"
    );
    assert!(byte_matters(cfg(), &v, 69));
}

// ── finding 9: Strict validates the densities ────────────────────────

#[test]
fn bmp_strict_zero_resolution_is_structure() {
    let mut v = bmp_head(40, 1, 1, 24, 0, 0, 54);
    v.extend_from_slice(&[1, 2, 3, 0]);
    let strict = zencodec::decode::DecodePolicy::none().with_strict(true);
    let inv = BmpDecoderConfig::new()
        .job()
        .with_policy(strict)
        .inventory(&v)
        .unwrap()
        .unwrap();
    inv.validate().unwrap();
    assert_eq!(
        leaf_at(&inv, 14 + 24).disposition,
        Disposition::Structure,
        "{inv}"
    );
    let default_inv = inv_of(BmpDecoderConfig::new(), &v);
    assert_eq!(
        leaf_at(&default_inv, 14 + 24).disposition,
        Disposition::Dropped
    );
    // And a flipped density really does fail under Strict.
    let mut m = v.clone();
    m[14 + 26] = 0x5A;
    let job = || BmpDecoderConfig::new().job().with_policy(strict);
    assert!(
        job()
            .decoder(Cow::Borrowed(&v), &[])
            .unwrap()
            .decode()
            .is_ok()
    );
    assert!(
        job()
            .decoder(Cow::Borrowed(&m), &[])
            .unwrap()
            .decode()
            .is_err()
    );
}

// ── finding 10: leading zeros in ASCII samples ───────────────────────

#[test]
fn pnm_ascii_leading_zero_sample_is_image_data() {
    let v = b"P2\n2 1\n255\n000000000007 9\n";
    let inv = inv_of(PnmDecoderConfig::new(), v);
    assert_eq!(decode(PnmDecoderConfig::new(), v).unwrap().2, [7, 9]);
    let px = find(&inv, "pixels").unwrap();
    assert_eq!(
        (px.range.clone(), px.disposition),
        (11..25, Disposition::ImageData),
        "{inv}"
    );
    assert!(
        inv.parts()
            .iter()
            .all(|p| p.disposition != Disposition::Malformed)
    );
}

// ── finding 11: colors-used above 8 bpp ──────────────────────────────

#[test]
fn bmp_truecolor_colors_used_is_dropped() {
    let mut v = bmp_head(40, 1, 1, 24, 0, 0, 54);
    v.extend_from_slice(&[1, 2, 3, 0]);
    let inv = inv_of(BmpDecoderConfig::new(), &v);
    let f = leaf_at(&inv, 14 + 32);
    assert_eq!(f.disposition, Disposition::Dropped, "{inv}");
    assert!(f.detail.as_deref().unwrap().contains("never uses"));
    assert!(!byte_matters(BmpDecoderConfig::new(), &v, 14 + 32));
}

// ── finding 12: memory bounded by the part cap ───────────────────────

#[test]
fn pnm_comment_flood_stops_at_the_part_cap() {
    let mut v = b"P5\n".to_vec();
    v.extend(std::iter::repeat_n(*b"#\n", 3 << 19).flatten());
    let e = PnmDecoderConfig::new().job().inventory(&v).unwrap_err();
    assert!(e.to_string().contains("inventory exceeds"), "{e}");
}

// ── minors ───────────────────────────────────────────────────────────

#[test]
fn pnm_byte_after_the_last_header_number_is_dropped() {
    let v = b"P5 2 1 255X\x01\x02";
    let inv = inv_of(PnmDecoderConfig::new(), v);
    let sep = leaf_at(&inv, 10);
    assert_eq!(
        (sep.range.clone(), sep.disposition),
        (10..11, Disposition::Dropped),
        "{inv}"
    );
    assert!(!byte_matters(PnmDecoderConfig::new(), v, 10));
}

#[test]
fn bmp_rejected_file_keeps_headers_and_is_malformed_after() {
    let mut v = bmp_head(40, 1, 1, 24, 0, 0, 54);
    v[14 + 12..14 + 14].copy_from_slice(&7u16.to_le_bytes()); // planes = 7
    v.extend_from_slice(&[1, 2, 3, 0]);
    assert!(decode(BmpDecoderConfig::new(), &v).is_err());
    let inv = inv_of(BmpDecoderConfig::new(), &v);
    let tail = leaf_at(&inv, 54);
    assert_eq!(tail.disposition, Disposition::Malformed, "{inv}");
    assert!(
        inv.parts()
            .iter()
            .all(|p| p.disposition != Disposition::ImageData)
    );
}

// ── finding 13: randomized byte-flip property ────────────────────────

#[test]
fn randomized_unconsumed_bytes_never_matter() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut fails = Vec::new();
    let perm = zencodec::decode::DecodePolicy::permissive;
    let strict = || zencodec::decode::DecodePolicy::none().with_strict(true);
    for case in 0..400 {
        let depth = [4u16, 8, 16, 32][rng.below(4) as usize];
        let (w, h) = (1 + rng.below(6) as i32, 1 + rng.below(4) as i32);
        let v = bmp_rle(&mut rng, depth, w, h);
        check_unconsumed_irrelevant(
            BmpDecoderConfig::new(),
            &v,
            &format!("bmp-rle#{case}"),
            &mut fails,
        );
        check_bmp_policy(&v, perm(), &format!("bmp-rle-perm#{case}"), &mut fails);
        check_bmp_policy(&v, strict(), &format!("bmp-rle-strict#{case}"), &mut fails);
    }
    for case in 0..400 {
        let bpp = [1u16, 2, 4, 8, 16, 24, 32][rng.below(7) as usize];
        let (w, h) = (1 + rng.below(7) as i32, 1 + rng.below(4) as i32);
        let comp = if matches!(bpp, 16 | 32) && rng.below(2) == 0 {
            3u32
        } else {
            0
        };
        let pal = if bpp <= 8 {
            (1u32 << bpp).min(1 + rng.below(1 << bpp) as u32)
        } else {
            rng.below(3) as u32
        };
        let extra = if comp == 3 { 12 } else { 0 };
        let off = 54 + extra + pal * 4 + rng.below(6) as u32;
        let mut v = bmp_head(
            40,
            w,
            if rng.below(2) == 0 { h } else { -h },
            bpp,
            comp,
            pal,
            off,
        );
        v[14 + 24..14 + 28].copy_from_slice(&le32(rng.below(2) as u32 * 2835));
        if comp == 3 {
            for m in [0xF800u32, 0x07E0, 0x001F] {
                v.extend_from_slice(&le32(m));
            }
        }
        for i in 0..pal {
            v.extend_from_slice(&[i as u8, (i * 3) as u8, (i * 7) as u8, 0x77]);
        }
        while (v.len() as u32) < off {
            v.push(0xEE);
        }
        let stride = ((w as u32 * bpp as u32).div_ceil(32) * 4) as usize;
        for _ in 0..stride * h as usize {
            let b = rng.byte();
            v.push(if bpp <= 8 {
                b % (pal.max(1) as u8).max(1)
            } else {
                b
            });
        }
        for _ in 0..rng.below(6) {
            v.push(rng.byte());
        }
        check_unconsumed_irrelevant(
            BmpDecoderConfig::new(),
            &v,
            &format!("bmp-raw#{case}"),
            &mut fails,
        );
        check_bmp_policy(&v, perm(), &format!("bmp-raw-perm#{case}"), &mut fails);
        check_bmp_policy(&v, strict(), &format!("bmp-raw-strict#{case}"), &mut fails);
    }
    for case in 0..200 {
        let (w, h) = (1 + rng.below(5) as u16, 1 + rng.below(4) as u16);
        let mut v = vec![rng.below(3) as u8, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.extend_from_slice(&[24, 0x20]);
        let id_len = v[0] as usize;
        v.resize(v.len() + id_len, b'I');
        for _ in 0..rng.below(12) {
            let hd = rng.byte() & 0x87;
            v.push(hd);
            let n = if hd & 0x80 != 0 {
                1
            } else {
                (hd & 0x7F) as usize + 1
            };
            for _ in 0..n * 3 {
                v.push(rng.byte());
            }
        }
        for _ in 0..rng.below(6) {
            v.push(rng.byte());
        }
        check_unconsumed_irrelevant(
            TgaDecoderConfig::new(),
            &v,
            &format!("tga-rle#{case}"),
            &mut fails,
        );
    }
    for case in 0..200 {
        let (w, h) = (1 + rng.below(5) as u32, 1 + rng.below(4) as u32);
        let mut v = b"qoif".to_vec();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[3 + rng.below(2) as u8, rng.below(2) as u8]);
        for _ in 0..rng.below(14) {
            match rng.below(6) {
                0 => v.extend_from_slice(&[0xFE, rng.byte(), rng.byte(), rng.byte()]),
                1 => v.extend_from_slice(&[0xFF, rng.byte(), rng.byte(), rng.byte(), rng.byte()]),
                2 => v.extend_from_slice(&[0x80 | (rng.byte() & 0x3F), rng.byte()]),
                3 => v.push(0xC0 | rng.below(8) as u8),
                _ => v.push(rng.byte() & 0x7F),
            }
        }
        if rng.below(2) == 0 {
            v.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
        }
        for _ in 0..rng.below(6) {
            v.push(rng.byte());
        }
        check_unconsumed_irrelevant(
            QoiDecoderConfig::new(),
            &v,
            &format!("qoi#{case}"),
            &mut fails,
        );
    }
    for case in 0..150 {
        let (w, h) = (8 + rng.below(8) as u32, 1 + rng.below(3) as u32);
        let mut v = format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y {h} +X {w}\n").into_bytes();
        for _ in 0..h {
            v.extend_from_slice(&[2, 2, 0, w as u8]);
            for _ in 0..4 {
                let mut col = 0;
                while col < w {
                    let k = 1 + rng.below(u64::from((w - col).min(5))) as u32;
                    if rng.below(2) == 0 {
                        v.extend_from_slice(&[128 + k as u8, rng.byte()]);
                    } else {
                        v.push(k as u8);
                        for _ in 0..k {
                            v.push(rng.byte());
                        }
                    }
                    col += k;
                }
            }
        }
        for _ in 0..rng.below(6) {
            v.push(rng.byte());
        }
        check_unconsumed_irrelevant(
            HdrDecoderConfig::new(),
            &v,
            &format!("hdr#{case}"),
            &mut fails,
        );
    }
    for case in 0..150 {
        let (w, h) = (1 + rng.below(3), 1 + rng.below(3));
        let mut s = format!("P2\n# c\n{w} {h}\n255\n");
        for _ in 0..w * h + rng.below(2) {
            if rng.below(4) == 0 {
                s.push_str("#x\n");
            }
            s.push_str(&format!("{} ", rng.below(300)));
        }
        let mut v = s.into_bytes();
        if rng.below(3) == 0 {
            let l = v.len() - rng.below(4) as usize;
            v.truncate(l);
        }
        check_unconsumed_irrelevant(
            PnmDecoderConfig::new(),
            &v,
            &format!("pnm#{case}"),
            &mut fails,
        );
    }
    assert!(
        fails.is_empty(),
        "{} failing cases, first:\n{}",
        fails.len(),
        fails[0]
    );
}

fn image_data_end(inv: &Inventory) -> Option<u64> {
    inv.parts()
        .iter()
        .filter(|p| p.parent.is_none() && p.disposition == Disposition::ImageData)
        .map(|p| p.range.end)
        .max()
}
