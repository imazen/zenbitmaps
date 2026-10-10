//! Structural inventories (`zencodec::inventory`) for the bitmap formats.
//!
//! Each submodule walks one container without decoding pixels and reports
//! every byte range with what the zencodec decode path does with it. The
//! walkers never fail on malformed input: unreadable bytes become
//! [`Disposition::Malformed`] parts, and the only errors are part-cap hits.
//!
//! Where a walker needs to know how far a decoder reads (the end of the pixel
//! data), it mirrors the decoder's control flow in a dry run instead of
//! decoding. The pinned tests and the corpus tests in `tests/inventory.rs`
//! keep the two in step.

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use zencodec::inventory::{
    Disposition, Inventory, InventoryError, Part, PartId, PartKind, PartTag,
};

use crate::error::BitmapError;

pub(crate) mod farbfeld;
pub(crate) mod pnm;

#[cfg(feature = "bmp")]
pub(crate) mod bmp;
#[cfg(feature = "hdr")]
pub(crate) mod hdr;
#[cfg(feature = "qoi")]
pub(crate) mod qoi;
#[cfg(feature = "tga")]
pub(crate) mod tga;

pub(crate) type Res<T> = Result<T, InventoryError>;

/// Longest label copied from a file.
const LABEL_MAX: usize = 64;

/// A label from file bytes: at most [`LABEL_MAX`] bytes, lossy UTF-8, with
/// trailing NULs and ASCII whitespace trimmed.
pub(crate) fn label(bytes: &[u8]) -> Cow<'static, str> {
    let bytes = &bytes[..bytes.len().min(LABEL_MAX)];
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == 0 || bytes[end - 1].is_ascii_whitespace()) {
        end -= 1;
    }
    Cow::Owned(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

/// The bytes of a NUL-terminated field (the whole slice when no NUL).
pub(crate) fn cstr(bytes: &[u8]) -> &[u8] {
    let n = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..n]
}

pub(crate) fn u16_le(d: &[u8], at: u64) -> Option<u16> {
    let at = usize::try_from(at).ok()?;
    Some(u16::from_le_bytes(
        d.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

pub(crate) fn u32_le(d: &[u8], at: u64) -> Option<u32> {
    let at = usize::try_from(at).ok()?;
    Some(u32::from_le_bytes(
        d.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

pub(crate) fn u32_be(d: &[u8], at: u64) -> Option<u32> {
    let at = usize::try_from(at).ok()?;
    Some(u32::from_be_bytes(
        d.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// The input as a byte slice for a clipped range.
pub(crate) fn slice<'a>(d: &'a [u8], r: &Range<u64>) -> &'a [u8] {
    let s = usize::try_from(r.start).unwrap_or(usize::MAX).min(d.len());
    let e = usize::try_from(r.end).unwrap_or(usize::MAX).min(d.len());
    &d[s..e.max(s)]
}

/// `start..start+len`, clipped to the input. `None` when nothing is left.
pub(crate) fn clip(start: u64, len: u64, input_len: u64) -> Option<Range<u64>> {
    let end = start.checked_add(len)?.min(input_len);
    (start < end).then_some(start..end)
}

/// Top-level byte ranges already claimed, so optional parts (a profile, a
/// developer area) that a corrupt offset points into other parts are dropped
/// instead of producing overlapping siblings.
#[derive(Default)]
pub(crate) struct Claimed {
    ranges: Vec<Range<u64>>,
}

impl Claimed {
    pub(crate) fn is_free(&self, r: &Range<u64>) -> bool {
        self.ranges
            .iter()
            .all(|c| r.end <= c.start || r.start >= c.end)
    }

    pub(crate) fn claim(&mut self, r: Range<u64>) {
        self.ranges.push(r);
    }

    /// Claim `r` if it is free.
    pub(crate) fn try_claim(&mut self, r: &Range<u64>) -> bool {
        if self.is_free(r) {
            self.ranges.push(r.clone());
            true
        } else {
            false
        }
    }
}

/// Cover `from..len` with one `Malformed` gap carrying `why`.
pub(crate) fn malformed_rest(
    inv: &mut Inventory,
    parent: Option<PartId>,
    from: u64,
    to: u64,
    why: impl Into<String>,
) -> Res<()> {
    if from < to {
        inv.push(
            parent,
            Part::new(
                PartKind::Gap,
                PartTag::None,
                from..to,
                Disposition::Malformed,
            )
            .with_detail(why),
        )?;
    }
    Ok(())
}

/// Record the bytes after the logical end as a `Trailer` part.
pub(crate) fn trailer(inv: &mut Inventory, from: u64, detail: Option<String>) -> Res<()> {
    let len = inv.input_len();
    if from < len {
        let mut p = Part::new(
            PartKind::Trailer,
            PartTag::None,
            from..len,
            Disposition::Trailing,
        );
        if let Some(d) = detail {
            p = p.with_detail(d);
        }
        inv.push(None, p)?;
    }
    Ok(())
}

/// Cover every hole in `span` among the top-level parts with a gap part.
///
/// `disposition_for(hole)` picks the disposition and detail for each hole.
pub(crate) fn fill_top_level_holes(
    inv: &mut Inventory,
    span: Range<u64>,
    mut describe: impl FnMut(&Range<u64>) -> (Disposition, Option<String>),
) -> Res<()> {
    let mut holes = Vec::new();
    let mut cursor = span.start;
    for id in inv.children(None) {
        let r = inv.get(id).map(|p| p.range.clone()).unwrap_or(0..0);
        if r.start >= span.end {
            break;
        }
        if r.start > cursor {
            holes.push(cursor..r.start.min(span.end));
        }
        cursor = cursor.max(r.end);
    }
    if cursor < span.end {
        holes.push(cursor..span.end);
    }
    for hole in holes {
        let (disposition, detail) = describe(&hole);
        let mut p = Part::new(PartKind::Gap, PartTag::None, hole, disposition);
        if let Some(d) = detail {
            p = p.with_detail(d);
        }
        inv.push(None, p)?;
    }
    Ok(())
}

pub(crate) fn to_bitmap_error(e: InventoryError) -> BitmapError {
    BitmapError::LimitExceeded(format!("inventory: {e}"))
}
