//! Full BMP decoder supporting all standard bit depths, RLE, and bitfields.
//!
//! Forked from zune-bmp 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib).
//! Adapted: ZReader → &[u8] cursor, DecoderOptions → Option<&Limits>,
//! BmpDecoderErrors → PnmError, log removed, stop.check() added.

use alloc::vec;
use alloc::vec::Vec;

use enough::Stop;

use super::utils::{expand_bits_to_byte, shift_signed};
use crate::error::PnmError;
use crate::pixel::PixelLayout;

// ── Permissiveness ──────────────────────────────────────────────────

/// Controls how strictly the BMP decoder validates input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BmpPermissiveness {
    /// Reject files that violate the BMP spec even in non-critical ways.
    /// Validates: planes == 1, file size matches, palette count, DPI
    /// values, image data size field, no RLE + top-down.
    Strict,

    /// Default behavior. Accept common spec deviations that don't
    /// affect correct pixel decoding (bad file size, bad DPI,
    /// bad image data size field). Reject: planes != 1,
    /// RLE + top-down, oversized palette, out-of-range palette indices.
    #[default]
    Standard,

    /// Accept as much as possible. Zero-pad truncated files,
    /// clamp RLE row overflows, accept unknown compression
    /// (zero-fill output), ignore planes/palette/topdown checks.
    Permissive,
}

// ── Compression enum ────────────────────────────────────────────────

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
enum BmpCompression {
    Rgb,
    Rle8,
    Rle4,
    Bitfields,
    /// Unknown compression type (only used in Permissive mode).
    Unknown(u32),
}

impl BmpCompression {
    fn from_u32(num: u32, permissive: bool) -> Option<Self> {
        match num {
            0 => Some(Self::Rgb),
            1 => Some(Self::Rle8),
            2 => Some(Self::Rle4),
            3 | 6 => Some(Self::Bitfields), // 6 = BI_ALPHABITFIELDS
            other if permissive => Some(Self::Unknown(other)),
            _ => None,
        }
    }
}

// ── Pixel format enum ───────────────────────────────────────────────

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum BmpPixelFormat {
    None,
    Rgba,
    Pal8,
    Gray8,
    Rgb,
}

impl BmpPixelFormat {
    fn num_components(self) -> usize {
        match self {
            Self::None => 0,
            Self::Rgba => 4,
            Self::Pal8 | Self::Rgb => 3,
            Self::Gray8 => 1,
        }
    }
}

// ── Palette entry ───────────────────────────────────────────────────

#[derive(Clone, Copy, Default, Debug)]
struct PaletteEntry {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

// ── Cursor for reading from &[u8] ───────────────────────────────────

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    /// When true, reads beyond EOF return zeros instead of errors.
    permissive: bool,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            permissive: false,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn set_position(&mut self, pos: usize) -> Result<(), PnmError> {
        if pos > self.data.len() {
            if self.permissive {
                self.pos = self.data.len();
                return Ok(());
            }
            return Err(PnmError::UnexpectedEof);
        }
        self.pos = pos;
        Ok(())
    }

    fn skip(&mut self, n: usize) -> Result<(), PnmError> {
        let new_pos = self.pos.checked_add(n).ok_or(PnmError::UnexpectedEof)?;
        if new_pos > self.data.len() {
            if self.permissive {
                self.pos = self.data.len();
                return Ok(());
            }
            return Err(PnmError::UnexpectedEof);
        }
        self.pos = new_pos;
        Ok(())
    }

    fn read_u8(&mut self) -> u8 {
        if self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            b
        } else {
            0
        }
    }

    fn read_u8_err(&mut self) -> Result<u8, PnmError> {
        if self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            Ok(b)
        } else {
            Err(PnmError::UnexpectedEof)
        }
    }

    fn get_u16_le_err(&mut self) -> Result<u16, PnmError> {
        if self.pos + 2 > self.data.len() {
            return Err(PnmError::UnexpectedEof);
        }
        let val = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(val)
    }

    fn get_u16_be(&mut self) -> u16 {
        if self.pos + 2 > self.data.len() {
            return 0;
        }
        let val = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        val
    }

    fn get_u32_le_err(&mut self) -> Result<u32, PnmError> {
        if self.pos + 4 > self.data.len() {
            return Err(PnmError::UnexpectedEof);
        }
        let val = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(val)
    }

    fn get_u32_le(&mut self) -> u32 {
        self.get_u32_le_err().unwrap_or(0)
    }

    fn read_fixed_bytes<const N: usize>(&mut self) -> Result<[u8; N], PnmError> {
        if self.pos + N > self.data.len() {
            if self.permissive {
                let mut buf = [0u8; N];
                let available = self.data.len().saturating_sub(self.pos);
                buf[..available].copy_from_slice(&self.data[self.pos..self.pos + available]);
                self.pos = self.data.len();
                return Ok(buf);
            }
            return Err(PnmError::UnexpectedEof);
        }
        let mut buf = [0u8; N];
        buf.copy_from_slice(&self.data[self.pos..self.pos + N]);
        self.pos += N;
        Ok(buf)
    }

    fn read_fixed_bytes_or_zero<const N: usize>(&mut self) -> [u8; N] {
        self.read_fixed_bytes().unwrap_or([0u8; N])
    }

    fn read_exact_bytes(&mut self, buf: &mut [u8]) -> Result<(), PnmError> {
        let n = buf.len();
        if self.pos + n > self.data.len() {
            if self.permissive {
                let available = self.data.len().saturating_sub(self.pos);
                buf[..available].copy_from_slice(&self.data[self.pos..self.pos + available]);
                buf[available..].fill(0);
                self.pos = self.data.len();
                return Ok(());
            }
            return Err(PnmError::UnexpectedEof);
        }
        buf.copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(())
    }
}

// ── Parsed BMP header info ──────────────────────────────────────────

pub(crate) struct BmpHeader {
    pub width: u32,
    pub height: u32,
    pub layout: PixelLayout,
}

// ── Public header parsing (for probe) ───────────────────────────────

/// Parse a BMP header to extract dimensions and pixel format.
/// This is the header-only fast path for probing.
pub(crate) fn parse_bmp_header(data: &[u8]) -> Result<BmpHeader, PnmError> {
    // Header probing uses Permissive to avoid rejecting files before
    // the caller has chosen a permissiveness level.
    let mut dec = BmpDecoderState::new(data, BmpPermissiveness::Permissive);
    dec.decode_headers()?;

    let layout = match dec.pix_fmt {
        BmpPixelFormat::Rgba => PixelLayout::Rgba8,
        BmpPixelFormat::Rgb | BmpPixelFormat::Pal8 => PixelLayout::Rgb8,
        BmpPixelFormat::Gray8 => PixelLayout::Gray8,
        BmpPixelFormat::None => {
            return Err(PnmError::UnsupportedVariant(
                "unsupported BMP pixel format".into(),
            ));
        }
    };

    Ok(BmpHeader {
        width: dec.width as u32,
        height: dec.height as u32,
        layout,
    })
}

// ── Full decode ─────────────────────────────────────────────────────

/// Decode BMP pixel data (RGB/RGBA output).
pub(crate) fn decode_bmp_pixels(
    data: &[u8],
    permissiveness: BmpPermissiveness,
    stop: &dyn Stop,
) -> Result<(Vec<u8>, PixelLayout), PnmError> {
    let mut dec = BmpDecoderState::new(data, permissiveness);
    dec.decode_headers()?;

    let output_size = dec.output_buf_size()?;
    let mut buf = vec![0u8; output_size];

    stop.check()?;
    dec.decode_into::<false>(&mut buf, stop)?;

    let layout = match dec.pix_fmt {
        BmpPixelFormat::Rgba => PixelLayout::Rgba8,
        BmpPixelFormat::Rgb | BmpPixelFormat::Pal8 => PixelLayout::Rgb8,
        BmpPixelFormat::Gray8 => PixelLayout::Gray8,
        BmpPixelFormat::None => {
            return Err(PnmError::UnsupportedVariant(
                "unsupported BMP pixel format".into(),
            ));
        }
    };

    Ok((buf, layout))
}

/// Decode BMP pixel data in native byte order (BGR/BGRA).
pub(crate) fn decode_bmp_pixels_native(
    data: &[u8],
    permissiveness: BmpPermissiveness,
    stop: &dyn Stop,
) -> Result<(Vec<u8>, PixelLayout), PnmError> {
    let mut dec = BmpDecoderState::new(data, permissiveness);
    dec.decode_headers()?;

    let output_size = dec.output_buf_size()?;
    let mut buf = vec![0u8; output_size];

    stop.check()?;
    dec.decode_into::<true>(&mut buf, stop)?;

    let layout = match dec.pix_fmt {
        BmpPixelFormat::Rgba => PixelLayout::Bgra8,
        BmpPixelFormat::Rgb | BmpPixelFormat::Pal8 => PixelLayout::Bgr8,
        BmpPixelFormat::Gray8 => PixelLayout::Gray8,
        BmpPixelFormat::None => {
            return Err(PnmError::UnsupportedVariant(
                "unsupported BMP pixel format".into(),
            ));
        }
    };

    Ok((buf, layout))
}

// ── Internal decoder state ──────────────────────────────────────────

struct BmpDecoderState<'a> {
    bytes: Cursor<'a>,
    width: usize,
    height: usize,
    flip_vertically: bool,
    rgb_bitfields: [u32; 4],
    decoded_headers: bool,
    pix_fmt: BmpPixelFormat,
    comp: BmpCompression,
    ihsize: u32,
    hsize: u32,
    palette: [PaletteEntry; 256],
    depth: u16,
    is_alpha: bool,
    palette_numbers: usize,
    image_in_bgra: bool,
    permissiveness: BmpPermissiveness,
}

impl<'a> BmpDecoderState<'a> {
    fn new(data: &'a [u8], permissiveness: BmpPermissiveness) -> Self {
        let mut cursor = Cursor::new(data);
        cursor.permissive = permissiveness == BmpPermissiveness::Permissive;
        Self {
            bytes: cursor,
            width: 0,
            height: 0,
            flip_vertically: false,
            rgb_bitfields: [0; 4],
            decoded_headers: false,
            pix_fmt: BmpPixelFormat::None,
            comp: BmpCompression::Rgb,
            ihsize: 0,
            hsize: 0,
            palette: [PaletteEntry::default(); 256],
            depth: 0,
            is_alpha: false,
            palette_numbers: 0,
            image_in_bgra: false,
            permissiveness,
        }
    }

    #[allow(unused_assignments)]
    fn decode_headers(&mut self) -> Result<(), PnmError> {
        if self.decoded_headers {
            return Ok(());
        }

        let is_strict = self.permissiveness == BmpPermissiveness::Strict;
        let is_permissive = self.permissiveness == BmpPermissiveness::Permissive;
        let data_len = self.bytes.data.len();

        if self.bytes.read_u8_err()? != b'B' || self.bytes.read_u8_err()? != b'M' {
            return Err(PnmError::UnrecognizedFormat);
        }

        // File size field (offset 2)
        let file_size_field = self.bytes.get_u32_le_err()?;
        // Reserved (4 bytes)
        self.bytes.skip(4)?;

        // Strict: validate file size field matches actual data length
        if is_strict && file_size_field != 0 && file_size_field as usize != data_len {
            return Err(PnmError::InvalidHeader(alloc::format!(
                "BMP file size field ({file_size_field}) doesn't match actual size ({data_len})"
            )));
        }

        let hsize = self.bytes.get_u32_le_err()?;
        let ihsize = self.bytes.get_u32_le_err()?;

        if ihsize.saturating_add(14) > hsize {
            return Err(PnmError::InvalidHeader("invalid BMP header size".into()));
        }

        let (width, height, planes, bpp, compression);
        match ihsize {
            12 => {
                // OS/2 BMPv1
                width = self.bytes.get_u16_le_err()? as u32;
                height = self.bytes.get_u16_le_err()? as u32;
                planes = self.bytes.get_u16_le_err()?;
                bpp = self.bytes.get_u16_le_err()?;
                compression = BmpCompression::Rgb;
            }
            16 | 40 | 52 | 56 | 64 | 108 | 124 => {
                width = self.bytes.get_u32_le_err()?;
                height = self.bytes.get_u32_le_err()?;
                planes = self.bytes.get_u16_le_err()?;
                bpp = self.bytes.get_u16_le_err()?;
                compression = if ihsize >= 40 {
                    match BmpCompression::from_u32(self.bytes.get_u32_le_err()?, is_permissive) {
                        Some(c) => c,
                        None => {
                            return Err(PnmError::UnsupportedVariant(
                                "unsupported BMP compression scheme".into(),
                            ));
                        }
                    }
                } else {
                    BmpCompression::Rgb
                };

                if ihsize > 16 {
                    let image_size_field = self.bytes.get_u32_le_err()?;
                    let x_pixels = self.bytes.get_u32_le_err()?;
                    let y_pixels = self.bytes.get_u32_le_err()?;
                    let _color_used = self.bytes.get_u32_le_err()?;
                    let _important_colors = self.bytes.get_u32_le_err()?;

                    // Strict: validate DPI and image data size fields
                    if is_strict {
                        // DPI values interpreted as i32 should be non-negative
                        if (x_pixels as i32) < 0 {
                            return Err(PnmError::InvalidHeader(alloc::format!(
                                "BMP horizontal resolution is negative ({})",
                                x_pixels as i32
                            )));
                        }
                        if (y_pixels as i32) < 0 {
                            return Err(PnmError::InvalidHeader(alloc::format!(
                                "BMP vertical resolution is negative ({})",
                                y_pixels as i32
                            )));
                        }
                        // Image data size should be 0 or match expected (for uncompressed)
                        if image_size_field != 0 && compression == BmpCompression::Rgb && width > 0
                        {
                            let row_bytes = (width as usize * bpp as usize).div_ceil(32) * 4;
                            let expected_size = row_bytes * (height as i32).unsigned_abs() as usize;
                            if image_size_field as usize != expected_size {
                                return Err(PnmError::InvalidHeader(alloc::format!(
                                    "BMP image data size field ({image_size_field}) doesn't match expected ({expected_size})"
                                )));
                            }
                        }
                    }

                    // Bitfield masks: embedded in header for ihsize >= 52
                    // (BITMAPV2INFOHEADER+), or external (after 40-byte header)
                    // when compression is BI_BITFIELDS.
                    if ihsize >= 52 || compression == BmpCompression::Bitfields {
                        self.rgb_bitfields[0] = self.bytes.get_u32_le_err()?;
                        self.rgb_bitfields[1] = self.bytes.get_u32_le_err()?;
                        self.rgb_bitfields[2] = self.bytes.get_u32_le_err()?;
                    }

                    let mut _colorspace_type: u32 = 0;

                    if ihsize > 40 {
                        // Alpha mask (V4+)
                        self.rgb_bitfields[3] = self.bytes.get_u32_le_err()?;
                        _colorspace_type = self.bytes.get_u32_le_err()?;

                        // Color primaries (9 fixed-point values) + gamma (3)
                        self.bytes.skip(4 * 9)?; // primaries
                        self.bytes.skip(4 * 3)?; // gamma
                    }

                    if ihsize > 108 {
                        // BMP v5: intent, ICC profile data/size, reserved
                        let _intent = self.bytes.get_u32_le_err()?;
                        let _profile_data = self.bytes.get_u32_le_err()?;
                        let _profile_size = self.bytes.get_u32_le_err()?;
                        // Skip reserved
                        self.bytes.skip(4)?;
                    }
                }
            }
            _ => {
                return Err(PnmError::InvalidHeader(alloc::format!(
                    "unknown BMP info header size: {ihsize}"
                )));
            }
        }

        // Planes validation (Standard and Strict reject planes != 1)
        if !is_permissive && planes != 1 {
            return Err(PnmError::InvalidHeader(alloc::format!(
                "BMP planes field is {planes}, expected 1"
            )));
        }

        self.flip_vertically = (height as i32) > 0;
        self.height = (height as i32).unsigned_abs() as usize;
        self.width = width as usize;

        if self.width == 0 {
            return Err(PnmError::InvalidHeader("BMP width is zero".into()));
        }
        if self.height == 0 {
            return Err(PnmError::InvalidHeader("BMP height is zero".into()));
        }

        // RLE + top-down is forbidden by spec (Standard and Strict reject)
        if !is_permissive
            && !self.flip_vertically
            && matches!(compression, BmpCompression::Rle4 | BmpCompression::Rle8)
        {
            return Err(PnmError::InvalidData(
                "RLE compression with top-down row order is forbidden by BMP spec".into(),
            ));
        }

        if bpp == 0 {
            return Err(PnmError::InvalidHeader("BMP bit depth is zero".into()));
        }

        match bpp {
            32 => self.pix_fmt = BmpPixelFormat::Rgba,
            24 => self.pix_fmt = BmpPixelFormat::Rgb,
            16 => {
                if compression == BmpCompression::Rgb {
                    self.pix_fmt = BmpPixelFormat::Rgb;
                } else if compression == BmpCompression::Bitfields {
                    // Only RGBA if the alpha mask is non-zero
                    if self.rgb_bitfields[3] != 0 {
                        self.pix_fmt = BmpPixelFormat::Rgba;
                    } else {
                        self.pix_fmt = BmpPixelFormat::Rgb;
                    }
                } else if matches!(compression, BmpCompression::Unknown(_)) {
                    self.pix_fmt = BmpPixelFormat::Rgb;
                }
            }
            8 => {
                if hsize.wrapping_sub(ihsize).wrapping_sub(14) > 0 {
                    self.pix_fmt = BmpPixelFormat::Pal8;
                } else {
                    self.pix_fmt = BmpPixelFormat::Gray8;
                }
            }
            1 | 2 | 4 => {
                if hsize.wrapping_sub(ihsize).wrapping_sub(14) > 0 {
                    self.pix_fmt = BmpPixelFormat::Pal8;
                } else {
                    return Err(PnmError::UnsupportedVariant(alloc::format!(
                        "unknown palette for {}-color BMP",
                        1u32 << bpp
                    )));
                }
            }
            _ => {
                return Err(PnmError::UnsupportedVariant(alloc::format!(
                    "BMP bit depth {bpp} unsupported"
                )));
            }
        }

        if self.pix_fmt == BmpPixelFormat::None {
            return Err(PnmError::UnsupportedVariant(
                "unsupported BMP pixel format".into(),
            ));
        }

        let p = self.hsize.wrapping_sub(self.ihsize).wrapping_sub(14);

        if self.pix_fmt == BmpPixelFormat::Pal8 {
            let max_colors = 1u32 << bpp;
            let mut colors = max_colors;

            if ihsize >= 36 {
                self.bytes.set_position(46)?;
                let t = self.bytes.get_u32_le_err()? as i32;
                if t < 0 || t > (1 << bpp) {
                    if !is_permissive {
                        return Err(PnmError::InvalidHeader(alloc::format!(
                            "BMP palette count ({t}) exceeds max for {bpp}-bit depth ({})",
                            1u32 << bpp
                        )));
                    }
                    // Permissive: clamp to max
                } else if t != 0 {
                    colors = t as u32;
                }
            } else {
                colors = 256.min(p / 3);
            }

            // Palette location
            self.bytes.set_position((14 + ihsize) as usize)?;

            // OS/2: 3 bytes per entry
            if ihsize == 12 {
                if p < colors * 3 {
                    return Err(PnmError::InvalidData("invalid BMP palette entries".into()));
                }
                for i in 0..colors.min(256) as usize {
                    let [b, g, r] = self.bytes.read_fixed_bytes_or_zero::<3>();
                    self.palette[i] = PaletteEntry {
                        red: r,
                        green: g,
                        blue: b,
                        alpha: 255,
                    };
                }
            } else {
                for i in 0..colors.min(256) as usize {
                    let [b, g, r, _] = self.bytes.read_fixed_bytes_or_zero::<4>();
                    self.palette[i] = PaletteEntry {
                        red: r,
                        green: g,
                        blue: b,
                        alpha: 255,
                    };
                }
            }
            self.palette_numbers = colors as usize;
        }

        self.comp = compression;
        self.depth = bpp;
        self.ihsize = ihsize;
        self.hsize = hsize;
        self.bytes.set_position(hsize as usize)?;
        self.decoded_headers = true;

        Ok(())
    }

    fn output_buf_size(&self) -> Result<usize, PnmError> {
        self.width
            .checked_mul(self.height)
            .and_then(|wh| wh.checked_mul(self.pix_fmt.num_components()))
            .ok_or(PnmError::DimensionsTooLarge {
                width: self.width as u32,
                height: self.height as u32,
            })
    }

    fn decode_into<const PRESERVE_BGRA: bool>(
        &mut self,
        buf: &mut [u8],
        stop: &dyn Stop,
    ) -> Result<(), PnmError> {
        let output_size = self.output_buf_size()?;
        let buf = &mut buf[0..output_size];

        // Unknown compression (Permissive only): zero-fill output
        if let BmpCompression::Unknown(_) = self.comp {
            buf.fill(0);
            return Ok(());
        }

        if self.comp == BmpCompression::Rle4 || self.comp == BmpCompression::Rle8 {
            let scanline_data = self.decode_rle(stop)?;
            if self.pix_fmt == BmpPixelFormat::Pal8 {
                self.expand_palette(&scanline_data, buf, false)?;
                self.flip_vertically = true;
            }
        } else {
            match self.depth {
                8 | 16 | 24 | 32 => {
                    if self.pix_fmt == BmpPixelFormat::Pal8 {
                        self.expand_palette_from_remaining_bytes(buf, true)?;
                        self.flip_vertically ^= true;
                    } else if self.depth == 32 || self.depth == 16 {
                        let pad_size = self.width * self.pix_fmt.num_components();

                        if (self.rgb_bitfields == [0; 4] || self.comp != BmpCompression::Bitfields)
                            && self.depth == 16
                        {
                            self.rgb_bitfields = [31 << 10, 31 << 5, 31, 31];
                        }

                        if (self.rgb_bitfields == [0; 4] || self.comp != BmpCompression::Bitfields)
                            && self.depth == 32
                        {
                            for (row_idx, out) in buf.rchunks_exact_mut(pad_size).enumerate() {
                                if row_idx % 16 == 0 {
                                    stop.check()?;
                                }
                                for a in out.chunks_exact_mut(4) {
                                    let mut pixels = self.bytes.read_fixed_bytes_or_zero::<4>();
                                    if !PRESERVE_BGRA {
                                        pixels.swap(0, 2);
                                    }
                                    a.copy_from_slice(&pixels);
                                }
                            }
                            self.image_in_bgra = true;
                        } else {
                            let [mr, mg, mb, ma] = self.rgb_bitfields;
                            let rshift =
                                (32u32.wrapping_sub(mr.leading_zeros())).wrapping_sub(8) as i32;
                            let gshift =
                                (32u32.wrapping_sub(mg.leading_zeros())).wrapping_sub(8) as i32;
                            let bshift =
                                (32u32.wrapping_sub(mb.leading_zeros())).wrapping_sub(8) as i32;
                            let ashift =
                                (32u32.wrapping_sub(ma.leading_zeros())).wrapping_sub(8) as i32;

                            let rcount = mr.count_ones();
                            let gcount = mg.count_ones();
                            let bcount = mb.count_ones();
                            let acount = ma.count_ones();

                            let conv_function = |v: u32, a: &mut [u8]| {
                                if PRESERVE_BGRA {
                                    a[0] = shift_signed(v & mb, bshift, bcount) as u8;
                                    a[1] = shift_signed(v & mg, gshift, gcount) as u8;
                                    a[2] = shift_signed(v & mr, rshift, rcount) as u8;
                                } else {
                                    a[0] = shift_signed(v & mr, rshift, rcount) as u8;
                                    a[1] = shift_signed(v & mg, gshift, gcount) as u8;
                                    a[2] = shift_signed(v & mb, bshift, bcount) as u8;
                                }
                                if a.len() > 3 {
                                    if ma == 0 {
                                        a[3] = 255;
                                    } else {
                                        a[3] = shift_signed(v & ma, ashift, acount) as u8;
                                    }
                                }
                            };

                            if self.depth == 32 {
                                for (row_idx, out) in buf.rchunks_exact_mut(pad_size).enumerate() {
                                    if row_idx % 16 == 0 {
                                        stop.check()?;
                                    }
                                    for raw_pix in out.chunks_exact_mut(4) {
                                        let v = self.bytes.get_u32_le();
                                        conv_function(v, raw_pix);
                                    }
                                }
                                self.image_in_bgra = true;
                            } else if self.depth == 16 {
                                let num_components = self.pix_fmt.num_components();
                                let in_row_bytes = (self.width * 2).div_ceil(4) * 4;

                                for (row_idx, out) in buf.rchunks_exact_mut(pad_size).enumerate() {
                                    if row_idx % 16 == 0 {
                                        stop.check()?;
                                    }
                                    let mut bytes_read = 0usize;
                                    for raw_pix in out.chunks_exact_mut(num_components) {
                                        let v = u32::from(u16::from_le_bytes(
                                            self.bytes.read_fixed_bytes_or_zero::<2>(),
                                        ));
                                        bytes_read += 2;
                                        conv_function(v, raw_pix);
                                    }
                                    let padding = in_row_bytes.saturating_sub(bytes_read);
                                    let _ = self.bytes.skip(padding);
                                }
                                self.image_in_bgra = true;
                            }
                        }
                        self.flip_vertically ^= true;
                    } else {
                        // 24-bit path
                        let out_width = self.width * self.pix_fmt.num_components();
                        let in_width = ((self.width * usize::from(self.depth) + 31) / 8) & !3;

                        for (row_idx, out) in buf.rchunks_exact_mut(out_width).enumerate() {
                            if row_idx % 16 == 0 {
                                stop.check()?;
                            }
                            self.bytes.read_exact_bytes(out)?;
                            let _ = self.bytes.skip(in_width.saturating_sub(out_width));
                            if !PRESERVE_BGRA {
                                for pix_pair in out.chunks_exact_mut(3) {
                                    pix_pair.swap(0, 2);
                                }
                            }
                            self.image_in_bgra = true;
                        }
                        self.flip_vertically ^= true;
                    }
                }
                1 | 2 | 4 => {
                    if self.pix_fmt != BmpPixelFormat::Pal8 {
                        return Err(PnmError::UnsupportedVariant(
                            "bit depths < 8 must have a palette".into(),
                        ));
                    }
                    let width_bytes = ((self.width + 7) >> 3) << 3;
                    let in_width_bytes = (self.width * usize::from(self.depth)).div_ceil(8);
                    let mut in_width_buf = vec![0u8; in_width_bytes];
                    let scanline_size = width_bytes * 3;
                    let mut scanline_bytes = vec![0u8; scanline_size];

                    let row_out_size = (3 + usize::from(self.is_alpha)) * self.width;
                    for (row_idx, out_bytes) in buf.rchunks_exact_mut(row_out_size).enumerate() {
                        if row_idx % 16 == 0 {
                            stop.check()?;
                        }
                        self.bytes.read_exact_bytes(&mut in_width_buf)?;
                        expand_bits_to_byte(
                            self.depth as usize,
                            true,
                            &in_width_buf,
                            &mut scanline_bytes,
                        );
                        self.expand_palette(&scanline_bytes, out_bytes, true)?;
                    }
                    self.flip_vertically ^= true;
                }
                d => {
                    return Err(PnmError::UnsupportedVariant(alloc::format!(
                        "unhandled BMP bit depth: {d}"
                    )));
                }
            }
        }

        // Flip if needed
        if self.flip_vertically {
            let length = self.width * self.pix_fmt.num_components();
            let mut scanline = vec![0u8; length];
            let mid = buf.len() / 2;
            let (in_img_top, in_img_bottom) = buf.split_at_mut(mid);

            for (in_dim, out_dim) in in_img_top
                .chunks_exact_mut(length)
                .zip(in_img_bottom.rchunks_exact_mut(length))
            {
                scanline.copy_from_slice(in_dim);
                in_dim.copy_from_slice(out_dim);
                out_dim.copy_from_slice(&scanline);
            }
        }

        // Convert to BGR(A) if requested and not already done
        if PRESERVE_BGRA && !self.image_in_bgra {
            match self.pix_fmt.num_components() {
                3 => {
                    for pix in buf.chunks_exact_mut(3) {
                        pix.swap(0, 2);
                    }
                }
                4 => {
                    for pix in buf.chunks_exact_mut(4) {
                        pix.swap(0, 2);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn expand_palette(&self, in_bytes: &[u8], buf: &mut [u8], unpad: bool) -> Result<(), PnmError> {
        let palette = &self.palette;
        let pad = usize::from(unpad) * (((-(self.width as i32)) as u32) & 3) as usize;
        let validate = self.permissiveness != BmpPermissiveness::Permissive;

        if self.is_alpha {
            for (out_stride, in_stride) in buf
                .rchunks_exact_mut(self.width * 4)
                .take(self.height)
                .zip(in_bytes.chunks_exact(self.width + pad))
            {
                for (pal_byte, chunks) in in_stride.iter().zip(out_stride.chunks_exact_mut(4)) {
                    let idx = usize::from(*pal_byte);
                    if validate && idx >= self.palette_numbers {
                        return Err(PnmError::InvalidData(alloc::format!(
                            "palette index {idx} out of range (palette has {} entries)",
                            self.palette_numbers
                        )));
                    }
                    let entry = palette[idx];
                    chunks[0] = entry.red;
                    chunks[1] = entry.green;
                    chunks[2] = entry.blue;
                    chunks[3] = entry.alpha;
                }
            }
        } else {
            for (out_stride, in_stride) in buf
                .rchunks_exact_mut(self.width * 3)
                .take(self.height)
                .zip(in_bytes.chunks_exact(self.width + pad))
            {
                for (pal_byte, chunks) in in_stride.iter().zip(out_stride.chunks_exact_mut(3)) {
                    let idx = usize::from(*pal_byte);
                    if validate && idx >= self.palette_numbers {
                        return Err(PnmError::InvalidData(alloc::format!(
                            "palette index {idx} out of range (palette has {} entries)",
                            self.palette_numbers
                        )));
                    }
                    let entry = palette[idx];
                    chunks[0] = entry.red;
                    chunks[1] = entry.green;
                    chunks[2] = entry.blue;
                }
            }
        }
        Ok(())
    }

    fn expand_palette_from_remaining_bytes(
        &mut self,
        buf: &mut [u8],
        unpad: bool,
    ) -> Result<(), PnmError> {
        let pad = usize::from(unpad) * (((-(self.width as i32)) as u32) & 3) as usize;
        let validate = self.permissiveness != BmpPermissiveness::Permissive;

        if self.is_alpha {
            for out_stride in buf.rchunks_exact_mut(self.width * 4).take(self.height) {
                for chunks in out_stride.chunks_exact_mut(4) {
                    let byte = self.bytes.read_u8();
                    let idx = usize::from(byte);
                    if validate && idx >= self.palette_numbers {
                        return Err(PnmError::InvalidData(alloc::format!(
                            "palette index {idx} out of range (palette has {} entries)",
                            self.palette_numbers
                        )));
                    }
                    let entry = self.palette[idx];
                    chunks[0] = entry.red;
                    chunks[1] = entry.green;
                    chunks[2] = entry.blue;
                    chunks[3] = entry.alpha;
                }
                self.bytes.skip(pad)?;
            }
        } else {
            for out_stride in buf.rchunks_exact_mut(self.width * 3).take(self.height) {
                for chunks in out_stride.chunks_exact_mut(3) {
                    let byte = self.bytes.read_u8();
                    let idx = usize::from(byte);
                    if validate && idx >= self.palette_numbers {
                        return Err(PnmError::InvalidData(alloc::format!(
                            "palette index {idx} out of range (palette has {} entries)",
                            self.palette_numbers
                        )));
                    }
                    let entry = self.palette[idx];
                    chunks[0] = entry.red;
                    chunks[1] = entry.green;
                    chunks[2] = entry.blue;
                }
                self.bytes.skip(pad)?;
            }
        }
        Ok(())
    }

    fn decode_rle(&mut self, stop: &dyn Stop) -> Result<Vec<u8>, PnmError> {
        let depth = if self.depth < 8 { 8 } else { self.depth };

        let pixel_bits = self
            .width
            .checked_mul(self.height)
            .and_then(|v| v.checked_mul(usize::from(depth)))
            .ok_or(PnmError::DimensionsTooLarge {
                width: self.width as u32,
                height: self.height as u32,
            })?;

        let alloc_size = pixel_bits
            .checked_add(7)
            .ok_or(PnmError::DimensionsTooLarge {
                width: self.width as u32,
                height: self.height as u32,
            })?
            >> 3;

        let mut pixels = vec![0u8; alloc_size];
        let mut line = (self.height - 1) as i32;
        let mut pos = 0usize;

        if !(self.depth == 4 || self.depth == 8 || self.depth == 16 || self.depth == 32) {
            return Err(PnmError::UnsupportedVariant(alloc::format!(
                "unknown depth + RLE combination: depth {}",
                self.depth
            )));
        }

        stop.check()?;

        if self.depth == 4 {
            self.decode_rle4(&mut pixels, &mut line, &mut pos)?;
        } else {
            self.decode_rle8plus(&mut pixels, &mut line, &mut pos, stop)?;
        }

        Ok(pixels)
    }

    fn decode_rle4(
        &mut self,
        pixels: &mut [u8],
        line: &mut i32,
        pos: &mut usize,
    ) -> Result<(), PnmError> {
        let mut rle_code: u16;
        let mut stream_byte: u8;

        while *line >= 0 && *pos <= self.width {
            rle_code = u16::from(self.bytes.read_u8());

            if rle_code == 0 {
                stream_byte = self.bytes.read_u8();

                if stream_byte == 0 {
                    *line -= 1;
                    if *line < 0 {
                        if self.permissiveness == BmpPermissiveness::Permissive {
                            return Ok(());
                        }
                        return Err(PnmError::InvalidData("RLE4 line underflow".into()));
                    }
                    *pos = 0;
                    continue;
                } else if stream_byte == 1 {
                    return Ok(());
                } else if stream_byte == 2 {
                    stream_byte = self.bytes.read_u8();
                    *pos += usize::from(stream_byte);
                    stream_byte = self.bytes.read_u8();
                    *line -= i32::from(stream_byte);
                    if *line < 0 {
                        if self.permissiveness == BmpPermissiveness::Permissive {
                            return Ok(());
                        }
                        return Err(PnmError::InvalidData("RLE4 line underflow".into()));
                    }
                } else {
                    let odd_pixel = usize::from(stream_byte & 1);
                    rle_code = u16::from(stream_byte).div_ceil(2);
                    let extra_byte = usize::from(rle_code & 0x01);

                    for i in 0..rle_code {
                        if *pos >= self.width {
                            break;
                        }
                        let row_start = *line as usize * self.width;
                        stream_byte = self.bytes.read_u8();
                        if row_start + *pos < pixels.len() {
                            pixels[row_start + *pos] = stream_byte >> 4;
                        }
                        *pos += 1;

                        if i + 1 == rle_code && odd_pixel > 0 {
                            break;
                        }
                        if *pos >= self.width {
                            break;
                        }
                        if row_start + *pos < pixels.len() {
                            pixels[row_start + *pos] = stream_byte & 0x0F;
                        }
                        *pos += 1;
                    }
                    let _ = self.bytes.skip(usize::from(extra_byte > 0));
                }
            } else {
                if *pos + usize::from(rle_code) > self.width + 1 {
                    if self.permissiveness == BmpPermissiveness::Permissive {
                        // Consume stream byte, skip this run
                        let _ = self.bytes.read_u8();
                        continue;
                    }
                    return Err(PnmError::InvalidData(
                        "RLE4 frame pointer out of bounds".into(),
                    ));
                }
                stream_byte = self.bytes.read_u8();
                let row_start = *line as usize * self.width;

                for i in 0..rle_code {
                    if *pos >= self.width {
                        break;
                    }
                    let idx = row_start + *pos;
                    if idx < pixels.len() {
                        if (i & 1) == 0 {
                            pixels[idx] = stream_byte >> 4;
                        } else {
                            pixels[idx] = stream_byte & 0x0F;
                        }
                    }
                    *pos += 1;
                }
            }
        }
        Ok(())
    }

    fn decode_rle8plus(
        &mut self,
        pixels: &mut [u8],
        line: &mut i32,
        pos: &mut usize,
        stop: &dyn Stop,
    ) -> Result<(), PnmError> {
        let mut check_counter = 0u32;

        while !self.bytes.eof() {
            check_counter += 1;
            if check_counter % 1024 == 0 {
                stop.check()?;
            }

            let p1 = self.bytes.read_u8();
            if p1 == 0 {
                let p2 = self.bytes.read_u8();
                if p2 == 0 {
                    // End of line
                    *line -= 1;
                    if *line < 0 {
                        if self.bytes.get_u16_be() == 1
                            || self.permissiveness == BmpPermissiveness::Permissive
                        {
                            return Ok(());
                        }
                        return Err(PnmError::InvalidData(
                            "RLE line beyond picture bounds".into(),
                        ));
                    }
                    *pos = 0;
                    continue;
                } else if p2 == 1 {
                    return Ok(());
                } else if p2 == 2 {
                    let dx = self.bytes.read_u8();
                    let dy = self.bytes.read_u8();
                    *pos += usize::from(dx);
                    *line -= i32::from(dy);
                    if *line < 0 {
                        if self.permissiveness == BmpPermissiveness::Permissive {
                            return Ok(());
                        }
                        return Err(PnmError::InvalidData("RLE delta line underflow".into()));
                    }
                    continue;
                }

                // Absolute mode
                let row_start = *line as usize * self.width;
                let output_slice_start = row_start + *pos;

                if output_slice_start + usize::from(p2) * usize::from(self.depth >> 3)
                    > pixels.len()
                {
                    // Skip invalid data
                    let _ = self.bytes.skip(2 * usize::from(self.depth >> 3));
                    continue;
                }

                match self.depth {
                    8 | 24 => {
                        let size = usize::from(p2) * usize::from(self.depth >> 3);
                        if output_slice_start + size <= pixels.len() {
                            self.bytes.read_exact_bytes(
                                &mut pixels[output_slice_start..output_slice_start + size],
                            )?;
                        }
                        *pos += size;
                        if self.depth == 8 && (p2 & 1) == 1 {
                            let _ = self.bytes.skip(1);
                        }
                    }
                    16 => {
                        for chunk in pixels[output_slice_start..]
                            .chunks_exact_mut(2)
                            .take(usize::from(p2))
                        {
                            chunk[0] = self.bytes.read_u8();
                            chunk[1] = self.bytes.read_u8();
                        }
                        *pos += 2 * usize::from(p2);
                    }
                    32 => {
                        for chunk in pixels[output_slice_start..]
                            .chunks_exact_mut(4)
                            .take(usize::from(p2))
                        {
                            chunk[0] = self.bytes.read_u8();
                            chunk[1] = self.bytes.read_u8();
                            chunk[2] = self.bytes.read_u8();
                            chunk[3] = self.bytes.read_u8();
                        }
                        *pos += 4 * usize::from(p2);
                    }
                    _ => {}
                }
            } else {
                // Run of pixels
                let row_start = *line as usize * self.width;
                let byte_depth = usize::from(self.depth >> 3);

                if *pos + (usize::from(p1) * byte_depth) > pixels.len().saturating_sub(row_start) {
                    if self.permissiveness == BmpPermissiveness::Permissive {
                        // Clamp: skip this run, consume the pixel data from stream
                        match self.depth {
                            8 => {
                                let _ = self.bytes.read_u8();
                            }
                            16 => {
                                let _ = self.bytes.read_u8();
                                let _ = self.bytes.read_u8();
                            }
                            24 => {
                                for _ in 0..3 {
                                    let _ = self.bytes.read_u8();
                                }
                            }
                            32 => {
                                for _ in 0..4 {
                                    let _ = self.bytes.read_u8();
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }
                    return Err(PnmError::InvalidData("RLE position overrun".into()));
                }

                let output_start = row_start + *pos;
                let mut pix = [0u8; 4];

                match self.depth {
                    8 => {
                        pix[0] = self.bytes.read_u8();
                        let end = (output_start + usize::from(p1)).min(pixels.len());
                        pixels[output_start..end].fill(pix[0]);
                        *pos += usize::from(p1);
                    }
                    16 => {
                        pix[0] = self.bytes.read_u8();
                        pix[1] = self.bytes.read_u8();
                        for chunk in pixels[output_start..]
                            .chunks_exact_mut(2)
                            .take(usize::from(p1))
                        {
                            chunk[0..2].copy_from_slice(&pix[..2]);
                        }
                        *pos += 2 * usize::from(p1);
                    }
                    24 => {
                        pix[0] = self.bytes.read_u8();
                        pix[1] = self.bytes.read_u8();
                        pix[2] = self.bytes.read_u8();
                        for chunk in pixels[output_start..]
                            .chunks_exact_mut(3)
                            .take(usize::from(p1))
                        {
                            chunk[0..3].copy_from_slice(&pix[..3]);
                        }
                        *pos += 3 * usize::from(p1);
                    }
                    32 => {
                        pix[0] = self.bytes.read_u8();
                        pix[1] = self.bytes.read_u8();
                        pix[2] = self.bytes.read_u8();
                        pix[3] = self.bytes.read_u8();
                        for chunk in pixels[output_start..]
                            .chunks_exact_mut(4)
                            .take(usize::from(p1))
                        {
                            chunk[0..4].copy_from_slice(&pix[..4]);
                        }
                        *pos += 4 * usize::from(p1);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
