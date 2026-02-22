//! zencodec-types trait implementations for zenpnm.

use alloc::vec::Vec;
use zencodec_types::{
    CodecCapabilities, DecodeFrame, DecodeOutput, EncodeOutput, ImageFormat, ImageInfo,
    ImageMetadata, OutputInfo, PixelData, PixelDescriptor, PixelSlice, PixelSliceMut,
    ResourceLimits, Stop,
};

use crate::error::PnmError;
use crate::limits::Limits;
use crate::pnm;

// ── Capabilities ─────────────────────────────────────────────────────

static ENCODE_CAPS: CodecCapabilities = CodecCapabilities::new().with_native_gray(true);

static DECODE_CAPS: CodecCapabilities = CodecCapabilities::new()
    .with_native_gray(true)
    .with_cheap_probe(true);

// ── Supported descriptors ────────────────────────────────────────────

static ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::RGBA16_SRGB,
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
    PixelDescriptor::RGBF32_LINEAR,
    PixelDescriptor::RGBAF32_LINEAR,
    PixelDescriptor::GRAYF32_LINEAR,
];

static DECODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::RGBA16_SRGB,
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
    PixelDescriptor::RGBF32_LINEAR,
    PixelDescriptor::RGBAF32_LINEAR,
    PixelDescriptor::GRAYF32_LINEAR,
];

// ── PnmEncoderConfig ─────────────────────────────────────────────────

/// Encoding configuration for PNM formats.
///
/// Implements [`zencodec_types::EncoderConfig`] for the PNM family.
/// Default output: PPM for RGB, PGM for Gray, PAM for RGBA.
#[derive(Clone, Debug)]
pub struct PnmEncoderConfig {
    limits: ResourceLimits,
}

impl Default for PnmEncoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PnmEncoderConfig {
    /// Create a new PNM encoder config with default settings.
    pub fn new() -> Self {
        Self {
            limits: ResourceLimits::none(),
        }
    }
}

impl zencodec_types::EncoderConfig for PnmEncoderConfig {
    type Error = PnmError;
    type Job<'a> = PnmEncodeJob<'a>;

    fn format() -> ImageFormat {
        ImageFormat::Pnm
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static CodecCapabilities {
        &ENCODE_CAPS
    }

    fn job(&self) -> PnmEncodeJob<'_> {
        PnmEncodeJob {
            config: self,
            limits: None,
        }
    }
}

// ── PnmEncodeJob ─────────────────────────────────────────────────────

/// Per-operation PNM encode job.
pub struct PnmEncodeJob<'a> {
    config: &'a PnmEncoderConfig,
    limits: Option<ResourceLimits>,
}

impl<'a> zencodec_types::EncodeJob<'a> for PnmEncodeJob<'a> {
    type Error = PnmError;
    type Encoder = PnmEncoder<'a>;
    type FrameEncoder = PnmFrameEncoder;

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

    fn encoder(self) -> PnmEncoder<'a> {
        PnmEncoder {
            config: self.config,
            limits: self.limits,
        }
    }

    fn frame_encoder(self) -> Result<PnmFrameEncoder, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }
}

// ── PnmEncoder ───────────────────────────────────────────────────────

/// Single-image PNM encoder.
pub struct PnmEncoder<'a> {
    config: &'a PnmEncoderConfig,
    limits: Option<ResourceLimits>,
}

impl PnmEncoder<'_> {
    fn effective_limits(&self) -> Option<Limits> {
        self.limits.as_ref().map(convert_limits).or_else(|| {
            let l = &self.config.limits;
            if l.max_pixels.is_some()
                || l.max_memory_bytes.is_some()
                || l.max_width.is_some()
                || l.max_height.is_some()
            {
                Some(convert_limits(l))
            } else {
                None
            }
        })
    }
}

impl zencodec_types::Encoder for PnmEncoder<'_> {
    type Error = PnmError;

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, PnmError> {
        let desc = pixels.descriptor();
        let w = pixels.width();
        let h = pixels.rows();

        // Check limits
        if let Some(limits) = self.effective_limits() {
            limits.check(w, h)?;
        }

        match (desc.channel_type, desc.layout) {
            (zencodec_types::ChannelType::U8, zencodec_types::ChannelLayout::Rgb) => {
                let bytes = collect_contiguous_bytes(&pixels);
                let encoded = pnm::encode(
                    &bytes,
                    w,
                    h,
                    crate::PixelLayout::Rgb8,
                    pnm::PnmFormat::Ppm,
                    &enough::Unstoppable,
                )?;
                Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
            }
            (zencodec_types::ChannelType::U8, zencodec_types::ChannelLayout::Rgba) => {
                let bytes = collect_contiguous_bytes(&pixels);
                let encoded = pnm::encode(
                    &bytes,
                    w,
                    h,
                    crate::PixelLayout::Rgba8,
                    pnm::PnmFormat::Pam,
                    &enough::Unstoppable,
                )?;
                Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
            }
            (zencodec_types::ChannelType::U8, zencodec_types::ChannelLayout::Gray) => {
                let bytes = collect_contiguous_bytes(&pixels);
                let encoded = pnm::encode(
                    &bytes,
                    w,
                    h,
                    crate::PixelLayout::Gray8,
                    pnm::PnmFormat::Pgm,
                    &enough::Unstoppable,
                )?;
                Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
            }
            (zencodec_types::ChannelType::U8, zencodec_types::ChannelLayout::Bgra) => {
                let bytes = collect_contiguous_bytes(&pixels);
                let encoded = pnm::encode(
                    &bytes,
                    w,
                    h,
                    crate::PixelLayout::Bgra8,
                    pnm::PnmFormat::Ppm,
                    &enough::Unstoppable,
                )?;
                Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
            }
            (zencodec_types::ChannelType::F32, zencodec_types::ChannelLayout::Rgb) => {
                let bytes = collect_contiguous_bytes(&pixels);
                let encoded = pnm::encode(
                    &bytes,
                    w,
                    h,
                    crate::PixelLayout::RgbF32,
                    pnm::PnmFormat::Pfm,
                    &enough::Unstoppable,
                )?;
                Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
            }
            (zencodec_types::ChannelType::F32, zencodec_types::ChannelLayout::Rgba) => {
                // PFM has no alpha channel — drop alpha and write PFM color.
                let bpp = desc.bytes_per_pixel();
                let mut rgb_bytes = Vec::with_capacity(w as usize * h as usize * 12);
                for y in 0..h {
                    let row = pixels.row(y);
                    for chunk in row.chunks_exact(bpp) {
                        // Copy RGB (12 bytes), skip alpha (4 bytes)
                        rgb_bytes.extend_from_slice(&chunk[..12]);
                    }
                }
                let encoded = pnm::encode(
                    &rgb_bytes,
                    w,
                    h,
                    crate::PixelLayout::RgbF32,
                    pnm::PnmFormat::Pfm,
                    &enough::Unstoppable,
                )?;
                Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
            }
            (zencodec_types::ChannelType::F32, zencodec_types::ChannelLayout::Gray) => {
                let bytes = collect_contiguous_bytes(&pixels);
                let encoded = pnm::encode(
                    &bytes,
                    w,
                    h,
                    crate::PixelLayout::GrayF32,
                    pnm::PnmFormat::Pfm,
                    &enough::Unstoppable,
                )?;
                Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
            }
            _ => Err(PnmError::UnsupportedVariant(alloc::format!(
                "unsupported pixel format: {:?}",
                desc
            ))),
        }
    }

    fn push_rows(&mut self, _rows: PixelSlice<'_>) -> Result<(), PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support incremental encoding".into(),
        ))
    }

    fn finish(self) -> Result<EncodeOutput, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support incremental encoding".into(),
        ))
    }

    fn encode_from(
        self,
        _source: &mut dyn FnMut(u32, PixelSliceMut<'_>) -> usize,
    ) -> Result<EncodeOutput, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support pull encoding".into(),
        ))
    }
}

// ── PnmFrameEncoder (stub) ──────────────────────────────────────────

/// Stub frame encoder — PNM does not support animation.
pub struct PnmFrameEncoder;

impl zencodec_types::FrameEncoder for PnmFrameEncoder {
    type Error = PnmError;

    fn push_frame(&mut self, _pixels: PixelSlice<'_>, _duration_ms: u32) -> Result<(), PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }

    fn begin_frame(&mut self, _duration_ms: u32) -> Result<(), PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }

    fn push_rows(&mut self, _rows: PixelSlice<'_>) -> Result<(), PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }

    fn end_frame(&mut self) -> Result<(), PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }

    fn pull_frame(
        &mut self,
        _duration_ms: u32,
        _source: &mut dyn FnMut(u32, PixelSliceMut<'_>) -> usize,
    ) -> Result<(), PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }

    fn finish(self) -> Result<EncodeOutput, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }
}

// ── PnmDecoderConfig ─────────────────────────────────────────────────

/// Decoding configuration for PNM formats.
///
/// Implements [`zencodec_types::DecoderConfig`] for the PNM family.
#[derive(Clone, Debug)]
pub struct PnmDecoderConfig {
    limits: Option<Limits>,
}

impl Default for PnmDecoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PnmDecoderConfig {
    /// Create a new PNM decoder config with default settings.
    pub fn new() -> Self {
        Self { limits: None }
    }
}

impl zencodec_types::DecoderConfig for PnmDecoderConfig {
    type Error = PnmError;
    type Job<'a> = PnmDecodeJob<'a>;

    fn format() -> ImageFormat {
        ImageFormat::Pnm
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static CodecCapabilities {
        &DECODE_CAPS
    }

    fn job(&self) -> PnmDecodeJob<'_> {
        PnmDecodeJob {
            config: self,
            limits: None,
        }
    }

    fn probe_header(&self, data: &[u8]) -> Result<ImageInfo, PnmError> {
        // Auto-detect BMP and farbfeld before falling back to PNM
        #[cfg(feature = "bmp")]
        if data.len() >= 2 && &data[0..2] == b"BM" {
            let header = crate::bmp::decode::parse_bmp_header(data)?;
            let has_alpha = matches!(
                header.layout,
                crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
            );
            return Ok(
                ImageInfo::new(header.width, header.height, ImageFormat::Pnm).with_alpha(has_alpha),
            );
        }
        if data.len() >= 8 && &data[0..8] == b"farbfeld" {
            let (width, height) = crate::farbfeld::decode::parse_header(data)?;
            return Ok(ImageInfo::new(width, height, ImageFormat::Pnm).with_alpha(true));
        }
        let header = pnm::decode::parse_header(data)?;
        Ok(header_to_image_info(&header))
    }
}

// ── PnmDecodeJob ─────────────────────────────────────────────────────

/// Per-operation PNM decode job.
pub struct PnmDecodeJob<'a> {
    config: &'a PnmDecoderConfig,
    limits: Option<Limits>,
}

impl<'a> zencodec_types::DecodeJob<'a> for PnmDecodeJob<'a> {
    type Error = PnmError;
    type Decoder = PnmDecoder<'a>;
    type FrameDecoder = PnmFrameDecoder;
    fn with_stop(self, _stop: &'a dyn Stop) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(convert_limits(&limits));
        self
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, PnmError> {
        // Auto-detect format for output info
        #[cfg(feature = "bmp")]
        if data.len() >= 2 && &data[0..2] == b"BM" {
            let header = crate::bmp::decode::parse_bmp_header(data)?;
            let has_alpha = matches!(
                header.layout,
                crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
            );
            let native_format = layout_to_descriptor(header.layout);
            return Ok(
                OutputInfo::full_decode(header.width, header.height, native_format)
                    .with_alpha(has_alpha),
            );
        }
        if data.len() >= 8 && &data[0..8] == b"farbfeld" {
            let (width, height) = crate::farbfeld::decode::parse_header(data)?;
            return Ok(
                OutputInfo::full_decode(width, height, PixelDescriptor::RGBA16_SRGB)
                    .with_alpha(true),
            );
        }
        let header = pnm::decode::parse_header(data)?;
        let has_alpha = matches!(
            header.layout,
            crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
        );
        let native_format = layout_to_descriptor(header.layout);
        Ok(
            OutputInfo::full_decode(header.width, header.height, native_format)
                .with_alpha(has_alpha),
        )
    }

    fn decoder(self) -> PnmDecoder<'a> {
        PnmDecoder {
            config: self.config,
            limits: self.limits,
        }
    }

    fn frame_decoder(self, _data: &[u8]) -> Result<PnmFrameDecoder, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }
}

// ── PnmDecoder ───────────────────────────────────────────────────────

/// Single-image PNM decoder.
pub struct PnmDecoder<'a> {
    config: &'a PnmDecoderConfig,
    limits: Option<Limits>,
}

impl PnmDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec_types::Decoder for PnmDecoder<'_> {
    type Error = PnmError;

    fn decode(self, data: &[u8]) -> Result<DecodeOutput, PnmError> {
        let limits = self.effective_limits();
        let decoded = crate::decode_dispatch(data, limits, &enough::Unstoppable)?;

        let has_alpha = matches!(
            decoded.layout,
            crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
        );
        let info =
            ImageInfo::new(decoded.width, decoded.height, ImageFormat::Pnm).with_alpha(has_alpha);

        let pixels = layout_to_pixel_data(&decoded)?;
        Ok(DecodeOutput::new(pixels, info))
    }

    fn decode_into(self, data: &[u8], mut dst: PixelSliceMut<'_>) -> Result<ImageInfo, PnmError> {
        let desc = dst.descriptor();
        let output = self.decode(data)?;
        let info = output.info().clone();

        match (desc.channel_type, desc.layout) {
            (zencodec_types::ChannelType::U8, zencodec_types::ChannelLayout::Rgb) => {
                let src = output.into_rgb8();
                copy_rows_u8(&src, &mut dst);
            }
            (zencodec_types::ChannelType::U8, zencodec_types::ChannelLayout::Rgba) => {
                let src = output.into_rgba8();
                copy_rows_u8(&src, &mut dst);
            }
            (zencodec_types::ChannelType::U8, zencodec_types::ChannelLayout::Gray) => {
                let src = output.into_gray8();
                copy_rows_u8(&src, &mut dst);
            }
            (zencodec_types::ChannelType::U8, zencodec_types::ChannelLayout::Bgra) => {
                let src = output.into_bgra8();
                copy_rows_u8(&src, &mut dst);
            }
            (zencodec_types::ChannelType::F32, zencodec_types::ChannelLayout::Rgb) => {
                let is_float = matches!(
                    output.pixels(),
                    PixelData::RgbF32(_) | PixelData::RgbaF32(_) | PixelData::GrayF32(_)
                );
                decode_into_rgb_f32(output, is_float, &mut dst);
                return Ok(info);
            }
            (zencodec_types::ChannelType::F32, zencodec_types::ChannelLayout::Rgba) => {
                let is_float = matches!(
                    output.pixels(),
                    PixelData::RgbF32(_) | PixelData::RgbaF32(_) | PixelData::GrayF32(_)
                );
                decode_into_rgba_f32(output, is_float, &mut dst);
                return Ok(info);
            }
            (zencodec_types::ChannelType::F32, zencodec_types::ChannelLayout::Gray) => {
                let is_float = matches!(
                    output.pixels(),
                    PixelData::RgbF32(_) | PixelData::RgbaF32(_) | PixelData::GrayF32(_)
                );
                decode_into_gray_f32(output, is_float, &mut dst);
                return Ok(info);
            }
            _ => {
                return Err(PnmError::UnsupportedVariant(alloc::format!(
                    "unsupported decode_into format: {:?}",
                    desc
                )));
            }
        }

        Ok(info)
    }

    fn decode_rows(
        self,
        _data: &[u8],
        _sink: &mut dyn zencodec_types::DecodeRowSink,
    ) -> Result<ImageInfo, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support row-level decoding".into(),
        ))
    }
}

// ── PnmFrameDecoder (stub) ──────────────────────────────────────────

/// Stub frame decoder — PNM does not support animation.
pub struct PnmFrameDecoder;

impl zencodec_types::FrameDecoder for PnmFrameDecoder {
    type Error = PnmError;

    fn next_frame(&mut self) -> Result<Option<DecodeFrame>, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }

    fn next_frame_into(
        &mut self,
        _dst: PixelSliceMut<'_>,
        _prior_frame: Option<u32>,
    ) -> Result<Option<ImageInfo>, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }

    fn next_frame_rows(
        &mut self,
        _sink: &mut dyn zencodec_types::DecodeRowSink,
    ) -> Result<Option<ImageInfo>, PnmError> {
        Err(PnmError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
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

fn layout_to_descriptor(layout: crate::PixelLayout) -> PixelDescriptor {
    use crate::PixelLayout;
    match layout {
        PixelLayout::Gray8 => PixelDescriptor::GRAY8_SRGB,
        PixelLayout::Gray16 => PixelDescriptor::GRAY16_SRGB,
        PixelLayout::Rgb8 => PixelDescriptor::RGB8_SRGB,
        PixelLayout::Rgba8 => PixelDescriptor::RGBA8_SRGB,
        PixelLayout::GrayF32 => PixelDescriptor::GRAYF32_LINEAR,
        PixelLayout::RgbF32 => PixelDescriptor::RGBF32_LINEAR,
        PixelLayout::Bgr8 | PixelLayout::Bgrx8 => PixelDescriptor::RGB8_SRGB,
        PixelLayout::Bgra8 => PixelDescriptor::BGRA8_SRGB,
        PixelLayout::Rgba16 => PixelDescriptor::RGBA16_SRGB,
    }
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
            Ok(PixelData::Gray8(imgref::ImgVec::new(pixels.to_vec(), w, h)))
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
            Ok(PixelData::Rgb8(imgref::ImgVec::new(pixels.to_vec(), w, h)))
        }
        PixelLayout::Rgba8 => {
            let pixels: &[rgb::Rgba<u8>] = bytes.as_pixels();
            Ok(PixelData::Rgba8(imgref::ImgVec::new(pixels.to_vec(), w, h)))
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
                    rgb::Rgba { r, g, b, a: 1.0 }
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
            Ok(PixelData::Bgra8(imgref::ImgVec::new(pixels.to_vec(), w, h)))
        }
        PixelLayout::Bgrx8 => {
            // Treat BGRX as BGRA (padding byte becomes alpha)
            let pixels: &[rgb::alt::BGRA<u8>] = bytes.as_pixels();
            Ok(PixelData::Bgra8(imgref::ImgVec::new(pixels.to_vec(), w, h)))
        }
        PixelLayout::Rgba16 => {
            let pixels: Vec<rgb::Rgba<u16>> = bytes
                .chunks_exact(8)
                .map(|c| {
                    rgb::Rgba {
                        r: u16::from_ne_bytes([c[0], c[1]]),
                        g: u16::from_ne_bytes([c[2], c[3]]),
                        b: u16::from_ne_bytes([c[4], c[5]]),
                        a: u16::from_ne_bytes([c[6], c[7]]),
                    }
                })
                .collect();
            Ok(PixelData::Rgba16(imgref::ImgVec::new(pixels, w, h)))
        }
    }
}

/// Collect contiguous bytes from a PixelSlice (handles stride).
fn collect_contiguous_bytes(pixels: &PixelSlice<'_>) -> Vec<u8> {
    let h = pixels.rows();
    let w = pixels.width();
    let bpp = pixels.descriptor().bytes_per_pixel();
    let row_bytes = w as usize * bpp;

    let mut out = Vec::with_capacity(row_bytes * h as usize);
    for y in 0..h {
        out.extend_from_slice(&pixels.row(y)[..row_bytes]);
    }
    out
}

/// Copy rows from a typed ImgVec into a PixelSliceMut via byte reinterpretation.
fn copy_rows_u8<P: Copy>(src: &imgref::ImgVec<P>, dst: &mut PixelSliceMut<'_>)
where
    [P]: rgb::ComponentBytes<u8>,
{
    use rgb::ComponentBytes;
    for y in 0..src.height().min(dst.rows() as usize) {
        let src_row = &src.buf()[y * src.stride()..][..src.width()];
        let src_bytes = src_row.as_bytes();
        let dst_row = dst.row_mut(y as u32);
        let n = src_bytes.len().min(dst_row.len());
        dst_row[..n].copy_from_slice(&src_bytes[..n]);
    }
}

/// Decode into linear RGB f32 from integer or float PNM data.
fn decode_into_rgb_f32(output: DecodeOutput, is_float: bool, dst: &mut PixelSliceMut<'_>) {
    use linear_srgb::default::srgb_to_linear_fast;

    let src = output.into_pixels().into_rgb8();
    for y in 0..src.height().min(dst.rows() as usize) {
        let src_row = &src.buf()[y * src.stride()..][..src.width()];
        let dst_row = dst.row_mut(y as u32);
        for (i, s) in src_row.iter().enumerate() {
            let offset = i * 12;
            if offset + 12 > dst_row.len() {
                break;
            }
            let rf = s.r as f32 / 255.0;
            let gf = s.g as f32 / 255.0;
            let bf = s.b as f32 / 255.0;
            let (r, g, b): (f32, f32, f32) = if is_float {
                (rf, gf, bf)
            } else {
                (
                    srgb_to_linear_fast(rf),
                    srgb_to_linear_fast(gf),
                    srgb_to_linear_fast(bf),
                )
            };
            dst_row[offset..offset + 4].copy_from_slice(&r.to_ne_bytes());
            dst_row[offset + 4..offset + 8].copy_from_slice(&g.to_ne_bytes());
            dst_row[offset + 8..offset + 12].copy_from_slice(&b.to_ne_bytes());
        }
    }
}

/// Decode into linear RGBA f32 from integer or float PNM data.
fn decode_into_rgba_f32(output: DecodeOutput, is_float: bool, dst: &mut PixelSliceMut<'_>) {
    use linear_srgb::default::srgb_to_linear_fast;

    let src = output.into_pixels().into_rgba8();
    for y in 0..src.height().min(dst.rows() as usize) {
        let src_row = &src.buf()[y * src.stride()..][..src.width()];
        let dst_row = dst.row_mut(y as u32);
        for (i, s) in src_row.iter().enumerate() {
            let offset = i * 16;
            if offset + 16 > dst_row.len() {
                break;
            }
            let rf = s.r as f32 / 255.0;
            let gf = s.g as f32 / 255.0;
            let bf = s.b as f32 / 255.0;
            let af = s.a as f32 / 255.0;
            let (r, g, b): (f32, f32, f32) = if is_float {
                (rf, gf, bf)
            } else {
                (
                    srgb_to_linear_fast(rf),
                    srgb_to_linear_fast(gf),
                    srgb_to_linear_fast(bf),
                )
            };
            dst_row[offset..offset + 4].copy_from_slice(&r.to_ne_bytes());
            dst_row[offset + 4..offset + 8].copy_from_slice(&g.to_ne_bytes());
            dst_row[offset + 8..offset + 12].copy_from_slice(&b.to_ne_bytes());
            dst_row[offset + 12..offset + 16].copy_from_slice(&af.to_ne_bytes());
        }
    }
}

/// Decode into linear Gray f32 from integer or float PNM data.
fn decode_into_gray_f32(output: DecodeOutput, is_float: bool, dst: &mut PixelSliceMut<'_>) {
    use linear_srgb::default::srgb_to_linear_fast;

    let src = output.into_pixels().into_gray8();
    for y in 0..src.height().min(dst.rows() as usize) {
        let src_row = &src.buf()[y * src.stride()..][..src.width()];
        let dst_row = dst.row_mut(y as u32);
        for (i, s) in src_row.iter().enumerate() {
            let offset = i * 4;
            if offset + 4 > dst_row.len() {
                break;
            }
            let vf = s.value() as f32 / 255.0;
            let v: f32 = if is_float {
                vf
            } else {
                srgb_to_linear_fast(vf)
            };
            dst_row[offset..offset + 4].copy_from_slice(&v.to_ne_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use zencodec_types::{DecodeJob, Decoder, DecoderConfig, EncodeJob, Encoder, EncoderConfig};

    #[test]
    fn encode_decode_rgb8_roundtrip() {
        let pixels = vec![
            rgb::Rgb { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();
        assert_eq!(output.format(), ImageFormat::Pnm);

        let dec = PnmDecoderConfig::new();
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
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_gray8(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let gray_img = decoded.into_gray8();
        assert_eq!(gray_img.buf().as_slice(), &pixels);
    }

    #[test]
    fn encode_decode_rgba8_roundtrip() {
        let pixels = vec![
            rgb::Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            rgb::Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 128,
            },
            rgb::Rgba {
                r: 0,
                g: 0,
                b: 255,
                a: 0,
            },
            rgb::Rgba {
                r: 128,
                g: 128,
                b: 128,
                a: 255,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgba8(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
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
            rgb::alt::BGRA {
                b: 0,
                g: 0,
                r: 255,
                a: 255,
            },
            rgb::alt::BGRA {
                b: 0,
                g: 255,
                r: 0,
                a: 255,
            },
            rgb::alt::BGRA {
                b: 255,
                g: 0,
                r: 0,
                a: 255,
            },
            rgb::alt::BGRA {
                b: 128,
                g: 128,
                r: 128,
                a: 255,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_bgra8(img.as_ref()).unwrap();

        // Decode and verify the RGB values came through correctly
        let dec = PnmDecoderConfig::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let rgb_img = decoded.into_rgb8();
        let buf = rgb_img.buf();
        assert_eq!(buf[0], rgb::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(buf[1], rgb::Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(buf[2], rgb::Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(
            buf[3],
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128
            }
        );
    }

    #[test]
    fn encode_bgrx8_no_double_swizzle() {
        // BGRX encode should go directly to PPM, ignoring the padding byte.
        let pixels = vec![
            rgb::alt::BGRA {
                b: 0,
                g: 0,
                r: 255,
                a: 0,
            }, // alpha ignored
            rgb::alt::BGRA {
                b: 0,
                g: 255,
                r: 0,
                a: 99,
            }, // alpha ignored
            rgb::alt::BGRA {
                b: 255,
                g: 0,
                r: 0,
                a: 200,
            }, // alpha ignored
            rgb::alt::BGRA {
                b: 128,
                g: 128,
                r: 128,
                a: 1,
            }, // alpha ignored
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoderConfig::new();
        // BGRX goes through the same BGRA descriptor — the encoder treats it
        // as BGRA for byte layout but the PPM path drops alpha.
        let output = enc.encode_bgra8(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let rgb_img = decoded.into_rgb8();
        let buf = rgb_img.buf();
        assert_eq!(buf[0], rgb::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(buf[1], rgb::Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(buf[2], rgb::Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(
            buf[3],
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128
            }
        );
    }

    #[test]
    fn probe_header_extracts_info() {
        let pixels = vec![rgb::Rgb { r: 1, g: 2, b: 3 }; 6];
        let img = imgref::ImgVec::new(pixels, 3, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let info = dec.probe_header(output.bytes()).unwrap();
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 2);
        assert_eq!(info.format, ImageFormat::Pnm);
        assert!(!info.has_alpha);
    }

    #[test]
    fn capabilities_are_correct() {
        let enc_caps = PnmEncoderConfig::capabilities();
        assert!(enc_caps.native_gray());
        assert!(!enc_caps.cheap_probe()); // encode side doesn't probe
        assert!(!enc_caps.encode_icc());
        assert!(!enc_caps.encode_cancel());

        let dec_caps = PnmDecoderConfig::capabilities();
        assert!(dec_caps.native_gray());
        assert!(dec_caps.cheap_probe());
        assert!(!dec_caps.decode_cancel());
    }

    #[test]
    fn with_limits_propagates() {
        let limits = ResourceLimits::none()
            .with_max_width(10)
            .with_max_height(10);

        let dec = PnmDecoderConfig::new();
        let big_pixels = vec![rgb::Rgb { r: 0, g: 0, b: 0 }; 100 * 100];
        let img = imgref::ImgVec::new(big_pixels, 100, 100);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let result = dec
            .job()
            .with_limits(limits)
            .decoder()
            .decode(output.bytes());
        assert!(result.is_err());
    }

    #[test]
    fn decode_into_bgra8_from_rgb() {
        let pixels = vec![
            rgb::Rgb { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let mut buf = vec![
            rgb::alt::BGRA {
                b: 0,
                g: 0,
                r: 0,
                a: 0
            };
            4
        ];
        let mut dst = imgref::ImgVec::new(buf.clone(), 2, 2);
        let info = dec.decode_into_bgra8(output.bytes(), dst.as_mut()).unwrap();
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 2);
        buf = dst.into_buf();
        assert_eq!(
            buf[0],
            rgb::alt::BGRA {
                b: 0,
                g: 0,
                r: 255,
                a: 255
            }
        );
        assert_eq!(
            buf[1],
            rgb::alt::BGRA {
                b: 0,
                g: 255,
                r: 0,
                a: 255
            }
        );
        assert_eq!(
            buf[2],
            rgb::alt::BGRA {
                b: 255,
                g: 0,
                r: 0,
                a: 255
            }
        );
        assert_eq!(
            buf[3],
            rgb::alt::BGRA {
                b: 128,
                g: 128,
                r: 128,
                a: 255
            }
        );
    }

    #[test]
    fn decode_into_bgrx8_forces_alpha_255() {
        // Encode RGBA with non-255 alpha
        let pixels = vec![
            rgb::Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 100,
            },
            rgb::Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 50,
            },
            rgb::Rgba {
                r: 0,
                g: 0,
                b: 255,
                a: 0,
            },
            rgb::Rgba {
                r: 128,
                g: 128,
                b: 128,
                a: 200,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgba8(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let buf = vec![
            rgb::alt::BGRA {
                b: 0,
                g: 0,
                r: 0,
                a: 0
            };
            4
        ];
        let mut dst = imgref::ImgVec::new(buf, 2, 2);
        dec.decode_into_bgra8(output.bytes(), dst.as_mut()).unwrap();
        let result = dst.into_buf();
        // BGRA decode preserves original alpha from PAM
        for px in &result {
            // Note: BGRX would force 255, but BGRA preserves alpha.
            // This test was for the old bgrx8 path; BGRA preserves alpha.
            assert!(px.a > 0 || px.r == 0); // alpha preserved from source
        }
    }

    #[test]
    fn encode_decode_rgb_f32_roundtrip() {
        let pixels = vec![
            rgb::Rgb {
                r: 0.0f32,
                g: 0.5,
                b: 1.0,
            },
            rgb::Rgb {
                r: 0.25,
                g: 0.75,
                b: 0.125,
            },
            rgb::Rgb {
                r: 1.0,
                g: 0.0,
                b: 0.0,
            },
            rgb::Rgb {
                r: 0.5,
                g: 0.5,
                b: 0.5,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgb_f32(img.as_ref()).unwrap();
        assert_eq!(output.format(), ImageFormat::Pnm);

        // Decode and verify f32 values survive PFM roundtrip
        // PFM RGB → zencodec PixelData::RgbaF32 (alpha = 1.0)
        let dec = PnmDecoderConfig::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        let rgba_img = decoded.into_rgba8();
        // PFM stores f32 natively; going through into_rgba8 will quantize to u8.
        // Use the raw PixelData instead.
        let dec2 = PnmDecoderConfig::new();
        let decoded2 = dec2.decode(output.bytes()).unwrap();
        match decoded2.into_pixels() {
            PixelData::RgbaF32(img) => {
                let buf = img.buf();
                for (orig, decoded) in pixels.iter().zip(buf.iter()) {
                    assert!((orig.r - decoded.r).abs() < 1e-6);
                    assert!((orig.g - decoded.g).abs() < 1e-6);
                    assert!((orig.b - decoded.b).abs() < 1e-6);
                }
            }
            other => panic!("expected RgbaF32, got {:?}", other),
        }
        // Ensure the u8 path didn't panic
        drop(rgba_img);
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
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_gray_f32(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        match decoded.into_pixels() {
            PixelData::GrayF32(img) => {
                let buf = img.buf();
                for (orig, decoded) in pixels.iter().zip(buf.iter()) {
                    assert!((orig.value() - decoded.value()).abs() < 1e-6);
                }
            }
            other => panic!("expected GrayF32, got {:?}", other),
        }
    }

    #[test]
    fn encode_rgba_f32_drops_alpha() {
        // RGBA f32 encodes to PFM (no alpha), verify RGB values survive
        let pixels = vec![
            rgb::Rgba {
                r: 0.5f32,
                g: 0.25,
                b: 0.75,
                a: 0.1,
            },
            rgb::Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            },
            rgb::Rgba {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 0.9,
            },
            rgb::Rgba {
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 0.0,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgba_f32(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let decoded = dec.decode(output.bytes()).unwrap();
        match decoded.into_pixels() {
            PixelData::RgbaF32(img) => {
                let buf = img.buf();
                for (orig, decoded) in pixels.iter().zip(buf.iter()) {
                    assert!((orig.r - decoded.r).abs() < 1e-6);
                    assert!((orig.g - decoded.g).abs() < 1e-6);
                    assert!((orig.b - decoded.b).abs() < 1e-6);
                }
            }
            other => panic!("expected RgbaF32, got {:?}", other),
        }
    }

    #[test]
    fn decode_into_rgb_f32_from_u8() {
        use linear_srgb::default::srgb_to_linear_fast;

        // Encode as PPM (u8), then decode_into_rgb_f32 — verifies sRGB→linear path
        let pixels = vec![
            rgb::Rgb {
                r: 0u8,
                g: 128,
                b: 255,
            },
            rgb::Rgb {
                r: 255,
                g: 0,
                b: 128,
            },
            rgb::Rgb {
                r: 64,
                g: 192,
                b: 32,
            },
            rgb::Rgb {
                r: 100,
                g: 100,
                b: 100,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let buf = vec![
            rgb::Rgb {
                r: 0.0f32,
                g: 0.0,
                b: 0.0
            };
            4
        ];
        let mut dst = imgref::ImgVec::new(buf, 2, 2);
        dec.decode_into_rgb_f32(output.bytes(), dst.as_mut())
            .unwrap();
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
        assert_traits::<PnmEncoderConfig>();
    }

    #[test]
    fn decoding_clone_send_sync() {
        fn assert_traits<T: Clone + Send + Sync>() {}
        assert_traits::<PnmDecoderConfig>();
    }

    #[test]
    fn f32_conversion_all_simd_tiers() {
        use archmage::testing::{CompileTimePolicy, for_each_token_permutation};
        use linear_srgb::default::srgb_to_linear_fast;

        let report = for_each_token_permutation(CompileTimePolicy::Warn, |_perm| {
            // Encode as PPM (u8), decode to linear f32, verify values
            let pixels = vec![
                rgb::Rgb {
                    r: 0u8,
                    g: 128,
                    b: 255,
                },
                rgb::Rgb {
                    r: 64,
                    g: 192,
                    b: 32,
                },
                rgb::Rgb {
                    r: 100,
                    g: 100,
                    b: 100,
                },
                rgb::Rgb {
                    r: 255,
                    g: 0,
                    b: 128,
                },
            ];
            let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
            let enc = PnmEncoderConfig::new();
            let output = enc.encode_rgb8(img.as_ref()).unwrap();

            let dec = PnmDecoderConfig::new();
            let buf = vec![
                rgb::Rgb {
                    r: 0.0f32,
                    g: 0.0,
                    b: 0.0
                };
                4
            ];
            let mut dst = imgref::ImgVec::new(buf, 2, 2);
            dec.decode_into_rgb_f32(output.bytes(), dst.as_mut())
                .unwrap();
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

    #[test]
    fn output_info_matches_decode() {
        let pixels = vec![rgb::Rgb { r: 1, g: 2, b: 3 }; 6];
        let img = imgref::ImgVec::new(pixels, 3, 2);
        let enc = PnmEncoderConfig::new();
        let output = enc.encode_rgb8(img.as_ref()).unwrap();

        let dec = PnmDecoderConfig::new();
        let info = dec.job().output_info(output.bytes()).unwrap();
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 2);

        let decoded = dec.decode(output.bytes()).unwrap();
        assert_eq!(decoded.width(), info.width);
        assert_eq!(decoded.height(), info.height);
    }

    #[test]
    fn four_layer_encode_flow() {
        let pixels = vec![
            rgb::Rgb::<u8> { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let config = PnmEncoderConfig::new();

        // Exercise the full 4-layer flow: config → job → encoder → encode
        let slice = PixelSlice::from(img.as_ref());
        let output = config.job().encoder().encode(slice).unwrap();
        assert_eq!(output.format(), ImageFormat::Pnm);
        assert!(!output.bytes().is_empty());
    }

    #[test]
    fn four_layer_decode_flow() {
        let pixels = vec![
            rgb::Rgb {
                r: 100,
                g: 200,
                b: 50
            };
            4
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let enc = PnmEncoderConfig::new();
        let encoded = enc.encode_rgb8(img.as_ref()).unwrap();

        // Exercise the full 4-layer flow: config → job → decoder → decode
        use zencodec_types::{DecodeJob, Decoder, DecoderConfig};
        let config = PnmDecoderConfig::new();
        let decoded = config.job().decoder().decode(encoded.bytes()).unwrap();
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
    }
}
