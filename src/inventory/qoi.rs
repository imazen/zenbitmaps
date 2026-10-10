//! QOI walker: 14-byte header, a stream of ops that fills `width*height`
//! pixels, then the 8-byte end marker. The decoder stops after the last
//! pixel and never reads the end marker (`QoiDecodeState::decode_into`).

use alloc::format;

use zencodec::ImageFormat;
use zencodec::inventory::{Disposition, Inventory, MetadataKind, Part, PartKind, PartTag};

use super::{Res, malformed_rest, trailer, u32_be};

const END_MARKER: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];

pub(crate) fn walk(data: &[u8]) -> Res<Inventory> {
    let len = data.len() as u64;
    let mut inv = Inventory::new(ImageFormat::Qoi, len);
    if data.is_empty() {
        return Ok(inv);
    }
    if len < 14 || &data[..4] != b"qoif" {
        let why = if len < 14 && b"qoif".starts_with(&data[..data.len().min(4)]) {
            "truncated QOI header"
        } else {
            "not a QOI header"
        };
        malformed_rest(&mut inv, None, 0, len, why)?;
        return Ok(inv);
    }
    let w = u128::from(u32_be(data, 4).unwrap_or(0));
    let h = u128::from(u32_be(data, 8).unwrap_or(0));
    let (channels, colorspace) = (data[12], data[13]);
    let header_ok = w != 0 && h != 0 && matches!(channels, 3 | 4) && matches!(colorspace, 0 | 1);
    let header = inv.push(
        None,
        Part::new(
            PartKind::Header,
            PartTag::None,
            0..14,
            Disposition::Structure,
        )
        .with_label("qoif"),
    )?;
    inv.push(
        Some(header),
        Part::new(
            PartKind::Field,
            PartTag::Name("colorspace".into()),
            13..14,
            Disposition::Metadata(MetadataKind::Cicp),
        )
        .with_detail(
            "0 = sRGB, 1 = linear; reported as CICP by probe() only, the decode() output does not carry it",
        ),
    )?;
    if !header_ok {
        malformed_rest(
            &mut inv,
            None,
            14,
            len,
            format!(
                "the decoder rejects this header (width {w}, height {h}, channels {channels}, colorspace {colorspace})"
            ),
        )?;
        return Ok(inv);
    }

    // The op stream.
    let mut pos: u64 = 14;
    let mut remaining: u128 = w * h;
    let mut truncated = false;
    let mut ignored_alpha = false;
    while remaining > 0 {
        let Some(&b) = data.get(pos as usize) else {
            truncated = true;
            break;
        };
        let (size, pixels): (u64, u128) = match b {
            0xFE => (4, 1),
            0xFF => {
                ignored_alpha |= channels == 3;
                (5, 1)
            }
            0xC0..=0xFD => (1, u128::from(b & 0x3F) + 1),
            0x80..=0xBF => (2, 1),
            _ => (1, 1),
        };
        if pos + size > len {
            truncated = true;
            break;
        }
        pos += size;
        remaining = remaining.saturating_sub(pixels);
    }
    if pos > 14 {
        let mut p = Part::new(
            PartKind::Block,
            PartTag::Name("pixels".into()),
            14..pos,
            Disposition::ImageData,
        );
        if truncated {
            p.disposition = Disposition::Malformed;
            p = p.with_detail(
                "truncated: the op stream ends before all pixels are filled; the decoder rejects the file",
            );
        } else if ignored_alpha {
            p = p.with_detail(
                "contains RGBA ops in a 3-channel file: their alpha byte is read and ignored",
            );
        }
        inv.push(None, p)?;
    }
    if truncated {
        malformed_rest(
            &mut inv,
            None,
            pos,
            len,
            "incomplete op at the end of the file",
        )?;
        return Ok(inv);
    }
    let mut end = pos;
    if data.get(pos as usize..pos as usize + 8) == Some(&END_MARKER[..]) {
        inv.push(
            None,
            Part::new(
                PartKind::Block,
                PartTag::Name("end-marker".into()),
                pos..pos + 8,
                Disposition::Skipped,
            )
            .with_detail("the decoder stops after the last pixel and never reads the end marker"),
        )?;
        end = pos + 8;
    }
    trailer(&mut inv, end, None)?;
    inv.fill_gaps(None, Disposition::Trailing)?;
    Ok(inv)
}
