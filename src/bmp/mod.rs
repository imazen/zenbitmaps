//! Full BMP image format decoder and basic encoder (internal).
//!
//! Use top-level [`crate::decode_bmp`], [`crate::encode_bmp`], etc.

pub(crate) mod decode;
mod encode;
mod utils;

use crate::decode::DecodeOutput;
use crate::error::BitmapError;
use crate::limits::Limits;
use crate::pixel::PixelLayout;
use alloc::vec::Vec;
pub use decode::BmpPermissiveness;
use enough::Stop;

/// Metadata extracted from a BMP file header.
///
/// Returned by [`crate::probe_bmp`]. Contains resolution and color table
/// information that is not part of the pixel decode output.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BmpMetadata {
    /// Image width in pixels.
    pub width: u32,

    /// Image height in pixels.
    pub height: u32,

    /// Pixel layout of the decoded output.
    pub layout: PixelLayout,

    /// Horizontal resolution in DPI, derived from `biXPelsPerMeter`.
    ///
    /// `None` if the BMP header did not contain resolution info (OS/2 v1
    /// or the field was zero).
    pub dpi_x: Option<f32>,

    /// Vertical resolution in DPI, derived from `biYPelsPerMeter`.
    ///
    /// `None` if the BMP header did not contain resolution info.
    pub dpi_y: Option<f32>,

    /// Color table entries in BGRA order (up to 256).
    ///
    /// Only present for indexed-color BMPs (1, 2, 4, or 8 bits per pixel).
    /// Each entry is `[B, G, R, A]` where A is typically 0 or 255.
    pub color_table: Option<Vec<[u8; 4]>>,
}

/// Decode BMP data (output in RGB/RGBA byte order).
pub(crate) fn decode<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    decode_with_permissiveness(data, limits, BmpPermissiveness::Standard, stop)
}

/// Decode BMP data with a specific permissiveness level.
pub(crate) fn decode_with_permissiveness<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    permissiveness: BmpPermissiveness,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    let header = decode::parse_bmp_header(data)?;
    check_limits(limits, header.width, header.height, &header.layout)?;
    stop.check()?;
    let (pixels, layout) = decode::decode_bmp_pixels(data, permissiveness, stop)?;
    Ok(DecodeOutput::owned(
        pixels,
        header.width,
        header.height,
        layout,
    ))
}

/// Decode BMP data in native byte order (BGR/BGRA — no channel swizzle).
pub(crate) fn decode_native<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    let header = decode::parse_bmp_header(data)?;
    check_limits(limits, header.width, header.height, &header.layout)?;
    stop.check()?;
    let (pixels, native_layout) =
        decode::decode_bmp_pixels_native(data, BmpPermissiveness::Standard, stop)?;
    Ok(DecodeOutput::owned(
        pixels,
        header.width,
        header.height,
        native_layout,
    ))
}

fn check_limits(
    limits: Option<&Limits>,
    width: u32,
    height: u32,
    layout: &PixelLayout,
) -> Result<(), BitmapError> {
    if let Some(limits) = limits {
        limits.check(width, height)?;
    }
    let out_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(layout.bytes_per_pixel()))
        .ok_or_else(|| BitmapError::LimitExceeded("output size overflows usize".into()))?;
    if let Some(limits) = limits {
        limits.check_memory(out_bytes)?;
    }
    Ok(())
}

/// Probe BMP metadata without decoding pixels.
pub(crate) fn probe(data: &[u8]) -> Result<BmpMetadata, BitmapError> {
    let header = decode::parse_bmp_header(data)?;

    /// Convert pixels-per-meter to DPI. Returns None if the value is 0.
    fn pels_to_dpi(pels: u32) -> Option<f32> {
        if pels == 0 {
            None
        } else {
            Some(pels as f32 * 0.0254)
        }
    }

    Ok(BmpMetadata {
        width: header.width,
        height: header.height,
        layout: header.layout,
        dpi_x: pels_to_dpi(header.x_pels_per_meter),
        dpi_y: pels_to_dpi(header.y_pels_per_meter),
        color_table: header.color_table,
    })
}

/// Encode to BMP.
pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    alpha: bool,
    stop: &dyn Stop,
) -> Result<Vec<u8>, BitmapError> {
    encode::encode_bmp(pixels, width, height, layout, alpha, stop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use enough::Unstoppable;

    /// Helper: build a minimal 24-bit 1x1 BMP with given resolution fields.
    fn make_bmp_with_resolution(x_pels: u32, y_pels: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        // ── File header (14 bytes) ──
        buf.extend_from_slice(b"BM");
        // File size: 14 (file header) + 40 (DIB header) + 4 (padded row) = 58
        buf.extend_from_slice(&58u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // reserved
        buf.extend_from_slice(&54u32.to_le_bytes()); // data offset

        // ── DIB header (BITMAPINFOHEADER, 40 bytes) ──
        buf.extend_from_slice(&40u32.to_le_bytes()); // header size
        buf.extend_from_slice(&1i32.to_le_bytes()); // width
        buf.extend_from_slice(&1i32.to_le_bytes()); // height (positive = bottom-up)
        buf.extend_from_slice(&1u16.to_le_bytes()); // planes
        buf.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
        buf.extend_from_slice(&0u32.to_le_bytes()); // compression (RGB)
        buf.extend_from_slice(&4u32.to_le_bytes()); // image data size (1 pixel + 1 pad = 4)
        buf.extend_from_slice(&x_pels.to_le_bytes()); // X pixels per meter
        buf.extend_from_slice(&y_pels.to_le_bytes()); // Y pixels per meter
        buf.extend_from_slice(&0u32.to_le_bytes()); // colors used
        buf.extend_from_slice(&0u32.to_le_bytes()); // important colors

        // ── Pixel data: 1 BGR pixel + 1 byte padding ──
        buf.extend_from_slice(&[0xFF, 0x00, 0x00, 0x00]); // blue pixel + pad

        buf
    }

    /// Helper: build a minimal 8-bit indexed 2x1 BMP with a 4-color palette.
    fn make_indexed_bmp() -> Vec<u8> {
        let mut buf = Vec::new();
        let palette_bytes = 4 * 4; // 4 colors, 4 bytes each
        let row_stride = 4; // 2 pixels padded to 4-byte boundary
        let pixel_data_size = row_stride; // 1 row
        let data_offset = 14 + 40 + palette_bytes;
        let file_size = data_offset + pixel_data_size;

        // ── File header (14 bytes) ──
        buf.extend_from_slice(b"BM");
        buf.extend_from_slice(&(file_size as u32).to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // reserved
        buf.extend_from_slice(&(data_offset as u32).to_le_bytes());

        // ── DIB header (BITMAPINFOHEADER, 40 bytes) ──
        buf.extend_from_slice(&40u32.to_le_bytes()); // header size
        buf.extend_from_slice(&2i32.to_le_bytes()); // width
        buf.extend_from_slice(&1i32.to_le_bytes()); // height
        buf.extend_from_slice(&1u16.to_le_bytes()); // planes
        buf.extend_from_slice(&8u16.to_le_bytes()); // bits per pixel
        buf.extend_from_slice(&0u32.to_le_bytes()); // compression (RGB)
        buf.extend_from_slice(&(pixel_data_size as u32).to_le_bytes());
        buf.extend_from_slice(&2835u32.to_le_bytes()); // X pixels per meter (~72 DPI)
        buf.extend_from_slice(&2835u32.to_le_bytes()); // Y pixels per meter
        buf.extend_from_slice(&4u32.to_le_bytes()); // colors used
        buf.extend_from_slice(&0u32.to_le_bytes()); // important colors

        // ── Color table (4 entries, BGRA) ──
        buf.extend_from_slice(&[0xFF, 0x00, 0x00, 0x00]); // entry 0: blue
        buf.extend_from_slice(&[0x00, 0xFF, 0x00, 0x00]); // entry 1: green
        buf.extend_from_slice(&[0x00, 0x00, 0xFF, 0x00]); // entry 2: red
        buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0x00]); // entry 3: white

        // ── Pixel data: 2 index bytes + 2 padding ──
        buf.extend_from_slice(&[0x00, 0x02, 0x00, 0x00]); // pixel 0=blue, pixel 1=red

        buf
    }

    #[test]
    fn probe_dpi_72() {
        // 2835 pels/meter = 72.009 DPI
        let bmp = make_bmp_with_resolution(2835, 2835);
        let meta = probe(&bmp).unwrap();
        let dpi_x = meta.dpi_x.unwrap();
        let dpi_y = meta.dpi_y.unwrap();
        assert!((dpi_x - 72.0).abs() < 0.1, "expected ~72 DPI, got {dpi_x}");
        assert!((dpi_y - 72.0).abs() < 0.1, "expected ~72 DPI, got {dpi_y}");
        assert_eq!(meta.width, 1);
        assert_eq!(meta.height, 1);
        assert!(meta.color_table.is_none()); // 24-bit, no palette
    }

    #[test]
    fn probe_dpi_300() {
        // 11811 pels/meter ~= 300 DPI
        let bmp = make_bmp_with_resolution(11811, 11811);
        let meta = probe(&bmp).unwrap();
        let dpi_x = meta.dpi_x.unwrap();
        let dpi_y = meta.dpi_y.unwrap();
        assert!(
            (dpi_x - 300.0).abs() < 0.1,
            "expected ~300 DPI, got {dpi_x}"
        );
        assert!(
            (dpi_y - 300.0).abs() < 0.1,
            "expected ~300 DPI, got {dpi_y}"
        );
    }

    #[test]
    fn probe_dpi_zero() {
        let bmp = make_bmp_with_resolution(0, 0);
        let meta = probe(&bmp).unwrap();
        assert_eq!(meta.dpi_x, None);
        assert_eq!(meta.dpi_y, None);
    }

    #[test]
    fn probe_dpi_asymmetric() {
        // 3937 pels/meter ~= 100 DPI, 7874 pels/meter ~= 200 DPI
        let bmp = make_bmp_with_resolution(3937, 7874);
        let meta = probe(&bmp).unwrap();
        let dpi_x = meta.dpi_x.unwrap();
        let dpi_y = meta.dpi_y.unwrap();
        assert!(
            (dpi_x - 100.0).abs() < 0.1,
            "expected ~100 DPI, got {dpi_x}"
        );
        assert!(
            (dpi_y - 200.0).abs() < 0.1,
            "expected ~200 DPI, got {dpi_y}"
        );
    }

    #[test]
    fn probe_color_table() {
        let bmp = make_indexed_bmp();
        let meta = probe(&bmp).unwrap();

        let table = meta.color_table.as_ref().expect("expected color table");
        assert_eq!(table.len(), 4);
        // Entries are BGRA. The BMP decoder forces alpha to 255 for palette entries.
        assert_eq!(table[0], [0xFF, 0x00, 0x00, 0xFF]); // blue
        assert_eq!(table[1], [0x00, 0xFF, 0x00, 0xFF]); // green
        assert_eq!(table[2], [0x00, 0x00, 0xFF, 0xFF]); // red
        assert_eq!(table[3], [0xFF, 0xFF, 0xFF, 0xFF]); // white
    }

    #[test]
    fn probe_no_color_table_for_24bit() {
        let bmp = make_bmp_with_resolution(2835, 2835);
        let meta = probe(&bmp).unwrap();
        assert!(meta.color_table.is_none());
    }

    #[test]
    fn roundtrip_encode_preserves_dpi() {
        // Encode a 1x1 BMP and verify the DPI in the output.
        // The encoder hardcodes 2835 (72 DPI).
        let pixels = [0xFF, 0x00, 0x00]; // one RGB pixel
        let encoded = encode(&pixels, 1, 1, PixelLayout::Rgb8, false, &Unstoppable).unwrap();
        let meta = probe(&encoded).unwrap();
        let dpi_x = meta.dpi_x.unwrap();
        assert!(
            (dpi_x - 72.0).abs() < 0.1,
            "expected ~72 DPI in encoder output, got {dpi_x}"
        );
    }
}
