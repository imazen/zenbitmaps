//! Farbfeld walker: 8-byte magic, big-endian width and height, then
//! `width*height*8` bytes of RGBA16. The decoder reads exactly that and
//! ignores anything after it (`crate::farbfeld::decode::decode_pixels`).

use alloc::format;

use zencodec::ImageFormat;
use zencodec::inventory::{Disposition, Inventory, Part, PartKind, PartTag};

use super::{Res, malformed_rest, trailer, u32_be};

pub(crate) fn walk(data: &[u8]) -> Res<Inventory> {
    let len = data.len() as u64;
    let mut inv = Inventory::new(ImageFormat::Farbfeld, len);
    if data.is_empty() {
        return Ok(inv);
    }
    if len < 16 || &data[..8] != b"farbfeld" {
        let why = if len < 16 && data.starts_with(&b"farbfeld"[..data.len().min(8)]) {
            "truncated farbfeld header"
        } else {
            "not a farbfeld header"
        };
        malformed_rest(&mut inv, None, 0, len, why)?;
        return Ok(inv);
    }
    let w = u64::from(u32_be(data, 8).unwrap_or(0));
    let h = u64::from(u32_be(data, 12).unwrap_or(0));
    let header = Part::new(
        PartKind::Header,
        PartTag::None,
        0..16,
        Disposition::Structure,
    )
    .with_label("farbfeld");
    if w == 0 || h == 0 {
        inv.push(
            None,
            header.with_detail("the decoder rejects a zero width or height"),
        )?;
        malformed_rest(&mut inv, None, 16, len, "after a rejected header")?;
        return Ok(inv);
    }
    inv.push(None, header)?;
    let need = u128::from(w) * u128::from(h) * 8;
    let avail = u128::from(len - 16);
    if need <= avail {
        let end = 16 + need as u64;
        inv.push(
            None,
            Part::new(
                PartKind::Block,
                PartTag::Name("pixels".into()),
                16..end,
                Disposition::ImageData,
            ),
        )?;
        trailer(&mut inv, end, None)?;
    } else if avail > 0 {
        inv.push(
            None,
            Part::new(
                PartKind::Block,
                PartTag::Name("pixels".into()),
                16..len,
                Disposition::Malformed,
            )
            .with_detail(format!(
                "truncated: the decoder needs {need} bytes, the file has {avail}; the decoder rejects the file"
            )),
        )?;
    }
    inv.fill_gaps(None, Disposition::Trailing)?;
    Ok(inv)
}
