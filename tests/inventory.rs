//! Structural inventories (`DecodeJob::inventory`) for every zenbitmaps decoder.
//!
//! - `check_inventory` on encoder output and on hand-built fixtures;
//! - pinned part lists for fixtures that contain every unit type;
//! - the whole bmp/pnm/farbfeld conformance corpus: coverage plus
//!   prefix properties that tie the walkers' pixel extents to what the
//!   decoders really consume.
#![cfg(feature = "zencodec")]

use std::borrow::Cow;

use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
use zencodec::inventory::{Disposition, Inventory, PartId, PartKind};
use zenpixels::{PixelDescriptor, PixelSlice};

// ── helpers ──────────────────────────────────────────────────────────

fn inventory_of<C: DecoderConfig>(cfg: C, data: &[u8]) -> Inventory {
    let inv = cfg
        .job()
        .inventory(data)
        .unwrap_or_else(|e| panic!("inventory failed: {e}"))
        .expect("decoder declares the inventory capability");
    inv.validate()
        .unwrap_or_else(|e| panic!("invalid inventory: {e}\n{inv}"));
    inv
}

fn render_level(inv: &Inventory, parent: Option<PartId>, depth: usize, out: &mut Vec<String>) {
    for id in inv.children(parent) {
        let p = inv.get(id).unwrap();
        let mut s = format!(
            "{}{} {} {}..{} {}",
            "  ".repeat(depth),
            p.kind.name(),
            p.tag,
            p.range.start,
            p.range.end,
            p.disposition
        );
        if let Some(l) = &p.label {
            s.push_str(&format!(" {l:?}"));
        }
        out.push(s);
        render_level(inv, Some(id), depth + 1, out);
    }
}

fn render(inv: &Inventory) -> Vec<String> {
    let mut out = Vec::new();
    render_level(inv, None, 0, &mut out);
    out
}

fn assert_parts(inv: &Inventory, expected: &[&str]) {
    let got = render(inv);
    let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        got,
        want,
        "\n--- got ---\n{}\n--- inventory ---\n{inv}",
        got.join("\n")
    );
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

fn encode<C: EncoderConfig>(cfg: C, desc: PixelDescriptor, bpp: usize) -> Vec<u8>
where
    <C::Job as EncodeJob>::Enc: Encoder<Error = C::Error>,
{
    let (w, h) = (8u32, 8u32);
    let bytes: Vec<u8> = (0..w as usize * h as usize * bpp)
        .map(|i| (i * 7 % 251) as u8)
        .collect();
    let slice = PixelSlice::new(&bytes, w, h, w as usize * bpp, desc).expect("slice");
    cfg.job()
        .encoder()
        .expect("encoder")
        .encode(slice)
        .expect("encode")
        .into_vec()
}

/// End of the last top-level `ImageData` part.
fn pixel_end(inv: &Inventory) -> u64 {
    inv.parts()
        .iter()
        .filter(|p| p.parent.is_none() && p.disposition == Disposition::ImageData)
        .map(|p| p.range.end)
        .max()
        .expect("an ImageData part")
}

/// The pixel-extent properties for a file the decoder accepts:
/// - cutting the file at the end of the ImageData still decodes to the same
///   pixels (the walker did not stop early);
/// - appending junk changes nothing (nothing after the ImageData is read);
/// - when `necessary_slack` is `Some(n)`, cutting `n` bytes before the end no
///   longer decodes to the same pixels (the walker did not run long).
fn check_extent<C: DecoderConfig + Clone>(
    cfg: C,
    data: &[u8],
    necessary_slack: Option<u64>,
    what: &str,
) {
    let Ok(full) = decode_pixels(cfg.clone(), data) else {
        return;
    };
    let inv = inventory_of(cfg.clone(), data);
    let end = pixel_end(&inv);
    let cut = decode_pixels(cfg.clone(), &data[..end as usize]).unwrap_or_else(|e| {
        panic!("{what}: cutting at the ImageData end ({end}) breaks decoding: {e}\n{inv}")
    });
    assert_eq!(cut, full, "{what}: prefix decodes differently\n{inv}");
    let mut junked = data[..end as usize].to_vec();
    junked.extend((0..53u8).map(|i| i.wrapping_mul(31) ^ 0xA7));
    let j = decode_pixels(cfg.clone(), &junked)
        .unwrap_or_else(|e| panic!("{what}: junk after the ImageData breaks decoding: {e}"));
    assert_eq!(j, full, "{what}: junk after the ImageData changes pixels");
    if let Some(slack) = necessary_slack
        && end > slack
    {
        let short = &data[..(end - slack.max(1)) as usize];
        match decode_pixels(cfg, short) {
            Err(_) => {}
            Ok(d) => assert_ne!(
                d, full,
                "{what}: the last {slack} byte(s) of the ImageData are not needed\n{inv}"
            ),
        }
    }
}

// ── encoder output ───────────────────────────────────────────────────

#[test]
fn encoder_output_passes_check_inventory() {
    use zenbitmaps::*;
    let rgb = PixelDescriptor::RGB8_SRGB;
    zencodec_testkit::check_inventory(
        PnmDecoderConfig::new(),
        &encode(PnmEncoderConfig::new(), rgb, 3),
    )
    .unwrap();
    zencodec_testkit::check_inventory(
        PnmDecoderConfig::new(),
        &encode(PnmEncoderConfig::new(), PixelDescriptor::RGBA8_SRGB, 4),
    )
    .unwrap();
    zencodec_testkit::check_inventory(
        PnmDecoderConfig::new(),
        &encode(PnmEncoderConfig::new(), PixelDescriptor::GRAY8_SRGB, 1),
    )
    .unwrap();
    zencodec_testkit::check_inventory(
        FarbfeldDecoderConfig::new(),
        &encode(FarbfeldEncoderConfig::new(), rgb, 3),
    )
    .unwrap();
    #[cfg(feature = "bmp")]
    {
        zencodec_testkit::check_inventory(
            BmpDecoderConfig::new(),
            &encode(BmpEncoderConfig::new(), rgb, 3),
        )
        .unwrap();
        zencodec_testkit::check_inventory(
            BmpDecoderConfig::new(),
            &encode(BmpEncoderConfig::new(), PixelDescriptor::RGBA8_SRGB, 4),
        )
        .unwrap();
    }
    #[cfg(feature = "qoi")]
    zencodec_testkit::check_inventory(
        QoiDecoderConfig::new(),
        &encode(QoiEncoderConfig::new(), rgb, 3),
    )
    .unwrap();
    #[cfg(feature = "tga")]
    zencodec_testkit::check_inventory(
        TgaDecoderConfig::new(),
        &encode(TgaEncoderConfig::new(), rgb, 3),
    )
    .unwrap();
    #[cfg(feature = "hdr")]
    zencodec_testkit::check_inventory(
        HdrDecoderConfig::new(),
        &encode(HdrEncoderConfig::new(), PixelDescriptor::RGBF32_LINEAR, 12),
    )
    .unwrap();
}

#[test]
fn every_decoder_declares_the_capability() {
    use zenbitmaps::*;
    assert!(PnmDecoderConfig::capabilities().inventory());
    assert!(FarbfeldDecoderConfig::capabilities().inventory());
    #[cfg(feature = "bmp")]
    assert!(BmpDecoderConfig::capabilities().inventory());
    #[cfg(feature = "qoi")]
    assert!(QoiDecoderConfig::capabilities().inventory());
    #[cfg(feature = "tga")]
    assert!(TgaDecoderConfig::capabilities().inventory());
    #[cfg(feature = "hdr")]
    assert!(HdrDecoderConfig::capabilities().inventory());
}

// ── PNM ──────────────────────────────────────────────────────────────

fn pnm_fixture() -> Vec<u8> {
    let mut v = b"P6\n# first comment\n2 1\n#second\n255\n".to_vec();
    v.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
    v.extend_from_slice(b"P6\n1 1\n255\n\x07\x08\x09");
    v
}

#[test]
fn pnm_pinned_comments_and_concatenated_image() {
    let data = pnm_fixture();
    let inv = inventory_of(zenbitmaps::PnmDecoderConfig::new(), &data);
    assert_parts(
        &inv,
        &[
            "header - 0..35 structure \"P6\"",
            "  block comment 3..18 skipped \" first comment\"",
            "  block comment 23..30 skipped \"second\"",
            "block pixels 35..41 image-data",
            "trailer - 41..55 trailing",
        ],
    );
    zencodec_testkit::check_inventory(zenbitmaps::PnmDecoderConfig::new(), &data).unwrap();
    let trailing = inv
        .parts()
        .iter()
        .find(|p| p.kind == PartKind::Trailer)
        .unwrap();
    assert!(
        trailing
            .detail
            .as_deref()
            .unwrap()
            .contains("concatenated PNM image")
    );
}

#[test]
fn pnm_pinned_pam_header_lines() {
    let mut v = b"P7\n# made by a tool\nWIDTH 1\nHEIGHT 1\nDEPTH 3\nMAXVAL 255\nTUPLTYPE RGB\nSERIAL 0042\nWIDTH 2\nENDHDR\n"
        .to_vec();
    v.extend_from_slice(&[9; 6]);
    let inv = inventory_of(zenbitmaps::PnmDecoderConfig::new(), &v);
    assert_parts(
        &inv,
        &[
            "header - 0..96 structure \"P7\"",
            "  block comment 3..19 skipped \" made by a tool\"",
            "  attribute WIDTH 20..27 dropped",
            "  attribute HEIGHT 28..36 structure",
            "  attribute DEPTH 37..44 structure",
            "  attribute MAXVAL 45..55 structure",
            "  attribute TUPLTYPE 56..68 dropped \"RGB\"",
            "  attribute unknown 69..80 unknown \"SERIAL 0042\"",
            "  attribute WIDTH 81..88 structure",
            "  attribute ENDHDR 89..95 structure",
            "block pixels 96..102 image-data",
        ],
    );
}

#[test]
fn pnm_pinned_ascii_comment_in_samples() {
    let v = b"P3\n2 1\n255\n1 2 3 # note in the data\n4 5 6\n trailing words";
    let inv = inventory_of(zenbitmaps::PnmDecoderConfig::new(), v);
    assert_parts(
        &inv,
        &[
            "header - 0..11 structure \"P3\"",
            "block pixels 11..41 image-data",
            "  block comment 17..35 skipped \" note in the data\"",
            "trailer - 41..57 trailing",
        ],
    );
}

// ── farbfeld ─────────────────────────────────────────────────────────

#[test]
fn farbfeld_pinned_trailing_data() {
    let mut v = b"farbfeld".to_vec();
    v.extend_from_slice(&1u32.to_be_bytes());
    v.extend_from_slice(&1u32.to_be_bytes());
    v.extend_from_slice(&[0, 1, 0, 2, 0, 3, 0xFF, 0xFF]);
    v.extend_from_slice(b"EXIF-ish trailer");
    let inv = inventory_of(zenbitmaps::FarbfeldDecoderConfig::new(), &v);
    assert_parts(
        &inv,
        &[
            "header - 0..16 structure \"farbfeld\"",
            "block pixels 16..24 image-data",
            "trailer - 24..40 trailing",
        ],
    );
    zencodec_testkit::check_inventory(zenbitmaps::FarbfeldDecoderConfig::new(), &v).unwrap();
}

// ── QOI ──────────────────────────────────────────────────────────────

#[cfg(feature = "qoi")]
#[test]
fn qoi_pinned_end_marker_and_trailing_data() {
    let mut v = b"qoif".to_vec();
    v.extend_from_slice(&2u32.to_be_bytes());
    v.extend_from_slice(&2u32.to_be_bytes());
    v.extend_from_slice(&[3, 1]); // RGB, linear
    v.extend_from_slice(&[0xFE, 10, 20, 30, 0xC2]); // RGB op, then a run of 3 more pixels
    v.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
    v.extend_from_slice(b"extra");
    let cfg = zenbitmaps::QoiDecoderConfig::new;
    let inv = inventory_of(cfg(), &v);
    assert_parts(
        &inv,
        &[
            "header - 0..14 structure \"qoif\"",
            "  field colorspace 13..14 metadata(cicp)",
            "block pixels 14..19 image-data",
            "block end-marker 19..27 skipped",
            "trailer - 27..32 trailing",
        ],
    );
    zencodec_testkit::check_inventory(cfg(), &v).unwrap();
    check_extent(cfg(), &v, Some(1), "qoi fixture");
}

// ── HDR ──────────────────────────────────────────────────────────────

#[cfg(feature = "hdr")]
#[test]
fn hdr_pinned_header_lines() {
    let mut v = b"#?RADIANCE
FORMAT=32-bit_rle_rgbe
EXPOSURE=1.0
SOFTWARE=pfilt 5.1 by someone
# a comment

-Y 1 +X 2
"
    .to_vec();
    v.extend_from_slice(&[128, 128, 128, 129, 64, 64, 64, 128]);
    v.extend_from_slice(b"junk");
    let cfg = zenbitmaps::HdrDecoderConfig::new;
    let inv = inventory_of(cfg(), &v);
    assert_parts(
        &inv,
        &[
            "header - 0..11 structure \"#?RADIANCE\"",
            "attribute FORMAT 11..34 skipped \"FORMAT=32-bit_rle_rgbe\"",
            "attribute EXPOSURE 34..47 skipped \"EXPOSURE=1.0\"",
            "attribute SOFTWARE 47..77 skipped \"SOFTWARE=pfilt 5.1 by someone\"",
            "attribute comment 77..89 skipped \"# a comment\"",
            "header end-of-header 89..90 structure",
            "header resolution 90..100 structure \"-Y 1 +X 2\"",
            "block pixels 100..108 image-data",
            "trailer - 108..112 trailing",
        ],
    );
    zencodec_testkit::check_inventory(cfg(), &v).unwrap();
    check_extent(cfg(), &v, Some(1), "hdr fixture");
}

// ── TGA ──────────────────────────────────────────────────────────────

#[cfg(feature = "tga")]
fn tga_fixture() -> Vec<u8> {
    let mut v = vec![5, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0x20];
    v.extend_from_slice(b"hello"); // image ID
    v.extend_from_slice(&[1, 2, 3, 4, 5, 6]); // 2x1 BGR
    v.extend_from_slice(b"DEVD"); // developer field data
    // developer directory: one entry (tag 0x1234, offset 29, size 4)
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&0x1234u16.to_le_bytes());
    v.extend_from_slice(&29u32.to_le_bytes());
    v.extend_from_slice(&4u32.to_le_bytes());
    // extension area
    let mut ext = vec![0u8; 495];
    ext[..2].copy_from_slice(&495u16.to_le_bytes());
    ext[2..10].copy_from_slice(b"Jane Doe");
    ext[43..55].copy_from_slice(b"private note");
    ext[426..433].copy_from_slice(b"EditorX");
    v.extend_from_slice(&ext);
    // footer: extension offset 45, developer directory offset 33
    v.extend_from_slice(&45u32.to_le_bytes());
    v.extend_from_slice(&33u32.to_le_bytes());
    v.extend_from_slice(b"TRUEVISION-XFILE.\0");
    v
}

#[cfg(feature = "tga")]
#[test]
fn tga_pinned_id_extension_developer_footer() {
    let v = tga_fixture();
    let cfg = zenbitmaps::TgaDecoderConfig::new;
    let inv = inventory_of(cfg(), &v);
    assert_parts(
        &inv,
        &[
            "header - 0..18 structure",
            "  field origin 8..12 dropped",
            "block image-id 18..23 skipped \"hello\"",
            "block pixels 23..29 image-data",
            "block 0x1234 29..33 skipped",
            "block developer-directory 33..45 skipped",
            "block extension-area 45..540 skipped",
            "  field author 47..88 skipped \"Jane Doe\"",
            "  field comments 88..412 skipped \"private note\"",
            "  field software-id 471..512 skipped \"EditorX\"",
            "block footer 540..566 skipped \"TRUEVISION-XFILE\"",
        ],
    );
    zencodec_testkit::check_inventory(cfg(), &v).unwrap();
    check_extent(cfg(), &v, Some(1), "tga fixture");
}

// ── BMP ──────────────────────────────────────────────────────────────

#[cfg(feature = "bmp")]
mod bmp {
    use super::*;
    use zenbitmaps::BmpDecoderConfig;

    pub(super) struct Spec {
        pub ihsize: u32,
        pub w: i32,
        pub h: i32,
        pub bpp: u16,
        pub comp: u32,
        pub clr_used: u32,
        pub off: u32,
        pub masks: [u32; 4],
        pub cstype: u32,
        pub profile_off: u32,
        pub profile_size: u32,
    }

    impl Spec {
        pub(super) fn new(ihsize: u32, w: i32, h: i32, bpp: u16) -> Self {
            Self {
                ihsize,
                w,
                h,
                bpp,
                comp: 0,
                clr_used: 0,
                off: 14 + ihsize,
                masks: [0; 4],
                cstype: 0,
                profile_off: 0,
                profile_size: 0,
            }
        }

        /// File header plus info header.
        pub(super) fn header(&self) -> Vec<u8> {
            let mut v = b"BM".to_vec();
            v.extend_from_slice(&[0; 4]); // file size, left zero
            v.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // reserved
            v.extend_from_slice(&self.off.to_le_bytes());
            v.extend_from_slice(&self.ihsize.to_le_bytes());
            if self.ihsize == 12 {
                v.extend_from_slice(&(self.w as u16).to_le_bytes());
                v.extend_from_slice(&(self.h as u16).to_le_bytes());
                v.extend_from_slice(&1u16.to_le_bytes());
                v.extend_from_slice(&self.bpp.to_le_bytes());
                return v;
            }
            v.extend_from_slice(&self.w.to_le_bytes());
            v.extend_from_slice(&self.h.to_le_bytes());
            v.extend_from_slice(&1u16.to_le_bytes());
            v.extend_from_slice(&self.bpp.to_le_bytes());
            if self.ihsize >= 40 {
                v.extend_from_slice(&self.comp.to_le_bytes());
                v.extend_from_slice(&0u32.to_le_bytes()); // image size
                v.extend_from_slice(&3780u32.to_le_bytes()); // x ppm
                v.extend_from_slice(&3781u32.to_le_bytes()); // y ppm
                v.extend_from_slice(&self.clr_used.to_le_bytes());
                v.extend_from_slice(&0u32.to_le_bytes()); // important
            }
            if self.ihsize >= 52 {
                for m in &self.masks[..3] {
                    v.extend_from_slice(&m.to_le_bytes());
                }
            }
            if self.ihsize >= 56 {
                v.extend_from_slice(&self.masks[3].to_le_bytes());
            }
            if self.ihsize >= 108 {
                v.extend_from_slice(&self.cstype.to_le_bytes());
                v.extend_from_slice(&[0; 48]); // endpoints + gamma
            }
            if self.ihsize >= 124 {
                v.extend_from_slice(&[0; 4]); // intent
                v.extend_from_slice(&self.profile_off.to_le_bytes());
                v.extend_from_slice(&self.profile_size.to_le_bytes());
                v.extend_from_slice(&[0; 4]);
            }
            assert_eq!(v.len(), 14 + self.ihsize as usize);
            v
        }
    }

    /// A V5 file with an embedded profile after the pixels, a gap before the
    /// pixels and trailing junk.
    pub(super) fn v5_with_profile() -> Vec<u8> {
        let mut s = Spec::new(124, 2, 2, 24);
        s.off = 142;
        s.cstype = 0x4D42_4544; // 'MBED'
        s.profile_off = 158 - 14;
        s.profile_size = 12;
        let mut v = s.header();
        v.extend_from_slice(b"GAP!"); // 138..142
        v.extend_from_slice(&[
            10, 20, 30, 40, 50, 60, 0, 0, 70, 80, 90, 100, 110, 120, 0, 0,
        ]); // 142..158
        v.extend_from_slice(b"ICCPROFILE.."); // 158..170
        v.extend_from_slice(b"JUNK!"); // 170..175
        v
    }

    pub(super) fn v5_with_linked_profile() -> Vec<u8> {
        let mut s = Spec::new(124, 2, 1, 24);
        s.cstype = 0x4C49_4E4B; // 'LINK'
        s.profile_off = 138 + 8 - 14;
        s.profile_size = 15;
        let mut v = s.header();
        v.extend_from_slice(&[1, 2, 3, 4, 5, 6, 0, 0]); // 138..146
        v.extend_from_slice(b"C:\\icc\\home.icc\0"); // 146..162, 15 + NUL
        v
    }

    pub(super) fn paletted_rle8() -> Vec<u8> {
        let mut s = Spec::new(40, 2, 2, 8);
        s.comp = 1;
        s.clr_used = 4;
        s.off = 54 + 16;
        let mut v = s.header();
        for c in 0..4u8 {
            v.extend_from_slice(&[c * 10, c * 20, c * 30, 0]);
        }
        v.extend_from_slice(&[2, 1, 0, 0, 2, 2, 0, 1]); // two runs, EOL, EOB
        v.extend_from_slice(b"after");
        v
    }

    /// A 52-byte-header file: the decoder over-reads the V4 colour block.
    pub(super) fn header52() -> Vec<u8> {
        let mut s = Spec::new(52, 2, 1, 24);
        s.off = 66;
        let mut v = s.header();
        v.extend_from_slice(&[7; 8]); // 66..74, where bfOffBits says the pixels are
        v.resize(122, 0x55);
        v.extend_from_slice(&[11, 12, 13, 14, 15, 16, 0, 0]); // 122..130
        v.extend_from_slice(b"tail");
        v
    }

    pub(super) fn bitfields16() -> Vec<u8> {
        let mut s = Spec::new(40, 2, 1, 16);
        s.comp = 3;
        s.off = 66;
        let mut v = s.header();
        for m in [0xF800u32, 0x07E0, 0x001F] {
            v.extend_from_slice(&m.to_le_bytes());
        }
        v.extend_from_slice(&[0x1F, 0x00, 0xE0, 0x07, 0, 0, 0, 0]);
        v
    }

    #[test]
    fn pinned_v5_embedded_profile_gap_and_trailing() {
        let v = v5_with_profile();
        let inv = inventory_of(BmpDecoderConfig::new(), &v);
        assert_parts(
            &inv,
            &[
                "header - 0..14 structure \"BM\"",
                "  field signature 0..2 structure",
                "  field file-size 2..6 dropped",
                "  field reserved 6..10 dropped",
                "  field pixel-offset 10..14 structure",
                "header 0x7c 14..138 structure",
                "  field geometry 18..34 structure",
                "  field image-size 34..38 dropped",
                "  field resolution 38..46 metadata(resolution)",
                "  field colors-used 46..50 structure",
                "  field colors-important 50..54 dropped",
                "  field masks 54..66 dropped",
                "  field alpha-mask 66..70 dropped",
                "  field colour-space-type 70..74 dropped",
                "  field endpoints 74..110 dropped",
                "  field gamma 110..122 dropped",
                "  field intent 122..126 dropped",
                "  field profile-offset 126..130 dropped",
                "  field profile-size 130..134 dropped",
                "  field reserved 134..138 dropped",
                "gap - 138..142 unreferenced",
                "block pixel-array 142..158 image-data",
                "block icc-profile 158..170 skipped",
                "gap - 170..175 trailing",
            ],
        );
        zencodec_testkit::check_inventory(BmpDecoderConfig::new(), &v).unwrap();
    }

    #[test]
    fn pinned_v5_linked_profile() {
        let v = v5_with_linked_profile();
        let inv = inventory_of(BmpDecoderConfig::new(), &v);
        assert_parts(
            &inv,
            &[
                "header - 0..14 structure \"BM\"",
                "  field signature 0..2 structure",
                "  field file-size 2..6 dropped",
                "  field reserved 6..10 dropped",
                "  field pixel-offset 10..14 structure",
                "header 0x7c 14..138 structure",
                "  field geometry 18..34 structure",
                "  field image-size 34..38 dropped",
                "  field resolution 38..46 metadata(resolution)",
                "  field colors-used 46..50 structure",
                "  field colors-important 50..54 dropped",
                "  field masks 54..66 dropped",
                "  field alpha-mask 66..70 dropped",
                "  field colour-space-type 70..74 dropped",
                "  field endpoints 74..110 dropped",
                "  field gamma 110..122 dropped",
                "  field intent 122..126 dropped",
                "  field profile-offset 126..130 dropped",
                "  field profile-size 130..134 dropped",
                "  field reserved 134..138 dropped",
                "block pixel-array 138..146 image-data",
                "block linked-profile 146..161 skipped \"C:\\\\icc\\\\home.icc\"",
                "gap - 161..162 trailing",
            ],
        );
        zencodec_testkit::check_inventory(BmpDecoderConfig::new(), &v).unwrap();
    }

    #[test]
    fn pinned_paletted_rle8() {
        let v = paletted_rle8();
        let inv = inventory_of(BmpDecoderConfig::new(), &v);
        assert_parts(
            &inv,
            &[
                "header - 0..14 structure \"BM\"",
                "  field signature 0..2 structure",
                "  field file-size 2..6 dropped",
                "  field reserved 6..10 dropped",
                "  field pixel-offset 10..14 structure",
                "header 0x28 14..54 structure",
                "  field geometry 18..34 structure",
                "  field image-size 34..38 dropped",
                "  field resolution 38..46 metadata(resolution)",
                "  field colors-used 46..50 structure",
                "  field colors-important 50..54 dropped",
                "block colour-table 54..70 structure",
                "block pixel-array 70..78 image-data",
                "gap - 78..83 trailing",
            ],
        );
        zencodec_testkit::check_inventory(BmpDecoderConfig::new(), &v).unwrap();
    }

    #[test]
    fn pinned_header52_overread() {
        let v = header52();
        let inv = inventory_of(BmpDecoderConfig::new(), &v);
        assert_parts(
            &inv,
            &[
                "header - 0..14 structure \"BM\"",
                "  field signature 0..2 structure",
                "  field file-size 2..6 dropped",
                "  field reserved 6..10 dropped",
                "  field pixel-offset 10..14 structure",
                "header 0x34 14..66 structure",
                "  field geometry 18..34 structure",
                "  field image-size 34..38 dropped",
                "  field resolution 38..46 metadata(resolution)",
                "  field colors-used 46..50 structure",
                "  field colors-important 50..54 dropped",
                "  field masks 54..66 dropped",
                "gap - 66..122 skipped",
                "block pixel-array 122..130 image-data",
                "gap - 130..134 trailing",
            ],
        );
        zencodec_testkit::check_inventory(BmpDecoderConfig::new(), &v).unwrap();
    }

    #[test]
    fn pinned_bitfields_external_masks() {
        let v = bitfields16();
        let inv = inventory_of(BmpDecoderConfig::new(), &v);
        assert_parts(
            &inv,
            &[
                "header - 0..14 structure \"BM\"",
                "  field signature 0..2 structure",
                "  field file-size 2..6 dropped",
                "  field reserved 6..10 dropped",
                "  field pixel-offset 10..14 structure",
                "header 0x28 14..54 structure",
                "  field geometry 18..34 structure",
                "  field image-size 34..38 dropped",
                "  field resolution 38..46 metadata(resolution)",
                "  field colors-used 46..50 structure",
                "  field colors-important 50..54 dropped",
                "field masks 54..66 structure",
                "block pixel-array 66..70 image-data",
                "gap - 70..74 trailing",
            ],
        );
        zencodec_testkit::check_inventory(BmpDecoderConfig::new(), &v).unwrap();
    }
}
