//! PNM/PAM/PFM walker.
//!
//! Mirrors `crate::pnm::decode::parse_header` and `crate::pnm::decode_with_alloc_pref`:
//! the header ends one byte after the last number (P1-P6), after the scale
//! line (PF/Pf) or after `ENDHDR` (P7); the pixel data is the first
//! `expected` bytes (binary) or the first `width*height*depth` tokens (ASCII);
//! everything after is ignored by the decoder.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use zencodec::ImageFormat;
use zencodec::inventory::{Disposition, Inventory, Part, PartId, PartKind, PartTag};

use super::{Res, label, malformed_rest, trailer};
use crate::pnm::decode::parse_header;
use crate::pnm::{PnmFormat, PnmHeader};

/// Comments and header lines are recorded only up to the inventory's part cap; a
/// file with that many cannot be inventoried anyway, and this bounds the walker's
/// memory by the cap rather than by the input size.
const MAX_RECORDED: usize = zencodec::inventory::DEFAULT_MAX_PARTS as usize;

fn too_many() -> zencodec::inventory::InventoryError {
    zencodec::inventory::InventoryError::TooManyParts {
        max: zencodec::inventory::DEFAULT_MAX_PARTS,
    }
}

pub(crate) fn walk(data: &[u8]) -> Res<Inventory> {
    let len = data.len() as u64;
    let mut inv = Inventory::new(ImageFormat::Pnm, len);
    if data.is_empty() {
        return Ok(inv);
    }
    let magic_ok = data.len() >= 3
        && matches!(
            &data[..2],
            b"P1" | b"P2" | b"P3" | b"P4" | b"P5" | b"P6" | b"P7" | b"Pf" | b"PF"
        );
    if !magic_ok {
        malformed_rest(
            &mut inv,
            None,
            0,
            len,
            "not a PNM header (needs P1-P7, Pf or PF and at least 3 bytes)",
        )?;
        return Ok(inv);
    }
    let magic = label(&data[..2]);
    let (comments, pam_lines, reach) = scan_header(data);
    if comments.len() >= MAX_RECORDED || pam_lines.len() >= MAX_RECORDED {
        return Err(too_many());
    }
    match parse_header(data) {
        Ok(h) => {
            let end = (h.data_offset as u64).min(len);
            let header = inv.push(
                None,
                Part::new(
                    PartKind::Header,
                    PartTag::None,
                    0..end,
                    Disposition::Structure,
                )
                .with_label(magic),
            )?;
            push_header_children(&mut inv, header, data, &comments, &pam_lines, end)?;
            if matches!(data[1], b'1'..=b'6') && end > 3 {
                inv.push(
                    Some(header),
                    Part::new(
                        PartKind::Field,
                        PartTag::Name("separator".into()),
                        end - 1..end,
                        Disposition::Dropped,
                    )
                    .with_detail(
                        "the byte after the last header number is skipped without being examined",
                    ),
                )?;
            }
            if end < len {
                walk_pixels(&mut inv, data, &h, end)?;
            }
        }
        Err(e) => {
            let end = (reach as u64).clamp(2, len);
            let header = inv.push(
                None,
                Part::new(
                    PartKind::Header,
                    PartTag::None,
                    0..end,
                    Disposition::Structure,
                )
                .with_label(magic)
                .with_detail(format!("the decoder rejects this header: {}", e.error())),
            )?;
            push_header_children(&mut inv, header, data, &comments, &pam_lines, end)?;
            malformed_rest(
                &mut inv,
                None,
                end,
                len,
                format!("after a rejected header: {}", e.error()),
            )?;
        }
    }
    inv.fill_gaps(None, Disposition::Trailing)?;
    Ok(inv)
}

// ── header ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PamLine {
    EndHdr,
    Key(&'static str),
    Tupltype,
    Comment,
    Blank,
    Unknown,
}

/// Comment ranges (from `#` to the end of the line, excluding the newline)
/// seen in the header, the P7 header lines, and how far the scan got.
type HeaderScan = (Vec<Range<usize>>, Vec<(Range<usize>, PamLine)>, usize);

fn scan_header(data: &[u8]) -> HeaderScan {
    let mut comments = Vec::new();
    let mut lines = Vec::new();
    let reach = match &data[..2] {
        b"P7" => scan_pam(data, &mut comments, &mut lines),
        b"Pf" | b"PF" => scan_numeric(data, 2, true, &mut comments),
        b"P1" | b"P4" => scan_numeric(data, 2, false, &mut comments),
        _ => scan_numeric(data, 3, false, &mut comments),
    };
    (comments, lines, reach)
}

/// Skip whitespace and `#` comments the way `skip_whitespace_and_comments`
/// does, recording the comments. `None` when the input ends first.
fn skip_ws(data: &[u8], mut pos: usize, comments: &mut Vec<Range<usize>>) -> Option<usize> {
    loop {
        match *data.get(pos)? {
            b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
            b'#' => {
                let start = pos;
                while pos < data.len() && data[pos] != b'\n' {
                    pos += 1;
                }
                if comments.len() < MAX_RECORDED {
                    comments.push(start..pos);
                }
                if pos < data.len() {
                    pos += 1;
                }
            }
            _ => return Some(pos),
        }
    }
}

/// End of the digit run at `pos` (at most 11 digits, like `parse_u32`).
fn number_end(data: &[u8], pos: usize) -> Option<usize> {
    let max_end = (pos + 11).min(data.len());
    let mut end = pos;
    while end < max_end && data[end].is_ascii_digit() {
        end += 1;
    }
    (end > pos).then_some(end)
}

/// Header end for P1-P6 (`tokens` numbers) and PF/Pf (two numbers and a scale
/// line). Returns the position reached; on success that is `data_offset`.
fn scan_numeric(data: &[u8], tokens: usize, pfm: bool, comments: &mut Vec<Range<usize>>) -> usize {
    let mut pos = 2;
    let mut reach = 2;
    for _ in 0..tokens {
        let Some(p) = skip_ws(data, pos, comments) else {
            return data.len();
        };
        let Some(e) = number_end(data, p) else {
            return p;
        };
        pos = e;
        reach = e;
    }
    if pfm {
        let Some(p) = skip_ws(data, pos, comments) else {
            return data.len();
        };
        let line_end = data[p..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(data.len(), |i| p + i);
        return (line_end + 1).min(data.len());
    }
    (reach + 1).min(data.len())
}

fn scan_pam(
    data: &[u8],
    comments: &mut Vec<Range<usize>>,
    lines: &mut Vec<(Range<usize>, PamLine)>,
) -> usize {
    let Some(mut pos) = skip_ws(data, 2, comments) else {
        return data.len();
    };
    loop {
        let line_end = data[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(data.len(), |i| pos + i);
        let raw = &data[pos..line_end];
        let Ok(text) = core::str::from_utf8(raw) else {
            lines.push((pos..line_end, PamLine::Unknown));
            return pos;
        };
        let t = text.trim();
        let kind = if t == "ENDHDR" {
            PamLine::EndHdr
        } else if t.starts_with("WIDTH ") {
            PamLine::Key("WIDTH")
        } else if t.starts_with("HEIGHT ") {
            PamLine::Key("HEIGHT")
        } else if t.starts_with("DEPTH ") {
            PamLine::Key("DEPTH")
        } else if t.starts_with("MAXVAL ") {
            PamLine::Key("MAXVAL")
        } else if t.starts_with("TUPLTYPE ") {
            PamLine::Tupltype
        } else if t.starts_with('#') {
            PamLine::Comment
        } else if t.is_empty() {
            PamLine::Blank
        } else {
            PamLine::Unknown
        };
        if pos < line_end && lines.len() < MAX_RECORDED {
            lines.push((pos..line_end, kind));
        }
        if kind == PamLine::EndHdr {
            return (line_end + 1).min(data.len());
        }
        pos = if line_end < data.len() {
            line_end + 1
        } else {
            data.len()
        };
        if pos >= data.len() {
            return data.len();
        }
    }
}

fn push_header_children(
    inv: &mut Inventory,
    header: PartId,
    data: &[u8],
    comments: &[Range<usize>],
    pam_lines: &[(Range<usize>, PamLine)],
    header_end: u64,
) -> Res<()> {
    // `#` comment text, without the marker.
    let comment = |inv: &mut Inventory, parent: PartId, r: &Range<usize>| -> Res<()> {
        let (s, e) = (r.start as u64, r.end as u64);
        if s < e && e <= header_end {
            inv.push(
                Some(parent),
                Part::new(
                    PartKind::Block,
                    PartTag::Name("comment".into()),
                    s..e,
                    Disposition::Skipped,
                )
                .with_label(label(&data[r.start + 1..r.end])),
            )?;
        }
        Ok(())
    };
    for r in comments {
        comment(inv, header, r)?;
    }
    // The last line for each key wins in the decoder.
    let last = |key: &str| {
        pam_lines.iter().rposition(|(_, k)| {
            *k == PamLine::Key(match key {
                "WIDTH" => "WIDTH",
                "HEIGHT" => "HEIGHT",
                "DEPTH" => "DEPTH",
                _ => "MAXVAL",
            })
        })
    };
    for (i, (r, kind)) in pam_lines.iter().enumerate() {
        let (s, e) = (r.start as u64, r.end as u64);
        if e > header_end || s >= e {
            continue;
        }
        let text = &data[r.clone()];
        let (name, disposition, label_text, detail): (
            &'static str,
            Disposition,
            Option<&[u8]>,
            Option<String>,
        ) = match kind {
            PamLine::EndHdr => ("ENDHDR", Disposition::Structure, None, None),
            PamLine::Key(k) => {
                if last(k) == Some(i) {
                    (k, Disposition::Structure, None, None)
                } else {
                    (
                        k,
                        Disposition::Dropped,
                        None,
                        Some("overridden by a later line".into()),
                    )
                }
            }
            PamLine::Tupltype => {
                let t = core::str::from_utf8(text).unwrap_or("").trim();
                let v = &t.as_bytes()["TUPLTYPE ".len().min(t.len())..];
                (
                    "TUPLTYPE",
                    Disposition::Dropped,
                    Some(v),
                    Some("parsed and dropped (parse_p7_header)".into()),
                )
            }
            PamLine::Comment => {
                let t = core::str::from_utf8(text).unwrap_or("").trim();
                (
                    "comment",
                    Disposition::Skipped,
                    Some(&t.as_bytes()[1.min(t.len())..]),
                    None,
                )
            }
            PamLine::Blank => ("blank", Disposition::Padding, None, None),
            PamLine::Unknown => (
                "unknown",
                Disposition::Unknown,
                Some(text),
                Some("unrecognised PAM header line, ignored".into()),
            ),
        };
        let mut p = Part::new(
            PartKind::Attribute,
            PartTag::Name(name.into()),
            s..e,
            disposition,
        );
        if let Some(l) = label_text {
            p = p.with_label(label(l));
        }
        if let Some(d) = detail {
            p = p.with_detail(d);
        }
        inv.push(Some(header), p)?;
    }
    Ok(())
}

// ── pixel data ───────────────────────────────────────────────────────

fn walk_pixels(inv: &mut Inventory, data: &[u8], h: &PnmHeader, start: u64) -> Res<()> {
    let len = data.len() as u64;
    let ascii = matches!(data[1], b'1' | b'2' | b'3');
    let pixels = |inv: &mut Inventory, range: Range<u64>, detail: Option<String>| -> Res<PartId> {
        // A truncated file is rejected by every PNM decode path.
        let disposition = if detail
            .as_deref()
            .is_some_and(|d| d.starts_with("truncated"))
        {
            Disposition::Malformed
        } else {
            Disposition::ImageData
        };
        let mut p = Part::new(
            PartKind::Block,
            PartTag::Name("pixels".into()),
            range,
            disposition,
        );
        if let Some(d) = detail {
            p = p.with_detail(d);
        }
        inv.push(None, p)
    };

    if !ascii {
        let (w, ht, depth) = (
            u128::from(h.width),
            u128::from(h.height),
            u128::from(h.depth),
        );
        let need: u128 = match h.format {
            PnmFormat::Pbm => w.div_ceil(8) * ht,
            PnmFormat::Pfm => w * ht * depth * 4,
            _ => w * ht * depth * if h.maxval > 255 { 2 } else { 1 },
        };
        let avail = u128::from(len - start);
        if need <= avail {
            let end = start + need as u64;
            pixels(inv, start..end, None)?;
            trailer(inv, end, concatenated_note(data, end))?;
        } else {
            pixels(
                inv,
                start..len,
                Some(format!(
                    "truncated: the decoder needs {need} bytes, the file has {avail}; the decoder rejects the file"
                )),
            )?;
        }
        return Ok(());
    }

    let total = u128::from(h.width) * u128::from(h.height) * u128::from(h.depth);
    let mut comments = Vec::new();
    let outcome = scan_ascii(
        data,
        start as usize,
        total,
        h.format == PnmFormat::Pbm,
        &mut comments,
    );
    if comments.len() >= MAX_RECORDED {
        return Err(too_many());
    }
    let (end, detail, bad) = match outcome {
        AsciiEnd::Done(end) => (end as u64, None, None),
        AsciiEnd::Eof => (
            len,
            Some(
                "truncated: the file ends before all samples are read; the decoder rejects the file"
                    .into(),
            ),
            None,
        ),
        AsciiEnd::Bad(at, why) => (at as u64, None, Some((at as u64, why))),
    };
    if end > start {
        let id = pixels(inv, start..end, detail)?;
        for r in comments.iter().filter(|r| (r.end as u64) <= end) {
            inv.push(
                Some(id),
                Part::new(
                    PartKind::Block,
                    PartTag::Name("comment".into()),
                    r.start as u64..r.end as u64,
                    Disposition::Skipped,
                )
                .with_label(label(&data[r.start + 1..r.end])),
            )?;
        }
    }
    match bad {
        Some((at, why)) => malformed_rest(
            inv,
            None,
            at.max(start),
            len,
            format!("the decoder rejects the pixel data: {why}"),
        )?,
        None => trailer(inv, end, concatenated_note(data, end))?,
    }
    Ok(())
}

/// A detail for trailing bytes that start another PNM image.
fn concatenated_note(data: &[u8], from: u64) -> Option<String> {
    let rest = data.get(from as usize..)?;
    let rest = {
        let n = rest.iter().take_while(|b| b.is_ascii_whitespace()).count();
        &rest[n..]
    };
    (rest.len() >= 3
        && matches!(
            &rest[..2],
            b"P1" | b"P2" | b"P3" | b"P4" | b"P5" | b"P6" | b"P7" | b"Pf" | b"PF"
        ))
    .then(|| {
        format!(
            "starts with {}: a concatenated PNM image, which the decoder never reads",
            label(&rest[..2])
        )
    })
}

enum AsciiEnd {
    Done(usize),
    Eof,
    Bad(usize, &'static str),
}

/// Walk `total` ASCII samples the way `decode_ascii_pbm` / `decode_ascii_samples`
/// do, recording comments between them.
fn scan_ascii(
    data: &[u8],
    mut pos: usize,
    total: u128,
    pbm: bool,
    comments: &mut Vec<Range<usize>>,
) -> AsciiEnd {
    let mut i: u128 = 0;
    while i < total {
        // whitespace and comments
        while pos < data.len() {
            match data[pos] {
                b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
                b'#' => {
                    let s = pos;
                    while pos < data.len() && data[pos] != b'\n' {
                        pos += 1;
                    }
                    if comments.len() < MAX_RECORDED {
                        comments.push(s..pos);
                    }
                }
                _ => break,
            }
        }
        if pos >= data.len() {
            return AsciiEnd::Eof;
        }
        if pbm {
            if !matches!(data[pos], b'0' | b'1') {
                return AsciiEnd::Bad(pos, "P1 expects '0' or '1'");
            }
            pos += 1;
        } else {
            let start = pos;
            while pos < data.len() && data[pos].is_ascii_digit() {
                pos += 1;
            }
            if pos == start {
                return AsciiEnd::Bad(start, "expected a decimal sample");
            }
            if core::str::from_utf8(&data[start..pos])
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .is_none()
            {
                return AsciiEnd::Bad(start, "sample does not fit in 32 bits");
            }
        }
        i += 1;
    }
    AsciiEnd::Done(pos)
}
