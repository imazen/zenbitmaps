//! Radiance HDR walker.
//!
//! Header: the `#?RADIANCE`/`#?RGBE` line, any number of `KEY=value` or `#`
//! lines, an empty line, then the resolution line. The decoder keeps only the
//! resolution line (`crate::hdr::decode::parse_header`); every other header
//! line is skipped, including `FORMAT=`, `EXPOSURE=` and `PRIMARIES=`. Pixels
//! follow as flat RGBE quads or new-style RLE scanlines.

use alloc::format;
use alloc::string::String;

use zencodec::ImageFormat;
use zencodec::inventory::{Disposition, Inventory, Part, PartKind, PartTag};

use super::{Res, label, malformed_rest, trailer};
use crate::hdr::decode::parse_header;

pub(crate) fn walk(data: &[u8]) -> Res<Inventory> {
    let len = data.len() as u64;
    let mut inv = Inventory::new(ImageFormat::Hdr, len);
    if data.is_empty() {
        return Ok(inv);
    }
    if len < 10 || !(data.starts_with(b"#?RADIANCE") || data.starts_with(b"#?RGBE")) {
        malformed_rest(&mut inv, None, 0, len, "not a Radiance HDR header")?;
        return Ok(inv);
    }
    let parsed = parse_header(data);

    // Header lines, split exactly like `parse_header`: a line ends at `\n`,
    // and an immediately following `\n` is the empty separator line.
    let mut pos = 0usize;
    let mut first = true;
    let mut separator_end = None;
    while pos < data.len() {
        let Some(nl) = data[pos..].iter().position(|&b| b == b'\n') else {
            break;
        };
        let line_end = pos + nl; // exclusive of '\n'
        let range = pos as u64..(line_end + 1) as u64;
        let text = &data[pos..line_end];
        if first {
            inv.push(
                None,
                Part::new(
                    PartKind::Header,
                    PartTag::None,
                    range,
                    Disposition::Structure,
                )
                .with_label(label(text)),
            )?;
            first = false;
        } else {
            let key = text.iter().position(|&b| b == b'=').map(|i| &text[..i]);
            let name: String = match key {
                Some(k) if !k.is_empty() && k.len() <= 32 => {
                    String::from_utf8_lossy(k).into_owned()
                }
                _ if text.first() == Some(&b'#') => "comment".into(),
                _ => "line".into(),
            };
            inv.push(
                None,
                Part::new(
                    PartKind::Attribute,
                    PartTag::Name(name.into()),
                    range,
                    Disposition::Skipped,
                )
                .with_label(label(text))
                .with_detail("header line the decoder skips"),
            )?;
        }
        pos = line_end + 1;
        if pos < data.len() && data[pos] == b'\n' {
            inv.push(
                None,
                Part::new(
                    PartKind::Header,
                    PartTag::Name("end-of-header".into()),
                    pos as u64..(pos + 1) as u64,
                    Disposition::Structure,
                ),
            )?;
            pos += 1;
            separator_end = Some(pos);
            break;
        }
    }
    let Some(res_start) = separator_end else {
        malformed_rest(
            &mut inv,
            None,
            pos.min(data.len()) as u64,
            len,
            "no empty line ends the header",
        )?;
        return Ok(inv);
    };

    let (w, h, offset) = match parsed {
        Ok(v) => v,
        Err(e) => {
            // The resolution line (if present) and everything after it.
            let rest = &data[res_start..];
            match rest.iter().position(|&b| b == b'\n') {
                Some(nl) => {
                    let end = (res_start + nl + 1) as u64;
                    inv.push(
                        None,
                        Part::new(
                            PartKind::Header,
                            PartTag::Name("resolution".into()),
                            res_start as u64..end,
                            Disposition::Malformed,
                        )
                        .with_label(label(&rest[..nl]))
                        .with_detail(format!("the decoder rejects this header: {}", e.error())),
                    )?;
                    malformed_rest(&mut inv, None, end, len, "after a rejected header")?;
                }
                None => malformed_rest(
                    &mut inv,
                    None,
                    res_start as u64,
                    len,
                    format!("the decoder rejects this header: {}", e.error()),
                )?,
            }
            return Ok(inv);
        }
    };
    inv.push(
        None,
        Part::new(
            PartKind::Header,
            PartTag::Name("resolution".into()),
            res_start as u64..offset as u64,
            Disposition::Structure,
        )
        .with_label(label(&data[res_start..offset])),
    )?;

    // Pixel scanlines.
    let (end, complete) = scan_rows(data, offset, u64::from(w), u64::from(h));
    if end > offset {
        let mut p = Part::new(
            PartKind::Block,
            PartTag::Name("pixels".into()),
            offset as u64..end as u64,
            Disposition::ImageData,
        );
        if !complete {
            p = p.with_detail("the decoder rejects or runs out of data in these scanlines");
        }
        inv.push(None, p)?;
    }
    if !complete {
        malformed_rest(
            &mut inv,
            None,
            end as u64,
            len,
            "scanline the decoder cannot read",
        )?;
        return Ok(inv);
    }
    trailer(&mut inv, end as u64, None)?;
    inv.fill_gaps(None, Disposition::Trailing)?;
    Ok(inv)
}

/// Walk `h` scanlines starting at `pos` like `decode_pixels`. Returns the end
/// of the last good scanline and whether all `h` were read.
fn scan_rows(data: &[u8], mut pos: usize, w: u64, h: u64) -> (usize, bool) {
    for _ in 0..h {
        let row_start = pos;
        if pos + 4 > data.len() {
            return (row_start, false);
        }
        if (8..=0x7FFF).contains(&w) && data[pos] == 2 && data[pos + 1] == 2 && data[pos + 2] < 128
        {
            let encoded = (u64::from(data[pos + 2]) << 8) | u64::from(data[pos + 3]);
            if encoded != w {
                return (row_start, false);
            }
            pos += 4;
            for _ch in 0..4 {
                let mut col = 0u64;
                while col < w {
                    let Some(&code) = data.get(pos) else {
                        return (row_start, false);
                    };
                    pos += 1;
                    if code > 128 {
                        let count = u64::from(code - 128);
                        if col + count > w || pos >= data.len() {
                            return (row_start, false);
                        }
                        pos += 1;
                        col += count;
                    } else {
                        let count = u64::from(code);
                        if count == 0 || col + count > w || pos as u64 + count > data.len() as u64 {
                            return (row_start, false);
                        }
                        pos += count as usize;
                        col += count;
                    }
                }
            }
        } else {
            let needed = w * 4;
            if pos as u64 + needed > data.len() as u64 {
                return (row_start, false);
            }
            pos += needed as usize;
        }
    }
    (pos, true)
}
