//! TGA walker.
//!
//! The decoder reads the 18-byte header, skips the image ID, reads the colour
//! map (when the header declares one, used or not) and then the pixel data
//! (`crate::tga::decode::decode_pixels`). It never looks at the TGA 2.0
//! footer, so the extension area (author, comments, software ID), the
//! developer directory and the thumbnail, scan-line and colour-correction
//! tables are all reported as skipped.

use alloc::format;
use alloc::string::String;

use zencodec::ImageFormat;
use zencodec::inventory::{Disposition, Inventory, Part, PartId, PartKind, PartTag};

use super::{
    Claimed, Res, clip, cstr, fill_top_level_holes, label, malformed_rest, slice, u16_le, u32_le,
};
use crate::tga::decode::{TgaHeader, parse_header};

const SIGNATURE: &[u8; 18] = b"TRUEVISION-XFILE.\0";

pub(crate) fn walk(data: &[u8]) -> Res<Inventory> {
    let len = data.len() as u64;
    let mut inv = Inventory::new(ImageFormat::Tga, len);
    if data.is_empty() {
        return Ok(inv);
    }
    if len < 18 {
        malformed_rest(&mut inv, None, 0, len, "truncated TGA header")?;
        return Ok(inv);
    }
    let h = match parse_header(data) {
        Ok(h) => h,
        Err(e) => {
            inv.push(
                None,
                Part::new(
                    PartKind::Header,
                    PartTag::None,
                    0..18,
                    Disposition::Structure,
                )
                .with_detail(format!("the decoder rejects this header: {}", e.error())),
            )?;
            malformed_rest(&mut inv, None, 18, len, "after a rejected header")?;
            return Ok(inv);
        }
    };
    let mut claimed = Claimed::default();
    let header = inv.push(
        None,
        Part::new(
            PartKind::Header,
            PartTag::None,
            0..18,
            Disposition::Structure,
        ),
    )?;
    inv.push(
        Some(header),
        Part::new(
            PartKind::Field,
            PartTag::Name("origin".into()),
            8..12,
            Disposition::Dropped,
        )
        .with_detail("x/y origin, parsed and dropped"),
    )?;
    // Colour-map specification bytes the decoder never uses.
    if h.color_map_type == 0 {
        inv.push(
            Some(header),
            Part::new(
                PartKind::Field,
                PartTag::Name("colour-map-spec".into()),
                3..8,
                Disposition::Dropped,
            )
            .with_detail(
                "no colour map: the first-entry index, length and entry size are not used",
            ),
        )?;
    } else if !h.is_color_mapped() {
        inv.push(
            Some(header),
            Part::new(
                PartKind::Field,
                PartTag::Name("first-entry-index".into()),
                3..5,
                Disposition::Dropped,
            )
            .with_detail("the colour map is skipped, so its first-entry index is not used"),
        )?;
    }
    claimed.claim(0..18);

    // Image ID.
    let id_len = u64::from(h.id_length);
    let mut at = 18u64;
    if id_len > 0 {
        if let Some(r) = clip(at, id_len, len) {
            claimed.claim(r.clone());
            inv.push(
                None,
                Part::new(
                    PartKind::Block,
                    PartTag::Name("image-id".into()),
                    r.clone(),
                    Disposition::Skipped,
                )
                .with_label(label(cstr(slice(data, &r)))),
            )?;
        }
        at += id_len;
    }

    // Colour map.
    let mut rejected: Option<String> = None;
    if h.color_map_type == 1 {
        let entry = match h.color_map_depth {
            15 | 16 => 2u64,
            24 => 3,
            32 => 4,
            _ => 0,
        };
        if entry == 0 {
            rejected = Some(format!(
                "colour map depth {} is not supported",
                h.color_map_depth
            ));
        } else {
            let n = u64::from(h.color_map_length) * entry;
            if n > 0
                && let Some(r) = clip(at, n, len)
            {
                claimed.claim(r.clone());
                let used = h.is_color_mapped();
                if used {
                    // An 8-bit index reaches entry `index - color_map_start`, so
                    // only the first `256 - color_map_start` entries can be read.
                    let reachable = 256u64
                        .saturating_sub(u64::from(h.color_map_start))
                        .min(u64::from(h.color_map_length));
                    let split = (at + reachable * entry).min(r.end);
                    if split > r.start {
                        inv.push(
                            None,
                            Part::new(
                                PartKind::Block,
                                PartTag::Name("colour-map".into()),
                                r.start..split,
                                Disposition::Structure,
                            ),
                        )?;
                    }
                    if split < r.end {
                        inv.push(
                            None,
                            Part::new(
                                PartKind::Block,
                                PartTag::Name("colour-map-unreachable".into()),
                                split..r.end,
                                Disposition::Dropped,
                            )
                            .with_detail(
                                "colour map entries no 8-bit pixel index can reach (index - color_map_start)",
                            ),
                        )?;
                    }
                } else {
                    inv.push(
                        None,
                        Part::new(
                            PartKind::Block,
                            PartTag::Name("colour-map".into()),
                            r,
                            Disposition::Skipped,
                        )
                        .with_detail(
                            "the image type is not colour-mapped; the map is read past and unused",
                        ),
                    )?;
                }
            }
            at += n;
        }
    }

    // Pixel data.
    let mut pixel_end = at.min(len);
    if rejected.is_none() && at < len {
        let src_bpp = match h.pixel_depth {
            8 => 1u64,
            15 | 16 => 2,
            24 => 3,
            _ => 4,
        };
        let total = u128::from(h.width) * u128::from(h.height);
        let rle = matches!(h.image_type, 9..=11);
        let (end, state) = if rle {
            rle_extent(data, at, total, src_bpp)
        } else {
            let need = total * u128::from(src_bpp);
            if u128::from(at) + need <= u128::from(len) {
                (at + need as u64, Extent::Complete)
            } else {
                (len, Extent::Truncated)
            }
        };
        if end > at {
            let mut p = Part::new(
                PartKind::Block,
                PartTag::Name("pixels".into()),
                at..end,
                Disposition::ImageData,
            );
            match state {
                Extent::Truncated => {
                    p.disposition = Disposition::Malformed;
                    p = p.with_detail(
                        "truncated: the file ends before all pixels are read; the decoder rejects the file",
                    );
                }
                Extent::Overrun => {
                    p.disposition = Disposition::Malformed;
                    p = p.with_detail(
                        "a packet runs past the image bounds; the decoder rejects the file",
                    );
                }
                Extent::Complete => {}
            }
            claimed.claim(at..end);
            inv.push(None, p)?;
        }
        pixel_end = end;
        if state == Extent::Overrun {
            malformed_rest(&mut inv, None, end, len, "packet the decoder rejects")?;
            return Ok(inv);
        }
    } else if let Some(why) = rejected {
        malformed_rest(
            &mut inv,
            None,
            at.min(len),
            len,
            format!("the decoder rejects this file: {why}"),
        )?;
        return Ok(inv);
    }

    // TGA 2.0 footer and the areas it points to.
    if len >= 26 && pixel_end <= len - 26 && &data[len as usize - 18..] == SIGNATURE {
        let foot = len - 26;
        claimed.claim(foot..len);
        inv.push(
            None,
            Part::new(
                PartKind::Block,
                PartTag::Name("footer".into()),
                foot..len,
                Disposition::Skipped,
            )
            .with_label("TRUEVISION-XFILE")
            .with_detail("TGA 2.0 footer; the decoder never reads it"),
        )?;
        let ext_off = u64::from(u32_le(data, foot).unwrap_or(0));
        let dev_off = u64::from(u32_le(data, foot + 4).unwrap_or(0));
        if ext_off != 0 {
            extension_area(&mut inv, data, &mut claimed, &h, ext_off)?;
        }
        if dev_off != 0 {
            developer_area(&mut inv, data, &mut claimed, dev_off)?;
        }
    }

    fill_top_level_holes(&mut inv, pixel_end..len, |_| (Disposition::Trailing, None))?;
    Ok(inv)
}

fn extension_area(
    inv: &mut Inventory,
    data: &[u8],
    claimed: &mut Claimed,
    h: &TgaHeader,
    off: u64,
) -> Res<()> {
    let len = data.len() as u64;
    let size = u64::from(u16_le(data, off).unwrap_or(0));
    if size < 2 {
        return Ok(());
    }
    let Some(r) = clip(off, size, len) else {
        return Ok(());
    };
    if !claimed.try_claim(&r) {
        return Ok(());
    }
    let ext = inv.push(
        None,
        Part::new(
            PartKind::Block,
            PartTag::Name("extension-area".into()),
            r.clone(),
            Disposition::Skipped,
        )
        .with_detail("author, comments, job and software strings; never read"),
    )?;
    if r.end - r.start >= 495 {
        for (rel, n, name) in [
            (2u64, 41u64, "author"),
            (43, 324, "comments"),
            (379, 41, "job-name"),
            (426, 41, "software-id"),
        ] {
            let text = cstr(slice(data, &(off + rel..off + rel + n)));
            if !text.is_empty() {
                inv.push(
                    Some(ext),
                    Part::new(
                        PartKind::Field,
                        PartTag::Name(name.into()),
                        off + rel..off + rel + n,
                        Disposition::Skipped,
                    )
                    .with_label(label(text)),
                )?;
            }
        }
        let stamp_off = u64::from(u32_le(data, off + 486).unwrap_or(0));
        let cc_off = u64::from(u32_le(data, off + 482).unwrap_or(0));
        let scan_off = u64::from(u32_le(data, off + 490).unwrap_or(0));
        let bytes_per = match h.pixel_depth {
            8 => 1u64,
            15 | 16 => 2,
            24 => 3,
            _ => 4,
        };
        if stamp_off != 0 {
            let (sw, sh) = (
                u64::from(*data.get(stamp_off as usize).unwrap_or(&0)),
                u64::from(*data.get(stamp_off as usize + 1).unwrap_or(&0)),
            );
            skipped_area(
                inv,
                claimed,
                "postage-stamp",
                stamp_off,
                2 + sw * sh * bytes_per,
                len,
            )?;
        }
        if cc_off != 0 {
            skipped_area(inv, claimed, "colour-correction-table", cc_off, 2048, len)?;
        }
        if scan_off != 0 {
            skipped_area(
                inv,
                claimed,
                "scan-line-table",
                scan_off,
                u64::from(h.height) * 4,
                len,
            )?;
        }
    }
    Ok(())
}

fn skipped_area(
    inv: &mut Inventory,
    claimed: &mut Claimed,
    name: &'static str,
    off: u64,
    n: u64,
    len: u64,
) -> Res<Option<PartId>> {
    if let Some(r) = clip(off, n, len)
        && claimed.try_claim(&r)
    {
        return inv
            .push(
                None,
                Part::new(
                    PartKind::Block,
                    PartTag::Name(name.into()),
                    r,
                    Disposition::Skipped,
                ),
            )
            .map(Some);
    }
    Ok(None)
}

fn developer_area(inv: &mut Inventory, data: &[u8], claimed: &mut Claimed, off: u64) -> Res<()> {
    let len = data.len() as u64;
    let count = u64::from(u16_le(data, off).unwrap_or(0));
    let Some(r) = clip(off, 2 + count * 10, len) else {
        return Ok(());
    };
    if !claimed.try_claim(&r) {
        return Ok(());
    }
    inv.push(
        None,
        Part::new(
            PartKind::Block,
            PartTag::Name("developer-directory".into()),
            r.clone(),
            Disposition::Skipped,
        )
        .with_detail(format!("{count} entries")),
    )?;
    for i in 0..count {
        let e = off + 2 + i * 10;
        let (Some(tag), Some(o), Some(n)) =
            (u16_le(data, e), u32_le(data, e + 2), u32_le(data, e + 6))
        else {
            break;
        };
        if let Some(fr) = clip(u64::from(o), u64::from(n), len)
            && claimed.try_claim(&fr)
        {
            inv.push(
                None,
                Part::new(
                    PartKind::Block,
                    PartTag::Code(u32::from(tag)),
                    fr,
                    Disposition::Skipped,
                )
                .with_detail("developer field"),
            )?;
        }
    }
    Ok(())
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Extent {
    Complete,
    Truncated,
    Overrun,
}

/// Walk RLE packets like `decode_rle` until `total` pixels are filled.
fn rle_extent(data: &[u8], start: u64, total: u128, src_bpp: u64) -> (u64, Extent) {
    let len = data.len() as u64;
    let mut pos = start;
    let mut filled: u128 = 0;
    while filled < total {
        let Some(&hd) = data.get(pos as usize) else {
            return (len, Extent::Truncated);
        };
        let run = u128::from(hd & 0x7F) + 1;
        if filled + run > total {
            return (pos, Extent::Overrun);
        }
        let body = if hd & 0x80 != 0 {
            src_bpp
        } else {
            run as u64 * src_bpp
        };
        if pos + 1 + body > len {
            return (len, Extent::Truncated);
        }
        pos += 1 + body;
        filled += run;
    }
    (pos, Extent::Complete)
}
