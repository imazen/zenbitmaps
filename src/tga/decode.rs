//! TGA (Targa) decoder.
//!
//! From-scratch implementation of the TGA 2.0 specification.
//! Supports uncompressed and RLE-compressed truecolor, grayscale,
//! and color-mapped images at 8, 15, 16, 24, and 32 bits per pixel.

use alloc::vec;
use alloc::vec::Vec;
use enough::Stop;

use crate::error::BitmapError;
use crate::pixel::PixelLayout;

/// Parsed TGA header (18 bytes, little-endian).
#[derive(Clone, Debug)]
#[allow(dead_code)] // x_origin, y_origin are part of the TGA spec
pub(crate) struct TgaHeader {
    pub id_length: u8,
    pub color_map_type: u8,
    pub image_type: u8,
    pub color_map_start: u16,
    pub color_map_length: u16,
    pub color_map_depth: u8,
    pub x_origin: u16,
    pub y_origin: u16,
    pub width: u16,
    pub height: u16,
    pub pixel_depth: u8,
    pub descriptor: u8,
}

impl TgaHeader {
    /// Number of alpha/attribute bits (descriptor bits 0-3).
    pub(crate) fn alpha_bits(&self) -> u8 {
        self.descriptor & 0x0F
    }

    /// Whether the image origin is top-to-bottom (bit 5 of descriptor).
    fn is_top_to_bottom(&self) -> bool {
        self.descriptor & 0x20 != 0
    }

    /// Whether the image origin is right-to-left (bit 4 of descriptor).
    fn is_right_to_left(&self) -> bool {
        self.descriptor & 0x10 != 0
    }

    /// Whether this is an RLE-compressed image type.
    fn is_rle(&self) -> bool {
        matches!(self.image_type, 9..=11)
    }

    /// Whether this is a color-mapped image type.
    pub(crate) fn is_color_mapped(&self) -> bool {
        matches!(self.image_type, 1 | 9)
    }

    /// Whether this is a grayscale image type.
    pub(crate) fn is_grayscale(&self) -> bool {
        matches!(self.image_type, 3 | 11)
    }
}

/// Parse and validate the 18-byte TGA header.
pub(crate) fn parse_header(data: &[u8]) -> Result<TgaHeader, BitmapError> {
    if data.len() < 18 {
        return Err(BitmapError::UnexpectedEof);
    }

    let header = TgaHeader {
        id_length: data[0],
        color_map_type: data[1],
        image_type: data[2],
        color_map_start: u16::from_le_bytes([data[3], data[4]]),
        color_map_length: u16::from_le_bytes([data[5], data[6]]),
        color_map_depth: data[7],
        x_origin: u16::from_le_bytes([data[8], data[9]]),
        y_origin: u16::from_le_bytes([data[10], data[11]]),
        width: u16::from_le_bytes([data[12], data[13]]),
        height: u16::from_le_bytes([data[14], data[15]]),
        pixel_depth: data[16],
        descriptor: data[17],
    };

    // Validate image type
    if !matches!(header.image_type, 1 | 2 | 3 | 9 | 10 | 11) {
        return Err(BitmapError::UnsupportedVariant(alloc::format!(
            "TGA image type {} is not supported",
            header.image_type
        )));
    }

    // Validate dimensions
    if header.width == 0 {
        return Err(BitmapError::InvalidHeader("TGA width is zero".into()));
    }
    if header.height == 0 {
        return Err(BitmapError::InvalidHeader("TGA height is zero".into()));
    }

    // Validate color map type
    if header.color_map_type > 1 {
        return Err(BitmapError::InvalidHeader(alloc::format!(
            "TGA color_map_type {} is invalid (must be 0 or 1)",
            header.color_map_type
        )));
    }

    // Color-mapped images must have a color map
    if header.is_color_mapped() && header.color_map_type != 1 {
        return Err(BitmapError::InvalidHeader(
            "TGA color-mapped image must have color_map_type=1".into(),
        ));
    }

    // Validate pixel depth
    match header.image_type {
        1 | 9 => {
            // Color-mapped: index depth must be 8 (we only support 8-bit indices)
            if header.pixel_depth != 8 {
                return Err(BitmapError::UnsupportedVariant(alloc::format!(
                    "TGA color-mapped pixel_depth {} not supported (only 8-bit indices)",
                    header.pixel_depth
                )));
            }
            // Palette entry depth
            if !matches!(header.color_map_depth, 15 | 16 | 24 | 32) {
                return Err(BitmapError::UnsupportedVariant(alloc::format!(
                    "TGA color_map_depth {} not supported (must be 15, 16, 24, or 32)",
                    header.color_map_depth
                )));
            }
        }
        2 | 10 => {
            // Truecolor
            if !matches!(header.pixel_depth, 15 | 16 | 24 | 32) {
                return Err(BitmapError::UnsupportedVariant(alloc::format!(
                    "TGA truecolor pixel_depth {} not supported (must be 15, 16, 24, or 32)",
                    header.pixel_depth
                )));
            }
        }
        3 | 11 => {
            // Grayscale
            if header.pixel_depth != 8 {
                return Err(BitmapError::UnsupportedVariant(alloc::format!(
                    "TGA grayscale pixel_depth {} not supported (only 8-bit)",
                    header.pixel_depth
                )));
            }
        }
        _ => unreachable!(), // already validated above
    }

    Ok(header)
}

/// Decode TGA pixel data to RGB8, RGBA8, or Gray8.
pub(crate) fn decode_pixels(
    data: &[u8],
    header: &TgaHeader,
    stop: &dyn Stop,
) -> Result<(Vec<u8>, PixelLayout), BitmapError> {
    let w = header.width as usize;
    let h = header.height as usize;

    // Skip past header and image ID
    let pixel_data_offset = 18 + header.id_length as usize;

    // Parse color map if present
    let (color_map, color_map_end) = if header.color_map_type == 1 {
        let entry_bytes = match header.color_map_depth {
            15 | 16 => 2,
            24 => 3,
            32 => 4,
            _ => {
                return Err(BitmapError::UnsupportedVariant(alloc::format!(
                    "TGA color_map_depth {} not supported",
                    header.color_map_depth
                )));
            }
        };
        let map_size = (header.color_map_length as usize)
            .checked_mul(entry_bytes)
            .ok_or_else(|| BitmapError::InvalidHeader("color map size overflow".into()))?;
        let map_start = pixel_data_offset;
        let map_end = map_start
            .checked_add(map_size)
            .ok_or(BitmapError::UnexpectedEof)?;
        if data.len() < map_end {
            return Err(BitmapError::UnexpectedEof);
        }
        (Some(&data[map_start..map_end]), map_end)
    } else {
        (None, pixel_data_offset)
    };

    let pixel_data = data
        .get(color_map_end..)
        .ok_or(BitmapError::UnexpectedEof)?;

    // Determine output layout
    let (layout, out_channels) = if header.is_grayscale() {
        (PixelLayout::Gray8, 1)
    } else if header.pixel_depth == 32
        || (header.is_color_mapped() && header.color_map_depth == 32)
        || header.alpha_bits() > 0
    {
        (PixelLayout::Rgba8, 4)
    } else {
        (PixelLayout::Rgb8, 3)
    };

    let pixel_count = w.checked_mul(h).ok_or(BitmapError::DimensionsTooLarge {
        width: header.width as u32,
        height: header.height as u32,
    })?;
    let out_size =
        pixel_count
            .checked_mul(out_channels)
            .ok_or(BitmapError::DimensionsTooLarge {
                width: header.width as u32,
                height: header.height as u32,
            })?;

    let mut out = vec![0u8; out_size];

    // Bytes per pixel in the source data
    let src_bpp: usize = match header.pixel_depth {
        8 => 1,
        15 | 16 => 2,
        24 => 3,
        32 => 4,
        _ => unreachable!(), // validated in parse_header
    };

    if header.is_rle() {
        decode_rle(
            pixel_data,
            &mut out,
            header,
            src_bpp,
            out_channels,
            color_map,
            stop,
        )?;
    } else {
        decode_raw(
            pixel_data,
            &mut out,
            header,
            src_bpp,
            out_channels,
            color_map,
            stop,
        )?;
    }

    // Handle right-to-left origin
    if header.is_right_to_left() {
        flip_horizontal(&mut out, w, h, out_channels);
    }

    // Handle bottom-to-top origin (TGA default is bottom-left)
    if !header.is_top_to_bottom() {
        flip_rows(&mut out, w, h, out_channels);
    }

    Ok((out, layout))
}

/// Decode uncompressed pixel data.
fn decode_raw(
    pixel_data: &[u8],
    out: &mut [u8],
    header: &TgaHeader,
    src_bpp: usize,
    out_channels: usize,
    color_map: Option<&[u8]>,
    stop: &dyn Stop,
) -> Result<(), BitmapError> {
    let w = header.width as usize;
    let h = header.height as usize;
    let row_bytes = w
        .checked_mul(src_bpp)
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width as u32,
            height: header.height as u32,
        })?;

    let total_src_bytes = row_bytes
        .checked_mul(h)
        .ok_or(BitmapError::DimensionsTooLarge {
            width: header.width as u32,
            height: header.height as u32,
        })?;

    if pixel_data.len() < total_src_bytes {
        return Err(BitmapError::UnexpectedEof);
    }

    // Fast path: 24-bit or 32-bit non-color-mapped — memcpy + batch swizzle
    if !header.is_color_mapped()
        && !header.is_grayscale()
        && src_bpp == out_channels
        && matches!(header.pixel_depth, 24 | 32)
    {
        out[..total_src_bytes].copy_from_slice(&pixel_data[..total_src_bytes]);
        // In-place BGR→RGB / BGRA→RGBA swizzle
        if out_channels == 3 {
            #[cfg(feature = "simd")]
            {
                let _ = garb::bytes::rgb_to_bgr_inplace(out);
            }
            #[cfg(not(feature = "simd"))]
            for pixel in out.chunks_exact_mut(3) {
                pixel.swap(0, 2);
            }
        } else {
            #[cfg(feature = "simd")]
            {
                let _ = garb::bytes::rgba_to_bgra_inplace(out);
            }
            #[cfg(not(feature = "simd"))]
            for pixel in out.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        return Ok(());
    }

    // General path: per-pixel conversion (color-mapped, 16-bit, grayscale)
    for y in 0..h {
        if y % 16 == 0 {
            stop.check()?;
        }
        let src_row = &pixel_data[y * row_bytes..(y + 1) * row_bytes];
        let dst_row_start = y * w * out_channels;

        for x in 0..w {
            let src = &src_row[x * src_bpp..(x + 1) * src_bpp];
            let dst_off = dst_row_start + x * out_channels;
            convert_pixel(
                src,
                &mut out[dst_off..dst_off + out_channels],
                header,
                color_map,
            )?;
        }
    }

    Ok(())
}

/// Decode RLE-compressed pixel data.
fn decode_rle(
    pixel_data: &[u8],
    out: &mut [u8],
    header: &TgaHeader,
    src_bpp: usize,
    out_channels: usize,
    color_map: Option<&[u8]>,
    stop: &dyn Stop,
) -> Result<(), BitmapError> {
    let w = header.width as usize;
    let h = header.height as usize;
    let total_pixels = w * h;
    let mut src_pos = 0;
    let mut pixel_idx = 0;

    while pixel_idx < total_pixels {
        if pixel_idx % (w * 16) == 0 {
            stop.check()?;
        }

        if src_pos >= pixel_data.len() {
            return Err(BitmapError::UnexpectedEof);
        }

        let packet_header = pixel_data[src_pos];
        src_pos += 1;

        let run_count = (packet_header & 0x7F) as usize + 1;
        let is_rle_packet = packet_header & 0x80 != 0;

        if pixel_idx + run_count > total_pixels {
            return Err(BitmapError::InvalidData(
                "TGA RLE packet exceeds image bounds".into(),
            ));
        }

        if is_rle_packet {
            // Run-length packet: one pixel value repeated
            if src_pos + src_bpp > pixel_data.len() {
                return Err(BitmapError::UnexpectedEof);
            }
            let src = &pixel_data[src_pos..src_pos + src_bpp];
            src_pos += src_bpp;

            // Convert the single pixel once
            let mut converted = [0u8; 4];
            convert_pixel(src, &mut converted[..out_channels], header, color_map)?;

            for _ in 0..run_count {
                let dst_off = pixel_idx * out_channels;
                out[dst_off..dst_off + out_channels].copy_from_slice(&converted[..out_channels]);
                pixel_idx += 1;
            }
        } else {
            // Raw packet: run_count literal pixels
            let needed = run_count
                .checked_mul(src_bpp)
                .ok_or(BitmapError::UnexpectedEof)?;
            if src_pos + needed > pixel_data.len() {
                return Err(BitmapError::UnexpectedEof);
            }

            for _ in 0..run_count {
                let src = &pixel_data[src_pos..src_pos + src_bpp];
                src_pos += src_bpp;
                let dst_off = pixel_idx * out_channels;
                convert_pixel(
                    src,
                    &mut out[dst_off..dst_off + out_channels],
                    header,
                    color_map,
                )?;
                pixel_idx += 1;
            }
        }
    }

    Ok(())
}

/// Convert a single source pixel to the output format.
///
/// Handles BGR→RGB swizzle, 16-bit 5-5-5 expansion, grayscale passthrough,
/// and color map lookup.
fn convert_pixel(
    src: &[u8],
    dst: &mut [u8],
    header: &TgaHeader,
    color_map: Option<&[u8]>,
) -> Result<(), BitmapError> {
    if header.is_color_mapped() {
        // Color-mapped: src is a single-byte index
        let index = src[0] as usize;
        let map = color_map.ok_or_else(|| {
            BitmapError::InvalidData("color-mapped image has no color map".into())
        })?;

        let adjusted_index = index
            .checked_sub(header.color_map_start as usize)
            .ok_or_else(|| {
                BitmapError::InvalidData(alloc::format!(
                    "palette index {index} is below color_map_start {}",
                    header.color_map_start
                ))
            })?;

        let entry_bytes: usize = match header.color_map_depth {
            15 | 16 => 2,
            24 => 3,
            32 => 4,
            _ => unreachable!(),
        };

        let entry_offset = adjusted_index
            .checked_mul(entry_bytes)
            .ok_or(BitmapError::UnexpectedEof)?;
        if entry_offset + entry_bytes > map.len() {
            return Err(BitmapError::InvalidData(alloc::format!(
                "palette index {index} out of range"
            )));
        }

        let entry = &map[entry_offset..entry_offset + entry_bytes];
        convert_color(entry, dst, header.color_map_depth);
    } else if header.is_grayscale() {
        dst[0] = src[0];
    } else {
        convert_color(src, dst, header.pixel_depth);
    }

    Ok(())
}

/// Convert a BGR/BGRA/16-bit color value to RGB/RGBA.
fn convert_color(src: &[u8], dst: &mut [u8], depth: u8) {
    match depth {
        15 | 16 => {
            // 5-5-5 packed: bit layout ARRRRRGG GGGBBBBB (little-endian stored as low, high)
            let val = u16::from_le_bytes([src[0], src[1]]);
            let r5 = ((val >> 10) & 0x1F) as u8;
            let g5 = ((val >> 5) & 0x1F) as u8;
            let b5 = (val & 0x1F) as u8;
            // Scale 5-bit to 8-bit: val * 255 / 31
            dst[0] = ((r5 as u16 * 255 + 15) / 31) as u8;
            dst[1] = ((g5 as u16 * 255 + 15) / 31) as u8;
            dst[2] = ((b5 as u16 * 255 + 15) / 31) as u8;
            if dst.len() >= 4 {
                // Alpha from bit 15 (only meaningful for 16-bit with alpha)
                dst[3] = if val & 0x8000 != 0 { 255 } else { 0 };
            }
        }
        24 => {
            // BGR → RGB
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
        }
        32 => {
            // BGRA → RGBA
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            if dst.len() >= 4 {
                dst[3] = src[3];
            }
        }
        _ => unreachable!(),
    }
}

/// Flip all rows vertically (bottom-to-top ↔ top-to-bottom).
fn flip_rows(buf: &mut [u8], w: usize, h: usize, channels: usize) {
    let row_bytes = w * channels;
    let mut top = 0;
    let mut bot = (h - 1) * row_bytes;
    while top < bot {
        // Swap rows by byte range
        for i in 0..row_bytes {
            buf.swap(top + i, bot + i);
        }
        top += row_bytes;
        bot -= row_bytes;
    }
}

/// Flip each row horizontally (right-to-left → left-to-right).
fn flip_horizontal(buf: &mut [u8], w: usize, h: usize, channels: usize) {
    let row_bytes = w * channels;
    for y in 0..h {
        let row_start = y * row_bytes;
        let mut left = 0;
        let mut right = (w - 1) * channels;
        while left < right {
            for c in 0..channels {
                buf.swap(row_start + left + c, row_start + right + c);
            }
            left += channels;
            right -= channels;
        }
    }
}
