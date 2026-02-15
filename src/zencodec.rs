//! zencodec-types trait implementations for zenpnm.

use alloc::vec::Vec;
use zencodec_types::{
    CodecCapabilities, DecodeOutput, EncodeOutput, ImageFormat, ImageInfo, ImageMetadata,
    PixelData, ResourceLimits, Stop,
};

use crate::error::PnmError;
use crate::limits::Limits;
use crate::pnm;

// ── Capabilities ─────────────────────────────────────────────────────

static ENCODE_CAPS: CodecCapabilities = CodecCapabilities::new().with_native_gray(true);

static DECODE_CAPS: CodecCapabilities = CodecCapabilities::new()
    .with_native_gray(true)
    .with_cheap_probe(true);

// ── PnmEncoding ──────────────────────────────────────────────────────

/// Encoding configuration for PNM formats.
///
/// Implements [`zencodec_types::Encoding`] for the PNM family.
/// Default output: PPM for RGB, PGM for Gray, PAM for RGBA.
#[derive(Clone, Debug)]
pub struct PnmEncoding {
    limits: ResourceLimits,
}

impl Default for PnmEncoding {
    fn default() -> Self {
        Self::new()
    }
}

impl PnmEncoding {
    /// Create a new PNM encoder config with default settings.
    pub fn new() -> Self {
        Self {
            limits: ResourceLimits::none(),
        }
    }
}

impl zencodec_types::Encoding for PnmEncoding {
    type Error = PnmError;
    type Job<'a> = PnmEncodingJob<'a>;

    fn capabilities() -> &'static CodecCapabilities {
        &ENCODE_CAPS
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    fn job(&self) -> PnmEncodingJob<'_> {
        PnmEncodingJob {
            _config: self,
            limits: None,
        }
    }
}

/// Per-operation PNM encode job.
pub struct PnmEncodingJob<'a> {
    _config: &'a PnmEncoding,
    limits: Option<ResourceLimits>,
}

impl<'a> zencodec_types::EncodingJob<'a> for PnmEncodingJob<'a> {
    type Error = PnmError;

    fn with_stop(self, _stop: &'a dyn Stop) -> Self {
        self
    }

    fn with_metadata(self, _meta: &'a ImageMetadata<'a>) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(limits);
        self
    }

    fn encode_rgb8(
        self,
        img: imgref::ImgRef<'_, rgb::Rgb<u8>>,
    ) -> Result<EncodeOutput, PnmError> {
        let w = img.width() as u32;
        let h = img.height() as u32;
        let (buf, _, _) = img.to_contiguous_buf();
        let bytes = rgb::ComponentBytes::as_bytes(buf.as_ref());
        let encoded = pnm::encode(
            bytes,
            w,
            h,
            crate::PixelLayout::Rgb8,
            pnm::PnmFormat::Ppm,
            &enough::Unstoppable,
        )?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
    }

    fn encode_rgba8(
        self,
        img: imgref::ImgRef<'_, rgb::Rgba<u8>>,
    ) -> Result<EncodeOutput, PnmError> {
        let w = img.width() as u32;
        let h = img.height() as u32;
        let (buf, _, _) = img.to_contiguous_buf();
        let bytes = rgb::ComponentBytes::as_bytes(buf.as_ref());
        let encoded = pnm::encode(
            bytes,
            w,
            h,
            crate::PixelLayout::Rgba8,
            pnm::PnmFormat::Pam,
            &enough::Unstoppable,
        )?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
    }

    fn encode_gray8(
        self,
        img: imgref::ImgRef<'_, rgb::Gray<u8>>,
    ) -> Result<EncodeOutput, PnmError> {
        let w = img.width() as u32;
        let h = img.height() as u32;
        let (buf, _, _) = img.to_contiguous_buf();
        let bytes = rgb::ComponentBytes::as_bytes(buf.as_ref());
        let encoded = pnm::encode(
            bytes,
            w,
            h,
            crate::PixelLayout::Gray8,
            pnm::PnmFormat::Pgm,
            &enough::Unstoppable,
        )?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
    }

    fn encode_bgra8(
        self,
        img: imgref::ImgRef<'_, rgb::alt::BGRA<u8>>,
    ) -> Result<EncodeOutput, PnmError> {
        let w = img.width() as u32;
        let h = img.height() as u32;
        let (buf, _, _) = img.to_contiguous_buf();
        let bytes = rgb::ComponentBytes::as_bytes(buf.as_ref());
        let encoded = pnm::encode(
            bytes,
            w,
            h,
            crate::PixelLayout::Bgra8,
            pnm::PnmFormat::Ppm,
            &enough::Unstoppable,
        )?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
    }

    fn encode_bgrx8(
        self,
        img: imgref::ImgRef<'_, rgb::alt::BGRA<u8>>,
    ) -> Result<EncodeOutput, PnmError> {
        let w = img.width() as u32;
        let h = img.height() as u32;
        let (buf, _, _) = img.to_contiguous_buf();
        let bytes = rgb::ComponentBytes::as_bytes(buf.as_ref());
        let encoded = pnm::encode(
            bytes,
            w,
            h,
            crate::PixelLayout::Bgrx8,
            pnm::PnmFormat::Ppm,
            &enough::Unstoppable,
        )?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
    }

    fn encode_rgb_f32(
        self,
        img: imgref::ImgRef<'_, rgb::Rgb<f32>>,
    ) -> Result<EncodeOutput, PnmError> {
        let w = img.width() as u32;
        let h = img.height() as u32;
        let (buf, _, _) = img.to_contiguous_buf();
        let bytes = rgb::ComponentBytes::as_bytes(buf.as_ref());
        let encoded = pnm::encode(
            bytes,
            w,
            h,
            crate::PixelLayout::RgbF32,
            pnm::PnmFormat::Pfm,
            &enough::Unstoppable,
        )?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
    }

    fn encode_rgba_f32(
        self,
        img: imgref::ImgRef<'_, rgb::Rgba<f32>>,
    ) -> Result<EncodeOutput, PnmError> {
        // PFM has no alpha channel — drop alpha and write PFM color.
        let w = img.width() as u32;
        let h = img.height() as u32;
        let (buf, _, _) = img.to_contiguous_buf();
        let rgb_pixels: Vec<rgb::Rgb<f32>> = buf
            .iter()
            .map(|px| rgb::Rgb { r: px.r, g: px.g, b: px.b })
            .collect();
        let bytes = rgb::ComponentBytes::as_bytes(rgb_pixels.as_slice());
        let encoded = pnm::encode(
            bytes,
            w,
            h,
            crate::PixelLayout::RgbF32,
            pnm::PnmFormat::Pfm,
            &enough::Unstoppable,
        )?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
    }

    fn encode_gray_f32(
        self,
        img: imgref::ImgRef<'_, rgb::Gray<f32>>,
    ) -> Result<EncodeOutput, PnmError> {
        let w = img.width() as u32;
        let h = img.height() as u32;
        let (buf, _, _) = img.to_contiguous_buf();
        let bytes = rgb::ComponentBytes::as_bytes(buf.as_ref());
        let encoded = pnm::encode(
            bytes,
            w,
            h,
            crate::PixelLayout::GrayF32,
            pnm::PnmFormat::Pfm,
            &enough::Unstoppable,
        )?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
    }
}

// ── PnmDecoding ──────────────────────────────────────────────────────

/// Decoding configuration for PNM formats.
///
/// Implements [`zencodec_types::Decoding`] for the PNM family.
#[derive(Clone, Debug)]
pub struct PnmDecoding {
    limits: Option<Limits>,
}

impl Default for PnmDecoding {
    fn default() -> Self {
        Self::new()
    }
}

impl PnmDecoding {
    /// Create a new PNM decoder config with default settings.
    pub fn new() -> Self {
        Self { limits: None }
    }
}

impl zencodec_types::Decoding for PnmDecoding {
    type Error = PnmError;
    type Job<'a> = PnmDecodingJob<'a>;

    fn capabilities() -> &'static CodecCapabilities {
        &DECODE_CAPS
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(convert_limits(&limits));
        self
    }

    fn job(&self) -> PnmDecodingJob<'_> {
        PnmDecodingJob {
            config: self,
            limits: None,
        }
    }

    fn probe_header(&self, data: &[u8]) -> Result<ImageInfo, PnmError> {
        let header = pnm::decode::parse_header(data)?;
        Ok(header_to_image_info(&header))
    }
}

/// Per-operation PNM decode job.
pub struct PnmDecodingJob<'a> {
    config: &'a PnmDecoding,
    limits: Option<Limits>,
}

impl<'a> zencodec_types::DecodingJob<'a> for PnmDecodingJob<'a> {
    type Error = PnmError;

    fn with_stop(self, _stop: &'a dyn Stop) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(convert_limits(&limits));
        self
    }

    fn decode(self, data: &[u8]) -> Result<DecodeOutput, PnmError> {
        let limits = self.limits.as_ref().or(self.config.limits.as_ref());
        let decoded = pnm::decode(data, limits, &enough::Unstoppable)?;

        let has_alpha = matches!(
            decoded.layout,
            crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
        );
        let info = ImageInfo::new(decoded.width, decoded.height, ImageFormat::Pnm)
            .with_alpha(has_alpha);

        let pixels = layout_to_pixel_data(&decoded)?;
        Ok(DecodeOutput::new(pixels, info))
    }

    fn decode_into_rgb8(
        self,
        data: &[u8],
        mut dst: imgref::ImgRefMut<'_, rgb::Rgb<u8>>,
    ) -> Result<ImageInfo, PnmError> {
        let output = self.decode(data)?;
        let info = output.info().clone();
        let src = output.into_rgb8();
        for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
            let n = src_row.len().min(dst_row.len());
            dst_row[..n].copy_from_slice(&src_row[..n]);
        }
        Ok(info)
    }

    fn decode_into_rgba8(
        self,
        data: &[u8],
        mut dst: imgref::ImgRefMut<'_, rgb::Rgba<u8>>,
    ) -> Result<ImageInfo, PnmError> {
        let output = self.decode(data)?;
        let info = output.info().clone();
        let src = output.into_rgba8();
        for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
            let n = src_row.len().min(dst_row.len());
            dst_row[..n].copy_from_slice(&src_row[..n]);
        }
        Ok(info)
    }

    fn decode_into_gray8(
        self,
        data: &[u8],
        mut dst: imgref::ImgRefMut<'_, rgb::Gray<u8>>,
    ) -> Result<ImageInfo, PnmError> {
        let output = self.decode(data)?;
        let info = output.info().clone();
        let src = output.into_gray8();
        for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
            let n = src_row.len().min(dst_row.len());
            dst_row[..n].copy_from_slice(&src_row[..n]);
        }
        Ok(info)
    }

    fn decode_into_bgra8(
        self,
        data: &[u8],
        mut dst: imgref::ImgRefMut<'_, rgb::alt::BGRA<u8>>,
    ) -> Result<ImageInfo, PnmError> {
        let output = self.decode(data)?;
        let info = output.info().clone();
        let src = output.into_bgra8();
        for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
            let n = src_row.len().min(dst_row.len());
            dst_row[..n].copy_from_slice(&src_row[..n]);
        }
        Ok(info)
    }

    fn decode_into_bgrx8(
        self,
        data: &[u8],
        mut dst: imgref::ImgRefMut<'_, rgb::alt::BGRA<u8>>,
    ) -> Result<ImageInfo, PnmError> {
        let output = self.decode(data)?;
        let info = output.info().clone();
        let src = output.into_bgra8();
        for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
            let n = src_row.len().min(dst_row.len());
            for (s, d) in src_row[..n].iter().zip(dst_row[..n].iter_mut()) {
                *d = rgb::alt::BGRA { b: s.b, g: s.g, r: s.r, a: 255 };
            }
        }
        Ok(info)
    }

    fn decode_into_rgb_f32(
        self,
        data: &[u8],
        mut dst: imgref::ImgRefMut<'_, rgb::Rgb<f32>>,
    ) -> Result<ImageInfo, PnmError> {
        use linear_srgb::default::srgb_to_linear_fast;

        let output = self.decode(data)?;
        let info = output.info().clone();
        let is_float = matches!(output.pixels(), PixelData::RgbF32(_) | PixelData::RgbaF32(_) | PixelData::GrayF32(_));
        if is_float {
            // PFM is already linear — copy directly
            let src = output.into_rgb_f32();
            for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
                let n = src_row.len().min(dst_row.len());
                dst_row[..n].copy_from_slice(&src_row[..n]);
            }
        } else {
            // Integer PNM (u8 or u16): preserve full precision via into_rgb_f32,
            // then linearize. Avoids truncating 16-bit to 8-bit.
            let src = output.into_rgb_f32();
            for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
                for (s, d) in src_row.iter().zip(dst_row.iter_mut()) {
                    *d = rgb::Rgb {
                        r: srgb_to_linear_fast(s.r),
                        g: srgb_to_linear_fast(s.g),
                        b: srgb_to_linear_fast(s.b),
                    };
                }
            }
        }
        Ok(info)
    }

    fn decode_into_rgba_f32(
        self,
        data: &[u8],
        mut dst: imgref::ImgRefMut<'_, rgb::Rgba<f32>>,
    ) -> Result<ImageInfo, PnmError> {
        use linear_srgb::default::srgb_to_linear_fast;

        let output = self.decode(data)?;
        let info = output.info().clone();
        let is_float = matches!(output.pixels(), PixelData::RgbF32(_) | PixelData::RgbaF32(_) | PixelData::GrayF32(_));
        if is_float {
            let src = output.into_rgba_f32();
            for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
                let n = src_row.len().min(dst_row.len());
                dst_row[..n].copy_from_slice(&src_row[..n]);
            }
        } else {
            // Preserve full precision (u16 → f32 via /65535), then linearize
            let src = output.into_rgba_f32();
            for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
                for (s, d) in src_row.iter().zip(dst_row.iter_mut()) {
                    *d = rgb::Rgba {
                        r: srgb_to_linear_fast(s.r),
                        g: srgb_to_linear_fast(s.g),
                        b: srgb_to_linear_fast(s.b),
                        a: s.a,
                    };
                }
            }
        }
        Ok(info)
    }

    fn decode_into_gray_f32(
        self,
        data: &[u8],
        mut dst: imgref::ImgRefMut<'_, rgb::Gray<f32>>,
    ) -> Result<ImageInfo, PnmError> {
        use linear_srgb::default::srgb_to_linear_fast;

        let output = self.decode(data)?;
        let info = output.info().clone();
        let is_float = matches!(output.pixels(), PixelData::RgbF32(_) | PixelData::RgbaF32(_) | PixelData::GrayF32(_));
        if is_float {
            let src = output.into_gray_f32();
            for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
                let n = src_row.len().min(dst_row.len());
                dst_row[..n].copy_from_slice(&src_row[..n]);
            }
        } else {
            // Preserve full precision (u16 → f32 via /65535), then linearize
            let src = output.into_gray_f32();
            for (src_row, dst_row) in src.as_ref().rows().zip(dst.rows_mut()) {
                for (s, d) in src_row.iter().zip(dst_row.iter_mut()) {
                    *d = rgb::Gray::new(srgb_to_linear_fast(s.value()));
                }
            }
        }
        Ok(info)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn convert_limits(limits: &ResourceLimits) -> Limits {
    Limits {
        max_width: limits.max_width.map(u64::from),
        max_height: limits.max_height.map(u64::from),
        max_pixels: limits.max_pixels,
        max_memory_bytes: limits.max_memory_bytes,
    }
}

fn header_to_image_info(header: &pnm::PnmHeader) -> ImageInfo {
    use crate::PixelLayout;
    let has_alpha = matches!(header.layout, PixelLayout::Rgba8 | PixelLayout::Bgra8);
    ImageInfo::new(header.width, header.height, ImageFormat::Pnm).with_alpha(has_alpha)
}

fn layout_to_pixel_data(decoded: &crate::decode::DecodeOutput<'_>) -> Result<PixelData, PnmError> {
    use crate::PixelLayout;
    use rgb::AsPixels as _;

    let w = decoded.width as usize;
    let h = decoded.height as usize;
    let bytes = decoded.pixels();

    match decoded.layout {
        PixelLayout::Gray8 => {
            let pixels: &[rgb::Gray<u8>] = bytes.as_pixels();
            Ok(PixelData::Gray8(imgref::ImgVec::new(
                pixels.to_vec(),
                w,
                h,
            )))
        }
        PixelLayout::Gray16 => {
            let pixels: Vec<rgb::Gray<u16>> = bytes
                .chunks_exact(2)
                .map(|c| rgb::Gray::new(u16::from_ne_bytes([c[0], c[1]])))
                .collect();
            Ok(PixelData::Gray16(imgref::ImgVec::new(pixels, w, h)))
        }
        PixelLayout::Rgb8 => {
            let pixels: &[rgb::Rgb<u8>] = bytes.as_pixels();
            Ok(PixelData::Rgb8(imgref::ImgVec::new(
                pixels.to_vec(),
                w,
                h,
            )))
        }
        PixelLayout::Rgba8 => {
            let pixels: &[rgb::Rgba<u8>] = bytes.as_pixels();
            Ok(PixelData::Rgba8(imgref::ImgVec::new(
                pixels.to_vec(),
                w,
                h,
            )))
        }
        PixelLayout::GrayF32 => {
            let pixels: Vec<rgb::Gray<f32>> = bytes
                .chunks_exact(4)
                .map(|c| rgb::Gray::new(f32::from_ne_bytes([c[0], c[1], c[2], c[3]])))
                .collect();
            Ok(PixelData::GrayF32(imgref::ImgVec::new(pixels, w, h)))
        }
        PixelLayout::RgbF32 => {
            // Expand RGB f32 to RGBA f32 with alpha = 1.0
            let pixels: Vec<rgb::Rgba<f32>> = bytes
                .chunks_exact(12)
                .map(|c| {
                    let r = f32::from_ne_bytes([c[0], c[1], c[2], c[3]]);
                    let g = f32::from_ne_bytes([c[4], c[5], c[6], c[7]]);
                    let b = f32::from_ne_bytes([c[8], c[9], c[10], c[11]]);
                    rgb::Rgba {
                        r,
                        g,
                        b,
                        a: 1.0,
                    }
                })
                .collect();
            Ok(PixelData::RgbaF32(imgref::ImgVec::new(pixels, w, h)))
        }
        PixelLayout::Bgr8 => {
            // Swizzle BGR → RGB
            let pixels: Vec<rgb::Rgb<u8>> = bytes
                .chunks_exact(3)
                .map(|c| rgb::Rgb {
                    r: c[2],
                    g: c[1],
                    b: c[0],
                })
                .collect();
            Ok(PixelData::Rgb8(imgref::ImgVec::new(pixels, w, h)))
        }
        PixelLayout::Bgra8 => {
            let pixels: &[rgb::alt::BGRA<u8>] = bytes.as_pixels();
            Ok(PixelData::Bgra8(imgref::ImgVec::new(
                pixels.to_vec(),
                w,
                h,
            )))
        }
        PixelLayout::Bgrx8 => {
            // Treat BGRX as BGRA (padding byte becomes alpha)
            let pixels: &[rgb::alt::BGRA<u8>] = bytes.as_pixels();
            Ok(PixelData::Bgra8(imgref::ImgVec::new(
                pixels.to_vec(),
                w,
                h,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use zencodec_types::{Decoding, Encoding};

    #[test]
    fn encode_decode_rgb8_roundtrip() {
        let pixels = vec![
            rgb::Rgb { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb { r: 128, g: 128, b: 128 },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();
        assert_eq!(output.format(), ImageFormat::Pnm);

        let dec = PnmDecoding::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
        let rgb_img = decoded.into_rgb8();
        assert_eq!(rgb_img.buf().as_slice(), &pixels);
    }

    #[test]
    fn encode_decode_gray8_roundtrip() {
        let pixels = vec![
            rgb::Gray::new(0u8),
            rgb::Gray::new(128),
            rgb::Gray::new(255),
            rgb::Gray::new(64),
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_gray8(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let gray_img = decoded.into_gray8();
        assert_eq!(gray_img.buf().as_slice(), &pixels);
    }

    #[test]
    fn encode_decode_rgba8_roundtrip() {
        let pixels = vec![
            rgb::Rgba { r: 255, g: 0, b: 0, a: 255 },
            rgb::Rgba { r: 0, g: 255, b: 0, a: 128 },
            rgb::Rgba { r: 0, g: 0, b: 255, a: 0 },
            rgb::Rgba { r: 128, g: 128, b: 128, a: 255 },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgba8(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        assert!(decoded.has_alpha());
        let rgba_img = decoded.into_rgba8();
        assert_eq!(rgba_img.buf().as_slice(), &pixels);
    }

    #[test]
    fn encode_bgra8_no_double_swizzle() {
        // BGRA encode should go directly to PPM via zenpnm's native BGRA→RGB
        // path, not through the default trait BGRA→RGBA→PAM path.
        let pixels = vec![
            rgb::alt::BGRA { b: 0, g: 0, r: 255, a: 255 },
            rgb::alt::BGRA { b: 0, g: 255, r: 0, a: 255 },
            rgb::alt::BGRA { b: 255, g: 0, r: 0, a: 255 },
            rgb::alt::BGRA { b: 128, g: 128, r: 128, a: 255 },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_bgra8(img.as_ref()).unwrap();

        // Decode and verify the RGB values came through correctly
        let dec = PnmDecoding::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let rgb_img = decoded.into_rgb8();
        let buf = rgb_img.buf();
        assert_eq!(buf[0], rgb::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(buf[1], rgb::Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(buf[2], rgb::Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(buf[3], rgb::Rgb { r: 128, g: 128, b: 128 });
    }

    #[test]
    fn encode_bgrx8_no_double_swizzle() {
        // BGRX encode should go directly to PPM, ignoring the padding byte.
        let pixels = vec![
            rgb::alt::BGRA { b: 0, g: 0, r: 255, a: 0 },   // alpha ignored
            rgb::alt::BGRA { b: 0, g: 255, r: 0, a: 99 },   // alpha ignored
            rgb::alt::BGRA { b: 255, g: 0, r: 0, a: 200 },  // alpha ignored
            rgb::alt::BGRA { b: 128, g: 128, r: 128, a: 1 }, // alpha ignored
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_bgrx8(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let rgb_img = decoded.into_rgb8();
        let buf = rgb_img.buf();
        assert_eq!(buf[0], rgb::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(buf[1], rgb::Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(buf[2], rgb::Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(buf[3], rgb::Rgb { r: 128, g: 128, b: 128 });
    }

    #[test]
    fn probe_header_extracts_info() {
        let pixels = vec![rgb::Rgb { r: 1, g: 2, b: 3 }; 6];
        let img = imgref::ImgVec::new(pixels, 3, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let info = dec.probe_header(output.bytes()).unwrap();
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 2);
        assert_eq!(info.format, ImageFormat::Pnm);
        assert!(!info.has_alpha);
    }

    #[test]
    fn capabilities_are_correct() {
        let enc_caps = PnmEncoding::capabilities();
        assert!(enc_caps.native_gray());
        assert!(!enc_caps.cheap_probe()); // encode side doesn't probe
        assert!(!enc_caps.encode_icc());
        assert!(!enc_caps.encode_cancel());

        let dec_caps = PnmDecoding::capabilities();
        assert!(dec_caps.native_gray());
        assert!(dec_caps.cheap_probe());
        assert!(!dec_caps.decode_cancel());
    }

    #[test]
    fn with_limits_propagates() {
        let limits = ResourceLimits::none()
            .with_max_width(10)
            .with_max_height(10);

        let dec = PnmDecoding::new().with_limits(limits);
        let big_pixels = vec![rgb::Rgb { r: 0, g: 0, b: 0 }; 100 * 100];
        let img = imgref::ImgVec::new(big_pixels, 100, 100);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let result = dec.decode(output.bytes());
        assert!(result.is_err());
    }

    #[test]
    fn decode_into_bgra8_from_rgb() {
        let pixels = vec![
            rgb::Rgb { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb { r: 128, g: 128, b: 128 },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let mut buf = vec![rgb::alt::BGRA { b: 0, g: 0, r: 0, a: 0 }; 4];
        let mut dst = imgref::ImgVec::new(buf.clone(), 2, 2);
        let info = dec.decode_into_bgra8(output.bytes(), dst.as_mut()).unwrap();
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 2);
        buf = dst.into_buf();
        assert_eq!(buf[0], rgb::alt::BGRA { b: 0, g: 0, r: 255, a: 255 });
        assert_eq!(buf[1], rgb::alt::BGRA { b: 0, g: 255, r: 0, a: 255 });
        assert_eq!(buf[2], rgb::alt::BGRA { b: 255, g: 0, r: 0, a: 255 });
        assert_eq!(buf[3], rgb::alt::BGRA { b: 128, g: 128, r: 128, a: 255 });
    }

    #[test]
    fn decode_into_bgrx8_forces_alpha_255() {
        // Encode RGBA with non-255 alpha
        let pixels = vec![
            rgb::Rgba { r: 255, g: 0, b: 0, a: 100 },
            rgb::Rgba { r: 0, g: 255, b: 0, a: 50 },
            rgb::Rgba { r: 0, g: 0, b: 255, a: 0 },
            rgb::Rgba { r: 128, g: 128, b: 128, a: 200 },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgba8(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let buf = vec![rgb::alt::BGRA { b: 0, g: 0, r: 0, a: 0 }; 4];
        let mut dst = imgref::ImgVec::new(buf, 2, 2);
        dec.decode_into_bgrx8(output.bytes(), dst.as_mut()).unwrap();
        let result = dst.into_buf();
        // All alpha bytes must be 255 regardless of source
        for px in &result {
            assert_eq!(px.a, 255);
        }
    }

    #[test]
    fn encode_decode_rgb_f32_roundtrip() {
        let pixels = vec![
            rgb::Rgb { r: 0.0f32, g: 0.5, b: 1.0 },
            rgb::Rgb { r: 0.25, g: 0.75, b: 0.125 },
            rgb::Rgb { r: 1.0, g: 0.0, b: 0.0 },
            rgb::Rgb { r: 0.5, g: 0.5, b: 0.5 },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgb_f32(img.as_ref()).unwrap();
        assert_eq!(output.format(), ImageFormat::Pnm);

        // Decode and verify f32 values survive PFM roundtrip
        let dec = PnmDecoding::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let rgb_img = decoded.into_rgb_f32();
        let buf = rgb_img.buf();
        for (orig, decoded) in pixels.iter().zip(buf.iter()) {
            assert!((orig.r - decoded.r).abs() < 1e-6);
            assert!((orig.g - decoded.g).abs() < 1e-6);
            assert!((orig.b - decoded.b).abs() < 1e-6);
        }
    }

    #[test]
    fn encode_decode_gray_f32_roundtrip() {
        let pixels = vec![
            rgb::Gray::new(0.0f32),
            rgb::Gray::new(0.25),
            rgb::Gray::new(0.5),
            rgb::Gray::new(1.0),
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_gray_f32(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let gray_img = decoded.into_gray_f32();
        let buf = gray_img.buf();
        for (orig, decoded) in pixels.iter().zip(buf.iter()) {
            assert!((orig.value() - decoded.value()).abs() < 1e-6);
        }
    }

    #[test]
    fn encode_rgba_f32_drops_alpha() {
        // RGBA f32 encodes to PFM (no alpha), verify RGB values survive
        let pixels = vec![
            rgb::Rgba { r: 0.5f32, g: 0.25, b: 0.75, a: 0.1 },
            rgb::Rgba { r: 1.0, g: 0.0, b: 0.0, a: 0.5 },
            rgb::Rgba { r: 0.0, g: 1.0, b: 0.0, a: 0.9 },
            rgb::Rgba { r: 0.0, g: 0.0, b: 1.0, a: 0.0 },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgba_f32(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let rgb_img = decoded.into_rgb_f32();
        let buf = rgb_img.buf();
        for (orig, decoded) in pixels.iter().zip(buf.iter()) {
            assert!((orig.r - decoded.r).abs() < 1e-6);
            assert!((orig.g - decoded.g).abs() < 1e-6);
            assert!((orig.b - decoded.b).abs() < 1e-6);
        }
    }

    #[test]
    fn decode_into_rgb_f32_from_u8() {
        use linear_srgb::default::srgb_to_linear_fast;

        // Encode as PPM (u8), then decode_into_rgb_f32 — verifies sRGB→linear path
        let pixels = vec![
            rgb::Rgb { r: 0u8, g: 128, b: 255 },
            rgb::Rgb { r: 255, g: 0, b: 128 },
            rgb::Rgb { r: 64, g: 192, b: 32 },
            rgb::Rgb { r: 100, g: 100, b: 100 },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoding::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let dec = PnmDecoding::new();
        let buf = vec![rgb::Rgb { r: 0.0f32, g: 0.0, b: 0.0 }; 4];
        let mut dst = imgref::ImgVec::new(buf, 2, 2);
        dec.decode_into_rgb_f32(output.bytes(), dst.as_mut()).unwrap();
        let result = dst.into_buf();
        // sRGB 0 → linear 0.0
        assert!((result[0].r - 0.0).abs() < 1e-6);
        // sRGB 128/255 → linear (via srgb_to_linear)
        assert!((result[0].g - srgb_to_linear_fast(128.0 / 255.0)).abs() < 1e-5);
        // sRGB 255 → linear 1.0
        assert!((result[0].b - 1.0).abs() < 1e-6);
    }

    #[test]
    fn encoding_clone_send_sync() {
        fn assert_traits<T: Clone + Send + Sync>() {}
        assert_traits::<PnmEncoding>();
    }

    #[test]
    fn decoding_clone_send_sync() {
        fn assert_traits<T: Clone + Send + Sync>() {}
        assert_traits::<PnmDecoding>();
    }

    #[test]
    fn f32_conversion_all_simd_tiers() {
        use archmage::testing::{for_each_token_permutation, CompileTimePolicy};
        use linear_srgb::default::srgb_to_linear_fast;

        let report = for_each_token_permutation(CompileTimePolicy::Warn, |_perm| {
            // Encode as PPM (u8), decode to linear f32, verify values
            let pixels = vec![
                rgb::Rgb { r: 0u8, g: 128, b: 255 },
                rgb::Rgb { r: 64, g: 192, b: 32 },
                rgb::Rgb { r: 100, g: 100, b: 100 },
                rgb::Rgb { r: 255, g: 0, b: 128 },
            ];
            let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
            let enc = PnmEncoding::new();
            let output = enc.encode_rgb8(img.as_ref()).unwrap();

            let dec = PnmDecoding::new();
            let buf = vec![rgb::Rgb { r: 0.0f32, g: 0.0, b: 0.0 }; 4];
            let mut dst = imgref::ImgVec::new(buf, 2, 2);
            dec.decode_into_rgb_f32(output.bytes(), dst.as_mut()).unwrap();
            let result = dst.into_buf();

            for (orig, decoded) in pixels.iter().zip(result.iter()) {
                let expected_r = srgb_to_linear_fast(orig.r as f32 / 255.0);
                let expected_g = srgb_to_linear_fast(orig.g as f32 / 255.0);
                let expected_b = srgb_to_linear_fast(orig.b as f32 / 255.0);
                assert!((decoded.r - expected_r).abs() < 1e-5);
                assert!((decoded.g - expected_g).abs() < 1e-5);
                assert!((decoded.b - expected_b).abs() < 1e-5);
            }
        });
        assert!(report.permutations_run >= 1);
    }
}
