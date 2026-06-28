//! Full BMP decoder supporting all standard bit depths, RLE, and bitfields.
//!
//! Forked from zune-bmp 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib).
//! Adapted: ZReader → &[u8] cursor, DecoderOptions → Option<&Limits>,
//! BmpDecoderErrors → BitmapError, log removed, stop.check() added.

use alloc::vec;
use alloc::vec::Vec;

use enough::Stop;

use super::utils::{expand_bits_to_byte, shift_signed};
use crate::alloc_util::{self, AllocPref};
use crate::error::BitmapError;
use crate::pixel::PixelLayout;
use whereat::at;

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

    fn set_position(&mut self, pos: usize) -> crate::Result<()> {
        if pos > self.data.len() {
            if self.permissive {
                self.pos = self.data.len();
                return Ok(());
            }
            return Err(at!(BitmapError::UnexpectedEof));
        }
        self.pos = pos;
        Ok(())
    }

    fn skip(&mut self, n: usize) -> crate::Result<()> {
        let new_pos = self
            .pos
            .checked_add(n)
            .ok_or_else(|| at!(BitmapError::UnexpectedEof))?;
        if new_pos > self.data.len() {
            if self.permissive {
                self.pos = self.data.len();
                return Ok(());
            }
            return Err(at!(BitmapError::UnexpectedEof));
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

    fn read_u8_err(&mut self) -> crate::Result<u8> {
        if self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            Ok(b)
        } else {
            Err(at!(BitmapError::UnexpectedEof))
        }
    }

    fn get_u16_le_err(&mut self) -> crate::Result<u16> {
        if self.pos + 2 > self.data.len() {
            return Err(at!(BitmapError::UnexpectedEof));
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

    fn get_u32_le_err(&mut self) -> crate::Result<u32> {
        if self.pos + 4 > self.data.len() {
            return Err(at!(BitmapError::UnexpectedEof));
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

    fn read_fixed_bytes<const N: usize>(&mut self) -> crate::Result<[u8; N]> {
        if self.pos + N > self.data.len() {
            if self.permissive {
                let mut buf = [0u8; N];
                let available = self.data.len().saturating_sub(self.pos);
                buf[..available].copy_from_slice(&self.data[self.pos..self.pos + available]);
                self.pos = self.data.len();
                return Ok(buf);
            }
            return Err(at!(BitmapError::UnexpectedEof));
        }
        let mut buf = [0u8; N];
        buf.copy_from_slice(&self.data[self.pos..self.pos + N]);
        self.pos += N;
        Ok(buf)
    }

    fn read_fixed_bytes_or_zero<const N: usize>(&mut self) -> [u8; N] {
        self.read_fixed_bytes().unwrap_or([0u8; N])
    }

    fn read_exact_bytes(&mut self, buf: &mut [u8]) -> crate::Result<()> {
        let n = buf.len();
        if self.pos + n > self.data.len() {
            if self.permissive {
                let available = self.data.len().saturating_sub(self.pos);
                buf[..available].copy_from_slice(&self.data[self.pos..self.pos + available]);
                buf[available..].fill(0);
                self.pos = self.data.len();
                return Ok(());
            }
            return Err(at!(BitmapError::UnexpectedEof));
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
    /// Bits per pixel as declared in the BMP header.
    pub bpp: u16,
    pub x_pels_per_meter: u32,
    pub y_pels_per_meter: u32,
    /// Color table entries (BGRA order, up to 256 entries).
    /// Only present for indexed-color BMPs (1/2/4/8-bit).
    pub color_table: Option<alloc::vec::Vec<[u8; 4]>>,
}

// ── Public header parsing (for probe) ───────────────────────────────

/// Parse a BMP header to extract dimensions and pixel format.
/// This is the header-only fast path for probing.
/// Parse the BMP header, returning the decoded geometry/format metadata.
///
/// `max_pixels` is the effective pixel-count ceiling enforced on the declared
/// dimensions (pass `u64::MAX` for metadata-only probing that must not reject
/// on size; the decode path passes the caller's resolved [`crate::Limits`]).
pub(crate) fn parse_bmp_header(data: &[u8], max_pixels: u64) -> crate::Result<BmpHeader> {
    // Header probing uses Permissive to avoid rejecting files before
    // the caller has chosen a permissiveness level. No output buffer is
    // allocated here, so the alloc preference is irrelevant — pass the default.
    let mut dec = BmpDecoderState::new(
        data,
        BmpPermissiveness::Permissive,
        max_pixels,
        AllocPref::CodecDefault,
    );
    dec.decode_headers()?;

    let layout = match dec.pix_fmt {
        BmpPixelFormat::Rgba => PixelLayout::Rgba8,
        BmpPixelFormat::Rgb | BmpPixelFormat::Pal8 => PixelLayout::Rgb8,
        BmpPixelFormat::Gray8 => PixelLayout::Gray8,
        BmpPixelFormat::None => {
            return Err(at!(BitmapError::UnsupportedVariant(
                "unsupported BMP pixel format".into(),
            )));
        }
    };

    // Extract color table for indexed formats
    let color_table = if dec.pix_fmt == BmpPixelFormat::Pal8 && dec.palette_numbers > 0 {
        let mut table = alloc::vec::Vec::with_capacity(dec.palette_numbers);
        for i in 0..dec.palette_numbers {
            let e = &dec.palette[i];
            table.push([e.blue, e.green, e.red, e.alpha]);
        }
        Some(table)
    } else {
        None
    };

    Ok(BmpHeader {
        width: dec.width as u32,
        height: dec.height as u32,
        layout,
        bpp: dec.depth,
        x_pels_per_meter: dec.x_pels_per_meter,
        y_pels_per_meter: dec.y_pels_per_meter,
        color_table,
    })
}

// ── Full decode ─────────────────────────────────────────────────────

/// Decode BMP pixel data (RGB/RGBA output).
///
/// `max_pixels` is the effective pixel-count ceiling (already resolved against
/// the caller's [`crate::Limits`], defaulting to
/// [`crate::limits::DEFAULT_MAX_PIXELS`]); pass `u64::MAX` to opt out.
pub(crate) fn decode_bmp_pixels(
    data: &[u8],
    permissiveness: BmpPermissiveness,
    max_pixels: u64,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<(Vec<u8>, PixelLayout)> {
    let mut dec = BmpDecoderState::new(data, permissiveness, max_pixels, alloc_pref);
    dec.decode_headers()?;

    // Output buffer sized from the (untrusted) header dimensions → default
    // fallible.
    let output_size = dec.output_buf_size()?;
    let mut buf = alloc_util::alloc_zeroed(alloc_pref, true, output_size)?;

    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
    dec.decode_into::<false>(&mut buf, stop)?;

    let layout = match dec.pix_fmt {
        BmpPixelFormat::Rgba => PixelLayout::Rgba8,
        BmpPixelFormat::Rgb | BmpPixelFormat::Pal8 => PixelLayout::Rgb8,
        BmpPixelFormat::Gray8 => PixelLayout::Gray8,
        BmpPixelFormat::None => {
            return Err(at!(BitmapError::UnsupportedVariant(
                "unsupported BMP pixel format".into(),
            )));
        }
    };

    Ok((buf, layout))
}

/// Decode BMP pixel data in native byte order (BGR/BGRA).
///
/// `max_pixels` is the effective pixel-count ceiling (already resolved against
/// the caller's [`crate::Limits`], defaulting to
/// [`crate::limits::DEFAULT_MAX_PIXELS`]); pass `u64::MAX` to opt out.
pub(crate) fn decode_bmp_pixels_native(
    data: &[u8],
    permissiveness: BmpPermissiveness,
    max_pixels: u64,
    alloc_pref: AllocPref,
    stop: &dyn Stop,
) -> crate::Result<(Vec<u8>, PixelLayout)> {
    let mut dec = BmpDecoderState::new(data, permissiveness, max_pixels, alloc_pref);
    dec.decode_headers()?;

    // Output buffer sized from the (untrusted) header dimensions → default
    // fallible.
    let output_size = dec.output_buf_size()?;
    let mut buf = alloc_util::alloc_zeroed(alloc_pref, true, output_size)?;

    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
    dec.decode_into::<true>(&mut buf, stop)?;

    let layout = match dec.pix_fmt {
        BmpPixelFormat::Rgba => PixelLayout::Bgra8,
        BmpPixelFormat::Rgb | BmpPixelFormat::Pal8 => PixelLayout::Bgr8,
        BmpPixelFormat::Gray8 => PixelLayout::Gray8,
        BmpPixelFormat::None => {
            return Err(at!(BitmapError::UnsupportedVariant(
                "unsupported BMP pixel format".into(),
            )));
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
    /// Horizontal pixels per meter from the DIB header (0 if not present).
    x_pels_per_meter: u32,
    /// Vertical pixels per meter from the DIB header (0 if not present).
    y_pels_per_meter: u32,
    /// Effective pixel-count ceiling, already resolved against the caller's
    /// [`crate::Limits`] (or [`crate::limits::DEFAULT_MAX_PIXELS`] when none
    /// was supplied). Checked in `decode_headers` immediately after the
    /// dimensions are parsed, *before* the byte-availability heuristic — so an
    /// over-cap header is rejected with a `LimitExceeded("pixel count …")`
    /// resource error rather than masked by a downstream truncation error.
    /// `u64::MAX` opts out (header-probe path uses this).
    max_pixels: u64,
    /// Allocation-fallibility preference for the RLE-decompressed output buffer
    /// (the only buffer this state allocates from untrusted-derived sizes).
    alloc_pref: AllocPref,
}

impl<'a> BmpDecoderState<'a> {
    /// Hard cap on output buffer size (1 GiB). Prevents OOM from
    /// pathological headers like 3M×2M. Callers can set lower limits
    /// via the `Limits` API. This limit is independent of system memory.
    const MAX_OUTPUT_BYTES: usize = 1024 * 1024 * 1024;

    fn new(
        data: &'a [u8],
        permissiveness: BmpPermissiveness,
        max_pixels: u64,
        alloc_pref: AllocPref,
    ) -> Self {
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
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            max_pixels,
            alloc_pref,
        }
    }

    #[allow(unused_assignments)]
    fn decode_headers(&mut self) -> crate::Result<()> {
        if self.decoded_headers {
            return Ok(());
        }

        let is_strict = self.permissiveness == BmpPermissiveness::Strict;
        let is_permissive = self.permissiveness == BmpPermissiveness::Permissive;
        let data_len = self.bytes.data.len();

        if self.bytes.read_u8_err()? != b'B' || self.bytes.read_u8_err()? != b'M' {
            return Err(at!(BitmapError::UnrecognizedFormat));
        }

        // File size field (offset 2)
        let file_size_field = self.bytes.get_u32_le_err()?;
        // Reserved (4 bytes)
        self.bytes.skip(4)?;

        // Strict: validate file size field matches actual data length
        if is_strict && file_size_field != 0 && file_size_field as usize != data_len {
            return Err(at!(BitmapError::InvalidHeader(alloc::format!(
                "BMP file size field ({file_size_field}) doesn't match actual size ({data_len})"
            ))));
        }

        let hsize = self.bytes.get_u32_le_err()?;
        let ihsize = self.bytes.get_u32_le_err()?;

        if ihsize.saturating_add(14) > hsize {
            return Err(at!(BitmapError::InvalidHeader(
                "invalid BMP header size".into()
            )));
        }

        let (width, height, planes, bpp, compression);
        let mut color_used: u32 = 0;
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
                            return Err(at!(BitmapError::UnsupportedVariant(
                                "unsupported BMP compression scheme".into(),
                            )));
                        }
                    }
                } else {
                    BmpCompression::Rgb
                };

                if ihsize > 16 {
                    let image_size_field = self.bytes.get_u32_le_err()?;
                    let x_pixels = self.bytes.get_u32_le_err()?;
                    let y_pixels = self.bytes.get_u32_le_err()?;
                    self.x_pels_per_meter = x_pixels;
                    self.y_pels_per_meter = y_pixels;
                    color_used = self.bytes.get_u32_le_err()?;
                    let _important_colors = self.bytes.get_u32_le_err()?;

                    // Strict: validate DPI and image data size fields
                    if is_strict {
                        // Resolution: must be non-negative and reasonable.
                        // Max ~1M pixels/meter ≈ 25,400 DPI, more than any real device.
                        const MAX_RESOLUTION: u32 = 1_000_000;
                        let x_signed = x_pixels as i32;
                        let y_signed = y_pixels as i32;
                        if x_signed < 0 || x_pixels > MAX_RESOLUTION {
                            return Err(at!(BitmapError::InvalidHeader(alloc::format!(
                                "BMP horizontal resolution out of range ({x_signed})"
                            ))));
                        }
                        if y_signed < 0 || y_pixels > MAX_RESOLUTION {
                            return Err(at!(BitmapError::InvalidHeader(alloc::format!(
                                "BMP vertical resolution out of range ({y_signed})"
                            ))));
                        }
                        // Image data size should be 0 or match expected (for uncompressed)
                        if image_size_field != 0 && compression == BmpCompression::Rgb && width > 0
                        {
                            let row_bytes = (width as usize * bpp as usize).div_ceil(32) * 4;
                            let expected_size = row_bytes * (height as i32).unsigned_abs() as usize;
                            if image_size_field as usize != expected_size {
                                return Err(at!(BitmapError::InvalidHeader(alloc::format!(
                                    "BMP image data size field ({image_size_field}) doesn't match expected ({expected_size})"
                                ))));
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
                return Err(at!(BitmapError::InvalidHeader(alloc::format!(
                    "unknown BMP info header size: {ihsize}"
                ))));
            }
        }

        // Planes validation (Standard and Strict reject planes != 1)
        if !is_permissive && planes != 1 {
            return Err(at!(BitmapError::InvalidHeader(alloc::format!(
                "BMP planes field is {planes}, expected 1"
            ))));
        }

        self.flip_vertically = (height as i32) > 0;
        self.height = (height as i32).unsigned_abs() as usize;
        self.width = width as usize;

        if self.width == 0 {
            return Err(at!(BitmapError::InvalidHeader("BMP width is zero".into())));
        }
        if self.height == 0 {
            return Err(at!(BitmapError::InvalidHeader("BMP height is zero".into())));
        }

        // Enforce the effective pixel-count ceiling on the *declared*
        // dimensions before the byte-availability heuristic below. A header
        // over the cap (e.g. 225 MP under the 120 MP default) must report the
        // resource limit as the reason it was rejected, not a downstream
        // "not enough pixel data" error. Mirrors `limits::check_dimensions`.
        let declared_pixels = (self.width as u64).saturating_mul(self.height as u64);
        if declared_pixels > self.max_pixels {
            return Err(at!(BitmapError::LimitExceeded(alloc::format!(
                "pixel count {declared_pixels} exceeds limit {}",
                self.max_pixels
            ))));
        }

        // RLE + top-down is forbidden by spec (Standard and Strict reject)
        if !is_permissive
            && !self.flip_vertically
            && matches!(compression, BmpCompression::Rle4 | BmpCompression::Rle8)
        {
            return Err(at!(BitmapError::InvalidData(
                "RLE compression with top-down row order is forbidden by BMP spec".into(),
            )));
        }

        if bpp == 0 {
            return Err(at!(BitmapError::InvalidHeader(
                "BMP bit depth is zero".into()
            )));
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
                if hsize.wrapping_sub(ihsize).wrapping_sub(14) > 0 || color_used > 0 {
                    self.pix_fmt = BmpPixelFormat::Pal8;
                } else {
                    self.pix_fmt = BmpPixelFormat::Gray8;
                }
            }
            1 | 2 | 4 => {
                if hsize.wrapping_sub(ihsize).wrapping_sub(14) > 0 || color_used > 0 {
                    self.pix_fmt = BmpPixelFormat::Pal8;
                } else {
                    return Err(at!(BitmapError::UnsupportedVariant(alloc::format!(
                        "unknown palette for {}-color BMP",
                        1u32 << bpp
                    ))));
                }
            }
            _ => {
                return Err(at!(BitmapError::UnsupportedVariant(alloc::format!(
                    "BMP bit depth {bpp} unsupported"
                ))));
            }
        }

        if self.pix_fmt == BmpPixelFormat::None {
            return Err(at!(BitmapError::UnsupportedVariant(
                "unsupported BMP pixel format".into(),
            )));
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
                        return Err(at!(BitmapError::InvalidHeader(alloc::format!(
                            "BMP palette count ({t}) exceeds max for {bpp}-bit depth ({})",
                            1u32 << bpp
                        ))));
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
                    return Err(at!(BitmapError::InvalidData(
                        "invalid BMP palette entries".into(),
                    )));
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
        // Pixel data starts at the data offset (hsize), or after the palette
        // if the data offset is too small (malformed BMP with wrong bfOffBits).
        let pixel_data_start = (hsize as usize).max(self.bytes.pos);
        self.bytes.set_position(pixel_data_start)?;

        // For non-RLE (uncompressed) formats, validate that the available
        // pixel data is sufficient for the claimed dimensions. Without this,
        // a tiny file claiming millions of pixels causes a huge allocation
        // and an extremely slow iteration over zero-padded rows.
        if !matches!(compression, BmpCompression::Rle4 | BmpCompression::Rle8) {
            let available_bytes = self.bytes.data.len().saturating_sub(self.bytes.pos);
            // Width is read as u32 from the header; on 32-bit usize platforms
            // `width * bpp` can overflow even though `width * height * channels`
            // is later capped via `output_buf_size`. Use checked_mul to
            // restore the panic-free contract on 32-bit.
            let bytes_per_row = self
                .width
                .checked_mul(usize::from(bpp))
                .map(|v| v.div_ceil(8))
                .ok_or_else(|| {
                    at!(BitmapError::DimensionsTooLarge {
                        width: self.width as u32,
                        height: self.height as u32,
                    })
                })?;

            if bytes_per_row > 0 && available_bytes < bytes_per_row {
                // Not enough data for even a single scanline
                if !is_permissive {
                    return Err(at!(BitmapError::InvalidData(alloc::format!(
                        "BMP pixel data too short: {available_bytes} bytes available, \
                         need at least {bytes_per_row} for one row of {}×{} @ {bpp}bpp",
                        self.width,
                        self.height
                    ))));
                }
            }

            // Cap: output size must not exceed 1024× the available input data.
            // Uncompressed BMP expands at most ~4× (1bpp → 3 bytes RGB) per
            // input byte; 1024× is extremely generous and only catches
            // pathological headers on tiny inputs.
            let output_size = self
                .width
                .saturating_mul(self.height)
                .saturating_mul(self.pix_fmt.num_components());
            let max_reasonable = available_bytes.saturating_mul(1024);
            if output_size > max_reasonable && available_bytes < 1024 * 1024 {
                return Err(at!(BitmapError::InvalidData(alloc::format!(
                    "BMP claims {}×{} @ {bpp}bpp ({output_size} output bytes) \
                     but only {available_bytes} bytes of pixel data available",
                    self.width,
                    self.height
                ))));
            }
        }

        self.decoded_headers = true;

        Ok(())
    }

    fn output_buf_size(&self) -> crate::Result<usize> {
        self.width
            .checked_mul(self.height)
            .and_then(|wh| wh.checked_mul(self.pix_fmt.num_components()))
            .filter(|&size| size <= Self::MAX_OUTPUT_BYTES)
            .ok_or_else(|| {
                at!(BitmapError::DimensionsTooLarge {
                    width: self.width as u32,
                    height: self.height as u32,
                })
            })
    }

    /// `self.width * factor`, returning `DimensionsTooLarge` on usize overflow.
    /// Header-parsed width is u32; on 32-bit usize platforms even modest
    /// per-row factors (e.g. `bpp=32`) can overflow without this check.
    fn width_times(&self, factor: usize) -> crate::Result<usize> {
        self.width.checked_mul(factor).ok_or_else(|| {
            at!(BitmapError::DimensionsTooLarge {
                width: self.width as u32,
                height: self.height as u32,
            })
        })
    }

    fn decode_into<const PRESERVE_BGRA: bool>(
        &mut self,
        buf: &mut [u8],
        stop: &dyn Stop,
    ) -> crate::Result<()> {
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
                        let pad_size = self.width_times(self.pix_fmt.num_components())?;

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
                                    stop.check().map_err(|r| at!(BitmapError::from(r)))?;
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
                                        stop.check().map_err(|r| at!(BitmapError::from(r)))?;
                                    }
                                    for raw_pix in out.chunks_exact_mut(4) {
                                        let v = self.bytes.get_u32_le();
                                        conv_function(v, raw_pix);
                                    }
                                }
                                self.image_in_bgra = true;
                            } else if self.depth == 16 {
                                let num_components = self.pix_fmt.num_components();
                                let in_row_bytes = self
                                    .width_times(2)?
                                    .checked_add(3)
                                    .map(|v| v & !3usize)
                                    .ok_or_else(|| {
                                        at!(BitmapError::DimensionsTooLarge {
                                            width: self.width as u32,
                                            height: self.height as u32,
                                        })
                                    })?;

                                for (row_idx, out) in buf.rchunks_exact_mut(pad_size).enumerate() {
                                    if row_idx % 16 == 0 {
                                        stop.check().map_err(|r| at!(BitmapError::from(r)))?;
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
                        // 8-bit grayscale (num_components == 1) and 24-bit RGB
                        // (num_components == 3) share this scanline reader. The
                        // BGR->RGB channel swap below applies only to 24-bit RGB:
                        // Gray8 is a single channel and must not be swizzled, or
                        // its scanlines get scrambled in 3-byte groups.
                        let num_components = self.pix_fmt.num_components();
                        let out_width = self.width_times(num_components)?;
                        let in_width = self
                            .width_times(usize::from(self.depth))?
                            .checked_add(31)
                            .map(|v| (v / 8) & !3usize)
                            .ok_or_else(|| {
                                at!(BitmapError::DimensionsTooLarge {
                                    width: self.width as u32,
                                    height: self.height as u32,
                                })
                            })?;

                        let swap_channels = !PRESERVE_BGRA && num_components == 3;
                        for (row_idx, out) in buf.rchunks_exact_mut(out_width).enumerate() {
                            if row_idx % 16 == 0 {
                                stop.check().map_err(|r| at!(BitmapError::from(r)))?;
                            }
                            self.bytes.read_exact_bytes(out)?;
                            let _ = self.bytes.skip(in_width.saturating_sub(out_width));
                            if swap_channels {
                                for pix_pair in out.chunks_exact_mut(3) {
                                    pix_pair.swap(0, 2);
                                }
                            }
                        }
                        // Only RGB data is now in BGR order awaiting the final
                        // swizzle; Gray8 has no channel order to track.
                        if num_components == 3 {
                            self.image_in_bgra = true;
                        }
                        self.flip_vertically ^= true;
                    }
                }
                1 | 2 | 4 => {
                    if self.pix_fmt != BmpPixelFormat::Pal8 {
                        return Err(at!(BitmapError::UnsupportedVariant(
                            "bit depths < 8 must have a palette".into(),
                        )));
                    }
                    let width_bytes = self
                        .width
                        .checked_add(7)
                        .map(|v| (v >> 3) << 3)
                        .ok_or_else(|| {
                            at!(BitmapError::DimensionsTooLarge {
                                width: self.width as u32,
                                height: self.height as u32,
                            })
                        })?;
                    let in_width_bytes = self.width_times(usize::from(self.depth))?.div_ceil(8);
                    let mut in_width_buf = vec![0u8; in_width_bytes];
                    let scanline_size = width_bytes * 3;
                    let mut scanline_bytes = vec![0u8; scanline_size];

                    let row_out_size = (3 + usize::from(self.is_alpha)) * self.width;
                    for (row_idx, out_bytes) in buf.rchunks_exact_mut(row_out_size).enumerate() {
                        if row_idx % 16 == 0 {
                            stop.check().map_err(|r| at!(BitmapError::from(r)))?;
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
                    return Err(at!(BitmapError::UnsupportedVariant(alloc::format!(
                        "unhandled BMP bit depth: {d}"
                    ))));
                }
            }
        }

        // Flip if needed
        if self.flip_vertically {
            let length = self.width_times(self.pix_fmt.num_components())?;
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

    fn expand_palette(&self, in_bytes: &[u8], buf: &mut [u8], unpad: bool) -> crate::Result<()> {
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
                        return Err(at!(BitmapError::InvalidData(alloc::format!(
                            "palette index {idx} out of range (palette has {} entries)",
                            self.palette_numbers
                        ))));
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
                        return Err(at!(BitmapError::InvalidData(alloc::format!(
                            "palette index {idx} out of range (palette has {} entries)",
                            self.palette_numbers
                        ))));
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
    ) -> crate::Result<()> {
        let pad = usize::from(unpad) * (((-(self.width as i32)) as u32) & 3) as usize;
        let validate = self.permissiveness != BmpPermissiveness::Permissive;

        if self.is_alpha {
            for out_stride in buf.rchunks_exact_mut(self.width * 4).take(self.height) {
                for chunks in out_stride.chunks_exact_mut(4) {
                    let byte = self.bytes.read_u8();
                    let idx = usize::from(byte);
                    if validate && idx >= self.palette_numbers {
                        return Err(at!(BitmapError::InvalidData(alloc::format!(
                            "palette index {idx} out of range (palette has {} entries)",
                            self.palette_numbers
                        ))));
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
                        return Err(at!(BitmapError::InvalidData(alloc::format!(
                            "palette index {idx} out of range (palette has {} entries)",
                            self.palette_numbers
                        ))));
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

    fn decode_rle(&mut self, stop: &dyn Stop) -> crate::Result<Vec<u8>> {
        let depth = if self.depth < 8 { 8 } else { self.depth };

        let pixel_bits = self
            .width
            .checked_mul(self.height)
            .and_then(|v| v.checked_mul(usize::from(depth)))
            .ok_or_else(|| {
                at!(BitmapError::DimensionsTooLarge {
                    width: self.width as u32,
                    height: self.height as u32,
                })
            })?;

        let alloc_size = pixel_bits.checked_add(7).ok_or_else(|| {
            at!(BitmapError::DimensionsTooLarge {
                width: self.width as u32,
                height: self.height as u32,
            })
        })? >> 3;

        if alloc_size > Self::MAX_OUTPUT_BYTES {
            return Err(at!(BitmapError::DimensionsTooLarge {
                width: self.width as u32,
                height: self.height as u32,
            }));
        }

        // Decompression-bomb guard. RLE4/RLE8 expand by at most ~127 output
        // bytes per input byte (a 2-byte encoded run yields ≤255 indices). A
        // declared output far larger than the whole file could possibly encode
        // is a bomb or a truncated header — reject it instead of allocating and
        // post-processing a near-output-cap buffer (palette expansion, flip,
        // format conversion all run per output pixel) for a tiny input. A
        // 158-byte file declaring ~2.7e8 pixels otherwise cost ~18 s of work
        // (fuzz DoS). The 256× factor leaves ~2× headroom over the worst legit
        // ratio; the 64 KiB floor keeps small images decodable from tiny input.
        const MAX_RLE_RATIO: usize = 256;
        let ratio_cap = self
            .bytes
            .data
            .len()
            .saturating_mul(MAX_RLE_RATIO)
            .max(64 * 1024);
        if alloc_size > ratio_cap {
            return Err(at!(BitmapError::InvalidData(
                "RLE output far exceeds the compressed size (decompression bomb)".into(),
            )));
        }

        // RLE-decompressed output sized from the header-declared `alloc_size`
        // (already bomb-ratio-guarded above) → default fallible.
        let mut pixels = alloc_util::alloc_zeroed(self.alloc_pref, true, alloc_size)?;
        let mut line = (self.height - 1) as i32;
        let mut pos = 0usize;

        if !(self.depth == 4 || self.depth == 8 || self.depth == 16 || self.depth == 32) {
            return Err(at!(BitmapError::UnsupportedVariant(alloc::format!(
                "unknown depth + RLE combination: depth {}",
                self.depth
            ))));
        }

        stop.check().map_err(|r| at!(BitmapError::from(r)))?;

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
    ) -> crate::Result<()> {
        let mut rle_code: u16;
        let mut stream_byte: u8;

        // Stop when the RLE stream is exhausted, matching `decode_rle8plus`. A
        // well-formed RLE4 stream ends with the `00 01` end-of-bitmap escape
        // (returns Ok below) before EOF; without this guard a truncated stream
        // makes `read_u8()` return 0 forever (read as the `00 00` end-of-line
        // escape), spinning `*line` down from a huge declared height — a tiny
        // input with height ≈ 2.7e8 looped ~18 s (fuzz zenbitmaps DoS). The
        // bound is now O(input), not O(height).
        while *line >= 0 && *pos <= self.width && !self.bytes.eof() {
            rle_code = u16::from(self.bytes.read_u8());

            if rle_code == 0 {
                stream_byte = self.bytes.read_u8();

                if stream_byte == 0 {
                    *line -= 1;
                    if *line < 0 {
                        if self.permissiveness == BmpPermissiveness::Permissive {
                            return Ok(());
                        }
                        return Err(at!(BitmapError::InvalidData("RLE4 line underflow".into())));
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
                        return Err(at!(BitmapError::InvalidData("RLE4 line underflow".into())));
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
                    return Err(at!(BitmapError::InvalidData(
                        "RLE4 frame pointer out of bounds".into(),
                    )));
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
    ) -> crate::Result<()> {
        let mut check_counter = 0u32;

        while !self.bytes.eof() {
            check_counter += 1;
            if check_counter.is_multiple_of(1024) {
                stop.check().map_err(|r| at!(BitmapError::from(r)))?;
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
                        return Err(at!(BitmapError::InvalidData(
                            "RLE line beyond picture bounds".into(),
                        )));
                    }
                    *pos = 0;
                    continue;
                } else if p2 == 1 {
                    return Ok(());
                } else if p2 == 2 {
                    let dx = self.bytes.read_u8();
                    let dy = self.bytes.read_u8();
                    // Use checked_add to prevent usize wrap on 32-bit targets:
                    // a maliciously crafted RLE stream that issues many "delta"
                    // escapes can otherwise accumulate `*pos` past usize::MAX.
                    *pos = match pos.checked_add(usize::from(dx)) {
                        Some(v) => v,
                        None => {
                            if self.permissiveness == BmpPermissiveness::Permissive {
                                return Ok(());
                            }
                            return Err(at!(BitmapError::InvalidData(
                                "RLE delta column overflow".into(),
                            )));
                        }
                    };
                    *line -= i32::from(dy);
                    if *line < 0 {
                        if self.permissiveness == BmpPermissiveness::Permissive {
                            return Ok(());
                        }
                        return Err(at!(BitmapError::InvalidData(
                            "RLE delta line underflow".into()
                        )));
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
                    return Err(at!(BitmapError::InvalidData("RLE position overrun".into())));
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
