//! BMP walker.
//!
//! Layout of a file, as the decoder reads it (`crate::bmp::decode`):
//! the 14-byte file header, the info header (12/16/40/52/56/64/108/124
//! bytes), optional external bitfield masks (40-byte header with
//! `BI_BITFIELDS`), the colour table (paletted images), then the pixel array
//! at `max(bfOffBits, position after the header fields)`. An embedded or
//! linked ICC profile (V5 header) is located by its own offset and is never
//! read. The start of the pixel array comes from the decoder itself
//! ([`crate::bmp::decode::header_trace`]); its end is computed from the
//! stride or, for RLE, by a dry run of the RLE control flow.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ops::Range;

use zencodec::ImageFormat;
use zencodec::inventory::{Disposition, Inventory, MetadataKind, Part, PartId, PartKind, PartTag};

use super::{
    Claimed, Res, clip, cstr, fill_top_level_holes, label, malformed_rest, slice, trailer, u16_le,
    u32_le,
};
use crate::bmp::BmpPermissiveness;
use crate::bmp::decode::header_trace;

const KNOWN_HEADER_SIZES: [u32; 8] = [12, 16, 40, 52, 56, 64, 108, 124];
const CS_EMBEDDED: u32 = 0x4D42_4544; // 'MBED'
const CS_LINKED: u32 = 0x4C49_4E4B; // 'LINK'

struct Field {
    /// Offset inside the info header.
    at: u64,
    len: u64,
    name: &'static str,
    disposition: Disposition,
    detail: Option<String>,
}

fn field(at: u64, len: u64, name: &'static str, disposition: Disposition) -> Field {
    Field {
        at,
        len,
        name,
        disposition,
        detail: None,
    }
}

pub(crate) fn walk(data: &[u8], perm: BmpPermissiveness) -> Res<Inventory> {
    let len = data.len() as u64;
    let mut inv = Inventory::new(ImageFormat::Bmp, len);
    if data.is_empty() {
        return Ok(inv);
    }
    if len < 2 || &data[..2] != b"BM" {
        malformed_rest(&mut inv, None, 0, len, "not a BMP (no BM signature)")?;
        return Ok(inv);
    }
    let strict = perm == BmpPermissiveness::Strict;
    // The decoder's Strict-mode size check multiplies without overflow checks and
    // panics (debug) or wraps (release) on absurd dimensions; report such a file
    // as rejected instead of running that code.
    let trace = if strict && strict_check_overflows(data) {
        Err(whereat::at!(crate::error::BitmapError::InvalidData(
            "Strict-mode image size check overflows (the decoder panics in debug builds)".into()
        )))
    } else {
        header_trace(data, perm)
    };
    let reject: Option<String> = trace.as_ref().err().map(|e| e.error().to_string());
    let reject_detail = reject
        .as_ref()
        .map(|r| format!("the decoder rejects this file: {r}"));

    // ── file header ──────────────────────────────────────────────────
    if len < 14 {
        inv.push(
            None,
            Part::new(
                PartKind::Header,
                PartTag::None,
                0..len,
                Disposition::Structure,
            )
            .with_label("BM")
            .with_detail("truncated file header"),
        )?;
        return Ok(inv);
    }
    let bf_size = u32_le(data, 2).unwrap_or(0);
    let fh = inv.push(
        None,
        Part::new(
            PartKind::Header,
            PartTag::None,
            0..14,
            Disposition::Structure,
        )
        .with_label("BM"),
    )?;
    let file_fields: [(u64, u64, &str, Disposition, Option<&str>); 4] = [
        (0, 2, "signature", Disposition::Structure, None),
        (
            2,
            4,
            "file-size",
            if strict {
                Disposition::Structure
            } else {
                Disposition::Dropped
            },
            Some("only compared with the real size in Strict mode"),
        ),
        (
            6,
            4,
            "reserved",
            Disposition::Dropped,
            Some("bfReserved1/2, skipped"),
        ),
        (10, 4, "pixel-offset", Disposition::Structure, None),
    ];
    for (at, n, name, d, detail) in file_fields {
        let mut p = Part::new(PartKind::Field, PartTag::Name(name.into()), at..at + n, d);
        if let Some(detail) = detail {
            p = p.with_detail(detail);
        }
        inv.push(Some(fh), p)?;
    }
    let mut claimed = Claimed::default();
    claimed.claim(0..14);
    if len < 18 {
        malformed_rest(
            &mut inv,
            None,
            14,
            len,
            "truncated before the info header size",
        )?;
        return Ok(inv);
    }

    // ── info header ──────────────────────────────────────────────────
    let ihsize = u32_le(data, 14).unwrap_or(0);
    if !KNOWN_HEADER_SIZES.contains(&ihsize) {
        inv.push(
            None,
            Part::new(
                PartKind::Header,
                PartTag::Code(ihsize),
                14..18,
                Disposition::Structure,
            )
            .with_detail(format!(
                "unknown info header size {ihsize}; the decoder rejects it"
            )),
        )?;
        malformed_rest(&mut inv, None, 18, len, "after an unknown info header size")?;
        return Ok(inv);
    }
    let dib_end = 14 + u64::from(ihsize);
    if dib_end > len {
        inv.push(
            None,
            Part::new(
                PartKind::Header,
                PartTag::Code(ihsize),
                14..len,
                Disposition::Structure,
            )
            .with_detail("truncated info header"),
        )?;
        return Ok(inv);
    }
    let os2 = ihsize == 12;
    let (width, height_raw, planes, bpp, comp_raw) = if os2 {
        (
            u64::from(u16_le(data, 18).unwrap_or(0)),
            u64::from(u16_le(data, 20).unwrap_or(0)),
            u16_le(data, 22).unwrap_or(0),
            u16_le(data, 24).unwrap_or(0),
            0u32,
        )
    } else {
        (
            u64::from(u32_le(data, 18).unwrap_or(0)),
            u64::from(u32_le(data, 22).unwrap_or(0)),
            u16_le(data, 26).unwrap_or(0),
            u16_le(data, 28).unwrap_or(0),
            if ihsize >= 40 {
                u32_le(data, 30).unwrap_or(0)
            } else {
                0
            },
        )
    };
    let _ = planes;
    let height = u64::from((height_raw as u32 as i32).unsigned_abs());
    let (xppm, yppm, clr_used) = if ihsize > 16 {
        (
            u32_le(data, 38).unwrap_or(0),
            u32_le(data, 42).unwrap_or(0),
            u32_le(data, 46).unwrap_or(0),
        )
    } else {
        (0, 0, 0)
    };
    let bitfields = matches!(comp_raw, 3 | 6);
    let masks_used = bitfields && matches!(bpp, 16 | 32);
    let masks_detail = || {
        (!masks_used).then(|| {
            "read, but the decoder only applies masks for BITFIELDS compression at 16 or 32 bpp"
                .to_string()
        })
    };
    let masks_disposition = if masks_used {
        Disposition::Structure
    } else {
        Disposition::Dropped
    };
    let colour_note = "parsed and dropped; the zencodec path reports sRGB".to_string();

    let mut fields: Vec<Field> = Vec::new();
    if os2 {
        fields.push(field(4, 12, "geometry", Disposition::Structure));
    } else if ihsize == 16 {
        fields.push(field(4, 16, "geometry", Disposition::Structure));
    } else {
        fields.push(field(4, 16, "geometry", Disposition::Structure));
        fields.push(Field {
            detail: Some(
                "only checked against the geometry in Strict mode, and only for uncompressed RGB"
                    .into(),
            ),
            ..field(
                20,
                4,
                "image-size",
                if strict && comp_raw == 0 && width > 0 {
                    Disposition::Structure
                } else {
                    Disposition::Dropped
                },
            )
        });
        let has_res = xppm > 0 || yppm > 0;
        fields.push(Field {
            detail: Some(if has_res {
                "reported by probe() only; the decode() output does not carry it".into()
            } else {
                "both densities are zero, so none is reported (range-checked in Strict mode)".into()
            }),
            ..field(
                24,
                8,
                "resolution",
                if has_res {
                    Disposition::Metadata(MetadataKind::Resolution)
                } else if strict {
                    Disposition::Structure
                } else {
                    Disposition::Dropped
                },
            )
        });
        fields.push(if bpp <= 8 {
            field(32, 4, "colors-used", Disposition::Structure)
        } else {
            Field {
                detail: Some(
                    "only sizes the skipped colour table above 8 bpp; the decoder never uses it"
                        .into(),
                ),
                ..field(32, 4, "colors-used", Disposition::Dropped)
            }
        });
        fields.push(field(36, 4, "colors-important", Disposition::Dropped));
        if ihsize >= 52 {
            fields.push(Field {
                detail: masks_detail(),
                ..field(40, 12, "masks", masks_disposition)
            });
        }
        if ihsize >= 56 {
            fields.push(Field {
                detail: masks_detail(),
                ..field(52, 4, "alpha-mask", masks_disposition)
            });
        }
        if ihsize >= 108 {
            for (at, n, name) in [
                (56, 4, "colour-space-type"),
                (60, 36, "endpoints"),
                (96, 12, "gamma"),
            ] {
                fields.push(Field {
                    detail: Some(colour_note.clone()),
                    ..field(at, n, name, Disposition::Dropped)
                });
            }
        } else if ihsize == 64 {
            fields.push(Field {
                detail: Some("OS/2 2.x extension fields, not read".into()),
                ..field(56, 8, "os2-extension", Disposition::Dropped)
            });
        }
        if ihsize >= 124 {
            for (at, n, name) in [
                (108, 4, "intent"),
                (112, 4, "profile-offset"),
                (116, 4, "profile-size"),
                (120, 4, "reserved"),
            ] {
                fields.push(Field {
                    detail: Some("parsed and dropped".into()),
                    ..field(at, n, name, Disposition::Dropped)
                });
            }
        }
    }
    let mut dib = Part::new(
        PartKind::Header,
        PartTag::Code(ihsize),
        14..dib_end,
        Disposition::Structure,
    );
    if let Some(d) = &reject_detail {
        dib = dib.with_detail(d.clone());
    }
    let dib_id = inv.push(None, dib)?;
    claimed.claim(14..dib_end);
    for f in fields {
        let (s, e) = (14 + f.at, 14 + f.at + f.len);
        if e > dib_end {
            continue;
        }
        let mut p = Part::new(
            PartKind::Field,
            PartTag::Name(f.name.into()),
            s..e,
            f.disposition,
        );
        if let Some(d) = f.detail {
            p = p.with_detail(d);
        }
        inv.push(Some(dib_id), p)?;
    }
    // Same shape as the other walkers: a file the decoder rejects keeps its
    // header and the rest is Malformed.
    let trace = match trace {
        Ok(t) => t,
        Err(_) => {
            malformed_rest(
                &mut inv,
                None,
                dib_end,
                len,
                reject_detail.unwrap_or_default(),
            )?;
            return Ok(inv);
        }
    };
    let pixel_start = trace.pixel_start as u64;
    // Overreads of the 52/56/64-byte headers: the decoder reads the V4 colour
    // block after the header, whatever is there.
    let overreads = matches!(ihsize, 52 | 56 | 64);

    // ── external masks (40-byte header with BITFIELDS) ────────────────
    let mut cursor_after_header = dib_end;
    let mut notes: Vec<String> = Vec::new();
    let entry = if os2 { 3u64 } else { 4 };
    if ihsize == 40
        && bitfields
        && let Some(r) = clip(dib_end, 12, len)
    {
        cursor_after_header = r.end;
        // The decoder reads the masks, then re-reads the colour table from the
        // same place; for a paletted image the bytes are the table.
        if trace.palette_entries.is_none() && claimed.try_claim(&r) {
            inv.push(
                None,
                Part::new(
                    PartKind::Field,
                    PartTag::Name("masks".into()),
                    r.clone(),
                    masks_disposition,
                )
                .with_detail(masks_detail().unwrap_or_else(|| "external bitfield masks".into())),
            )?;
        }
    }

    // ── colour table (as the decoder counted it) ─────────────────────
    let paletted = trace.palette_entries.is_some();
    if let Some(entries) = trace.palette_entries {
        let n = entries.min(256) as u64;
        // A short file: the decoder reads whole entries only (a partial one is
        // read in Permissive mode, zero-filled), and entries past the end are zero.
        let avail = len - dib_end;
        let bytes = if perm == BmpPermissiveness::Permissive {
            (n * entry).min(avail)
        } else {
            n.min(avail / entry) * entry
        };
        if let Some(r) = clip(dib_end, bytes, len) {
            if claimed.try_claim(&r) {
                let mut detail = format!("{n} entries of {entry} bytes");
                if entry == 4 {
                    detail.push_str("; the fourth byte of each entry is not read");
                }
                if ihsize == 40 && bitfields {
                    detail.push_str(
                        "; the same bytes are first read as the bitfield masks, which an image of 8 bpp or less ignores",
                    );
                }
                if ihsize == 12 {
                    detail.push_str(
                        "; OS/2 files: the decoder counts 256 entries whatever the file declares",
                    );
                }
                inv.push(
                    None,
                    Part::new(
                        PartKind::Block,
                        PartTag::Name("colour-table".into()),
                        r,
                        Disposition::Structure,
                    )
                    .with_detail(detail),
                )?;
            } else {
                notes.push("the colour table overlaps another part".into());
            }
        }
    }

    // A colour table in a truecolor file is sized by biClrUsed and never read.
    // It can only occupy the bytes before the pixel array.
    if !paletted && clr_used > 0 && bpp > 8 {
        let room = pixel_start.min(len).saturating_sub(cursor_after_header);
        if let Some(r) = clip(
            cursor_after_header,
            (u64::from(clr_used) * entry).min(room),
            len,
        ) && claimed.try_claim(&r)
        {
            inv.push(
                None,
                Part::new(
                    PartKind::Block,
                    PartTag::Name("colour-table".into()),
                    r,
                    Disposition::Skipped,
                )
                .with_detail("truecolor image: the decoder ignores the colour table"),
            )?;
        }
    }

    // ── pixel array (before the profile: the decoder reads these bytes) ──
    let stride = (width * u64::from(bpp)).div_ceil(32) * 4;
    let rows_len = u128::from(stride) * u128::from(height);
    let mut pixel_part: Option<Part> = None;
    let mut rle_pads: Vec<u64> = Vec::new();
    let pixel_end;
    if pixel_start >= len {
        pixel_end = len;
        notes.push(format!(
            "the pixel array would start at {pixel_start}, past the end of the file"
        ));
    } else {
        let known_comp = matches!(comp_raw, 0..=3 | 6);
        let rle = matches!(comp_raw, 1 | 2);
        let (end, disposition, detail): (u64, Disposition, Option<String>) = if !known_comp {
            let end = (u128::from(pixel_start) + rows_len).min(u128::from(len)) as u64;
            if perm == BmpPermissiveness::Permissive {
                (
                    end,
                    Disposition::Skipped,
                    Some(format!(
                        "unknown compression {comp_raw}: Permissive mode zero-fills the image without reading this"
                    )),
                )
            } else {
                (end, Disposition::ImageData, None)
            }
        } else if rle {
            let rle = rle_extent(
                data,
                pixel_start,
                bpp,
                width,
                height,
                perm == BmpPermissiveness::Permissive,
            );
            let end = rle.end;
            rle_pads = rle.pads;
            let mut detail = match rle.stop {
                RleStop::Marker => None,
                RleStop::Ended => {
                    Some("RLE data ends without the end-of-bitmap marker".to_string())
                }
                RleStop::Rejected(why) => Some(format!("the decoder rejects the file: {why}")),
            };
            let disposition = if let RleStop::Rejected(_) = rle.stop {
                Disposition::Malformed
            } else if paletted {
                Disposition::ImageData
            } else {
                let d = "RLE stream parsed for validity only: without a colour table the decoder discards the decoded pixels and the output is zero-filled";
                detail = Some(match detail {
                    Some(x) => format!("{x}; {d}"),
                    None => d.to_string(),
                });
                Disposition::Dropped
            };
            (end, disposition, detail)
        } else {
            let want = u128::from(pixel_start) + rows_len;
            let permissive = perm == BmpPermissiveness::Permissive;
            let avail = len - pixel_start;
            let (end, mut disposition, mut detail) = if want <= u128::from(len) {
                (want as u64, Disposition::ImageData, None)
            } else {
                let rejects = !permissive
                    && truncation_rejected(bpp, comp_raw, paletted, width, height, avail);
                let base = format!(
                    "truncated: {rows_len} bytes of rows are declared, {avail} are present"
                );
                if rejects {
                    (
                        len,
                        Disposition::Malformed,
                        Some(format!(
                            "{base}; the decoder rejects the file under this policy"
                        )),
                    )
                } else {
                    (
                        len,
                        Disposition::ImageData,
                        Some(format!(
                            "{base}; the decoder zero-fills the missing pixels, and a partial last pixel is not read"
                        )),
                    )
                }
            };
            if !permissive
                && disposition == Disposition::ImageData
                && let Some(n) = trace.palette_entries
                && let Some(idx) = first_bad_index(data, pixel_start, width, height, bpp, n)
            {
                disposition = Disposition::Malformed;
                detail = Some(format!(
                    "palette index {idx} out of range ({n} entries): the decoder rejects the file"
                ));
            }
            (end, disposition, detail)
        };
        pixel_end = end;
        if end > pixel_start {
            let mut detail = detail.unwrap_or_default();
            if !rle && known_comp {
                if !detail.is_empty() {
                    detail.push_str("; ");
                }
                detail.push_str("row padding bytes are not distinguished from pixel bytes");
            }
            let mut p = Part::new(
                PartKind::Block,
                PartTag::Name("pixel-array".into()),
                pixel_start..end,
                disposition,
            );
            if !detail.is_empty() {
                p = p.with_detail(detail);
            }
            claimed.claim(pixel_start..end);
            pixel_part = Some(p);
        }
    }

    // ── embedded / linked profile (V5) ───────────────────────────────
    if ihsize == 124 {
        let cs = u32_le(data, 14 + 56).unwrap_or(0);
        let off = u64::from(u32_le(data, 14 + 112).unwrap_or(0));
        let size = u64::from(u32_le(data, 14 + 116).unwrap_or(0));
        if matches!(cs, CS_EMBEDDED | CS_LINKED) {
            let start = 14 + off;
            match clip(start, size, len) {
                Some(r) => {
                    let linked = cs == CS_LINKED;
                    let mut p = Part::new(
                        PartKind::Block,
                        PartTag::Name(
                            if linked {
                                "linked-profile"
                            } else {
                                "icc-profile"
                            }
                            .into(),
                        ),
                        r.clone(),
                        Disposition::Skipped,
                    );
                    p = if linked {
                        p.with_label(label(cstr(slice(data, &r))))
                            .with_detail("linked profile path; never opened or reported")
                    } else {
                        p.with_detail(
                            "embedded ICC profile; not read, the zencodec path reports sRGB",
                        )
                    };
                    if r.end - r.start < size {
                        p = p.with_detail("profile extends past the end of the file");
                    }
                    if claimed.try_claim(&r) {
                        inv.push(None, p)?;
                    } else {
                        notes.push(format!(
                            "the {} profile named by the header at {}..{} overlaps other parts and is not reported separately",
                            if linked { "linked" } else { "embedded" },
                            r.start,
                            r.end
                        ));
                    }
                }
                None if size > 0 => notes.push("the profile lies outside the file".into()),
                None => {}
            }
        }
    }
    if let Some(mut p) = pixel_part {
        if !notes.is_empty() {
            let mut d = p.detail.take().unwrap_or_default();
            if !d.is_empty() {
                d.push_str("; ");
            }
            d.push_str(&notes.join("; "));
            p.detail = Some(d);
        }
        let range = p.range.clone();
        let id = inv.push(None, p)?;
        // RLE absolute runs of odd length end with a pad byte the decoder skips unread.
        let pads: Vec<u64> = rle_pads
            .iter()
            .copied()
            .filter(|&b| range.start <= b && b < range.end)
            .collect();
        if pads.len() <= MAX_PAD_PARTS {
            for b in pads {
                inv.push(
                    Some(id),
                    Part::new(PartKind::Gap, PartTag::None, b..b + 1, Disposition::Padding)
                        .with_detail("RLE absolute-run pad byte, skipped unread"),
                )?;
            }
        } else if let Some(p) = inv.get(id) {
            let d = format!(
                "{}{} RLE absolute-run pad bytes are not distinguished from pixel bytes",
                p.detail
                    .as_deref()
                    .map(|d| format!("{d}; "))
                    .unwrap_or_default(),
                pads.len()
            );
            inv.set_detail(id, d);
        }
    }

    // ── 52-byte header: the decoder reads the alpha mask just past the header ──
    let before = pixel_start.min(len);
    if ihsize == 52
        && let Some(r) = clip(dib_end, 4, before)
        && claimed.try_claim(&r)
    {
        inv.push(
            None,
            Part::new(
                PartKind::Field,
                PartTag::Name("alpha-mask".into()),
                r,
                masks_disposition,
            )
            .with_detail(
                masks_detail()
                    .map(|d| format!("read past the 52-byte header (over-read); {d}"))
                    .unwrap_or_else(|| "read past the 52-byte header (over-read)".into()),
            ),
        )?;
    }

    // ── holes ────────────────────────────────────────────────────────
    if overreads {
        // The V4 colour block the decoder skips after the header, up to 14 + 108.
        let zone_start = if ihsize == 52 { dib_end + 4 } else { dib_end };
        if zone_start < before.min(14 + 108) {
            fill_top_level_holes(&mut inv, zone_start..before.min(14 + 108), |_| {
                (
                    Disposition::Skipped,
                    Some(
                        "read by the decoder as the V4 colour block of the header (52/56/64-byte headers are over-read), then skipped"
                            .into(),
                    ),
                )
            })?;
        }
    }
    fill_top_level_holes(&mut inv, 0..before, |_| {
        (
            Disposition::Unreferenced,
            Some("between the headers/colour table and the pixel array".into()),
        )
    })?;
    if pixel_end < len {
        fill_top_level_holes(&mut inv, pixel_end..len, |_| (Disposition::Trailing, None))?;
    }
    let _ = (trailer, bf_size);
    Ok(inv)
}

/// Whether `decode_headers` would overflow in its Strict-mode image-size check
/// (`row_bytes * height`, unchecked in the decoder).
fn strict_check_overflows(data: &[u8]) -> bool {
    let Some(ihsize) = u32_le(data, 14) else {
        return false;
    };
    if ihsize <= 16 || !KNOWN_HEADER_SIZES.contains(&ihsize) {
        return false;
    }
    let (Some(w), Some(h), Some(bpp)) = (u32_le(data, 18), u32_le(data, 22), u16_le(data, 28))
    else {
        return false;
    };
    let height = (h as i32).unsigned_abs() as usize;
    (w as usize)
        .checked_mul(usize::from(bpp))
        .map(|v| v.div_ceil(32))
        .and_then(|v| v.checked_mul(4))
        .and_then(|row| row.checked_mul(height))
        .is_none()
}

// ── RLE dry run ──────────────────────────────────────────────────────

struct Cur<'a> {
    d: &'a [u8],
    p: usize,
    permissive: bool,
}

impl Cur<'_> {
    fn eof(&self) -> bool {
        self.p >= self.d.len()
    }
    fn u8(&mut self) -> u8 {
        match self.d.get(self.p) {
            Some(&b) => {
                self.p += 1;
                b
            }
            None => 0,
        }
    }
    /// `Cursor::skip`: past the end it errors (position unchanged) unless
    /// permissive (position = end).
    fn skip(&mut self, n: u64) -> bool {
        let np = (self.p as u64).saturating_add(n);
        if np > self.d.len() as u64 {
            if self.permissive {
                self.p = self.d.len();
                return true;
            }
            return false;
        }
        self.p = np as usize;
        true
    }
}

/// How the decoder's RLE loop ends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RleStop {
    /// An end-of-bitmap marker (or a final end-of-line the decoder accepts).
    Marker,
    /// The data ran out or the picture filled up; the decoder accepts that.
    Ended,
    /// The decoder returns an error here (never in Permissive mode).
    Rejected(&'static str),
}

struct RleExtent {
    end: u64,
    stop: RleStop,
    /// Offsets of the pad bytes after odd-length absolute runs.
    pads: Vec<u64>,
}

/// Largest number of RLE pad bytes reported as their own parts; beyond it the
/// pixel array's detail gives the count.
const MAX_PAD_PARTS: usize = 4096;

/// Where the RLE stream ends, how, and where its pad bytes are. Mirrors
/// `decode_rle4` / `decode_rle8plus`.
fn rle_extent(
    data: &[u8],
    start: u64,
    depth: u16,
    width: u64,
    height: u64,
    permissive: bool,
) -> RleExtent {
    let mut c = Cur {
        d: data,
        p: start as usize,
        permissive,
    };
    let mut pads = Vec::new();
    let mut line = height as i64 - 1;
    let mut x: u64 = 0;
    let done = |c: &Cur<'_>, stop: RleStop, pads: Vec<u64>| RleExtent {
        end: c.p as u64,
        stop,
        pads,
    };
    let underflow = |why: &'static str| {
        if permissive {
            RleStop::Ended
        } else {
            RleStop::Rejected(why)
        }
    };
    if depth == 4 {
        while line >= 0 && x <= width && !c.eof() {
            let code = u64::from(c.u8());
            if code == 0 {
                let s = c.u8();
                match s {
                    0 => {
                        line -= 1;
                        if line < 0 {
                            return done(&c, underflow("RLE4 line underflow"), pads);
                        }
                        x = 0;
                    }
                    1 => return done(&c, RleStop::Marker, pads),
                    2 => {
                        x += u64::from(c.u8());
                        line -= i64::from(c.u8());
                        if line < 0 {
                            return done(&c, underflow("RLE4 line underflow"), pads);
                        }
                    }
                    n => {
                        let odd = n & 1;
                        let bytes = u64::from(n).div_ceil(2);
                        for i in 0..bytes {
                            if x >= width {
                                break;
                            }
                            c.u8();
                            x += 1;
                            if i + 1 == bytes && odd > 0 {
                                break;
                            }
                            if x >= width {
                                break;
                            }
                            x += 1;
                        }
                        if bytes & 1 == 1 {
                            if !c.eof() {
                                pads.push(c.p as u64);
                            }
                            c.skip(1);
                        }
                    }
                }
            } else {
                if x + code > width + 1 {
                    if permissive {
                        c.u8();
                        continue;
                    }
                    return done(
                        &c,
                        RleStop::Rejected("RLE4 frame pointer out of bounds"),
                        pads,
                    );
                }
                c.u8();
                x += code.min(width.saturating_sub(x));
            }
        }
        return done(&c, RleStop::Ended, pads);
    }
    if !matches!(depth, 8 | 16 | 32) {
        c.p = data.len();
        return done(
            &c,
            RleStop::Rejected("unknown depth + RLE combination"),
            pads,
        );
    }
    let bd = u64::from((depth >> 3).max(1));
    let pixels_len = (u128::from(width) * u128::from(height) * u128::from(depth.max(8)))
        .div_ceil(8)
        .min(u128::from(u64::MAX)) as u64;
    while !c.eof() {
        let p1 = c.u8();
        if p1 == 0 {
            let p2 = c.u8();
            match p2 {
                0 => {
                    line -= 1;
                    if line < 0 {
                        let ok = c.p + 2 <= data.len() && data[c.p] == 0 && data[c.p + 1] == 1;
                        if c.p + 2 <= data.len() {
                            c.p += 2;
                        }
                        let stop = if ok {
                            RleStop::Marker
                        } else if permissive {
                            RleStop::Ended
                        } else {
                            RleStop::Rejected("RLE line beyond picture bounds")
                        };
                        return done(&c, stop, pads);
                    }
                    x = 0;
                    continue;
                }
                1 => return done(&c, RleStop::Marker, pads),
                2 => {
                    let dx = c.u8();
                    let dy = c.u8();
                    x += u64::from(dx);
                    line -= i64::from(dy);
                    if line < 0 {
                        return done(&c, underflow("RLE delta line underflow"), pads);
                    }
                    continue;
                }
                _ => {}
            }
            let n = u64::from(p2);
            let row_start = (line.max(0) as u64).saturating_mul(width);
            let out_start = row_start.saturating_add(x);
            if out_start.saturating_add(n.saturating_mul(bd)) > pixels_len {
                // The decoder ignores the result of this skip: a failed one leaves
                // the cursor where it is (Permissive: at the end) and goes on.
                c.skip(2 * bd);
                continue;
            }
            match depth {
                8 => {
                    let size = n;
                    if !c.skip(size) {
                        return done(
                            &c,
                            RleStop::Rejected("unexpected end of file in an absolute run"),
                            pads,
                        );
                    }
                    x += size;
                    if n & 1 == 1 {
                        if !c.eof() {
                            pads.push(c.p as u64);
                        }
                        c.skip(1);
                    }
                }
                _ => {
                    for _ in 0..n * bd {
                        c.u8();
                    }
                    x += bd * n;
                }
            }
        } else {
            let row_start = (line.max(0) as u64).saturating_mul(width);
            if x.saturating_add(u64::from(p1) * bd) > pixels_len.saturating_sub(row_start) {
                if permissive {
                    for _ in 0..bd {
                        c.u8();
                    }
                    continue;
                }
                return done(&c, RleStop::Rejected("RLE position overrun"), pads);
            }
            for _ in 0..bd {
                c.u8();
            }
            x += u64::from(p1) * bd;
        }
    }
    done(&c, RleStop::Ended, pads)
}

/// Whether the decoder rejects an uncompressed pixel array cut short (outside
/// Permissive): rows read with `read_exact_bytes` (1/2/4 bpp, 8 bpp grey, 24 bpp)
/// fail when the last row's data is short; paletted 8 bpp rows fail on a missing
/// row pad; 16 and 32 bpp rows are zero-filled.
fn truncation_rejected(
    bpp: u16,
    comp_raw: u32,
    paletted: bool,
    width: u64,
    height: u64,
    avail: u64,
) -> bool {
    let _ = comp_raw;
    let h = u128::from(height);
    let avail = u128::from(avail);
    match bpp {
        16 | 32 => false,
        8 if paletted => (width.wrapping_neg() & 3) != 0,
        1 | 2 | 4 => {
            let stride = u128::from((width * u64::from(bpp)).div_ceil(32) * 4);
            let row = u128::from((width * u64::from(bpp)).div_ceil(8));
            avail < (h - 1) * stride + row
        }
        8 | 24 => {
            let comp = u128::from(bpp / 8);
            let stride = u128::from((width * u64::from(bpp)).div_ceil(32) * 4);
            avail < (h - 1) * stride + u128::from(width) * comp
        }
        _ => false,
    }
}

/// The first palette index of an uncompressed paletted image that is not below
/// `entries`: the decoder rejects such a file outside Permissive mode
/// (`expand_palette*`). Bytes past the end of the file read as index 0.
fn first_bad_index(
    data: &[u8],
    start: u64,
    width: u64,
    height: u64,
    bpp: u16,
    entries: usize,
) -> Option<u32> {
    if entries >= 256 || entries >= (1usize << bpp) {
        return None;
    }
    let stride = if bpp == 8 {
        width + (width.wrapping_neg() & 3)
    } else {
        (width * u64::from(bpp)).div_ceil(32) * 4
    };
    let per_byte = 8 / u64::from(bpp);
    let mask = (1u16 << bpp) - 1;
    for row in 0..height {
        let row_at = start.checked_add(row.checked_mul(stride)?)?;
        if row_at >= data.len() as u64 {
            return None;
        }
        for x in 0..width {
            let byte_at = row_at + x / per_byte;
            let &b = data.get(byte_at as usize)?;
            let shift = 8 - u64::from(bpp) * (x % per_byte + 1);
            let idx = u32::from((u16::from(b) >> shift) & mask);
            if idx as usize >= entries {
                return Some(idx);
            }
        }
    }
    None
}

#[allow(dead_code)]
fn _range_len(r: &Range<u64>) -> u64 {
    r.end - r.start
}
#[allow(dead_code)]
fn _id(_: PartId) {}
