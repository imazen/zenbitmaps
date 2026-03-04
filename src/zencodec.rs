//! zencodec-types trait implementations for zenbitmaps.
//!
//! Provides per-format codec pairs:
//! - PNM: PnmEncoderConfig / PnmDecoderConfig (always available)
//! - BMP: BmpEncoderConfig / BmpDecoderConfig (requires `bmp` feature)
//! - Farbfeld: FarbfeldEncoderConfig / FarbfeldDecoderConfig (always available)

use alloc::vec::Vec;
use zencodec_types::{
    CodecCapabilities, DecodeFrame, DecodeOutput, EncodeOutput, ImageFormat, ImageInfo,
    MetadataView, OutputInfo, ResourceLimits, Stop,
};
use zenpixels::{ChannelLayout, ChannelType, PixelBuffer, PixelDescriptor, PixelSlice};

use crate::error::BitmapError;
use crate::limits::Limits;
use crate::pnm;

// ══════════════════════════════════════════════════════════════════════
// Shared capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static PNM_ENCODE_CAPS: CodecCapabilities = CodecCapabilities::new().with_native_gray(true);

static PNM_DECODE_CAPS: CodecCapabilities = CodecCapabilities::new()
    .with_native_gray(true)
    .with_cheap_probe(true);

static PNM_ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::RGBA16_SRGB,
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
    PixelDescriptor::RGBF32_LINEAR,
    PixelDescriptor::RGBAF32_LINEAR,
    PixelDescriptor::GRAYF32_LINEAR,
];

static PNM_DECODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::RGBA16_SRGB,
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
    PixelDescriptor::RGBF32_LINEAR,
    PixelDescriptor::RGBAF32_LINEAR,
    PixelDescriptor::GRAYF32_LINEAR,
];

#[cfg(feature = "bmp")]
static BMP_ENCODE_CAPS: CodecCapabilities = CodecCapabilities::new();

#[cfg(feature = "bmp")]
static BMP_DECODE_CAPS: CodecCapabilities = CodecCapabilities::new().with_cheap_probe(true);

#[cfg(feature = "bmp")]
static BMP_ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
];

#[cfg(feature = "bmp")]
static BMP_DECODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
];

static FF_ENCODE_CAPS: CodecCapabilities = CodecCapabilities::new();

static FF_DECODE_CAPS: CodecCapabilities = CodecCapabilities::new().with_cheap_probe(true);

static FF_ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGBA16_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::GRAY8_SRGB,
];

static FF_DECODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGBA16_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::GRAY8_SRGB,
];

// ══════════════════════════════════════════════════════════════════════
// PNM codec
// ══════════════════════════════════════════════════════════════════════

// ── PnmEncoderConfig ─────────────────────────────────────────────────

/// Encoding configuration for PNM formats.
///
/// Implements [`zencodec_types::EncoderConfig`] for the PNM family.
/// Default output: PPM for RGB, PGM for Gray, PAM for RGBA, PFM for float.
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
    type Error = BitmapError;
    type Job<'a> = PnmEncodeJob<'a>;

    fn format() -> ImageFormat {
        ImageFormat::Pnm
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        PNM_ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static CodecCapabilities {
        &PNM_ENCODE_CAPS
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
    type Error = BitmapError;
    type Enc = PnmEncoder<'a>;
    type FrameEnc = PnmFrameEncoder;

    fn with_stop(self, _stop: &'a dyn Stop) -> Self {
        self
    }

    fn with_metadata(self, _meta: &'a MetadataView<'a>) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(limits);
        self
    }

    fn encoder(self) -> Result<PnmEncoder<'a>, BitmapError> {
        Ok(PnmEncoder {
            config: self.config,
            limits: self.limits,
        })
    }

    fn frame_encoder(self) -> Result<PnmFrameEncoder, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
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
    type Error = BitmapError;

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, BitmapError> {
        let desc = pixels.descriptor();
        let w = pixels.width();
        let h = pixels.rows();

        if let Some(limits) = self.effective_limits() {
            limits.check(w, h)?;
        }

        match (desc.channel_type(), desc.layout()) {
            (ChannelType::U8, ChannelLayout::Rgb) => {
                let bytes = pixels.contiguous_bytes();
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
            (ChannelType::U8, ChannelLayout::Rgba) => {
                let bytes = pixels.contiguous_bytes();
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
            (ChannelType::U8, ChannelLayout::Gray) => {
                let bytes = pixels.contiguous_bytes();
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
            (ChannelType::U8, ChannelLayout::Bgra) => {
                let bytes = pixels.contiguous_bytes();
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
            (ChannelType::F32, ChannelLayout::Rgb) => {
                let bytes = pixels.contiguous_bytes();
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
            (ChannelType::F32, ChannelLayout::Rgba) => {
                // PFM has no alpha channel — drop alpha and write PFM color.
                let bpp = desc.bytes_per_pixel();
                let mut rgb_bytes = Vec::with_capacity(w as usize * h as usize * 12);
                for y in 0..h {
                    let row = pixels.row(y);
                    for chunk in row.chunks_exact(bpp) {
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
            (ChannelType::F32, ChannelLayout::Gray) => {
                let bytes = pixels.contiguous_bytes();
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
            _ => Err(BitmapError::UnsupportedVariant(alloc::format!(
                "unsupported pixel format: {:?}",
                desc
            ))),
        }
    }
}

// ── PnmFrameEncoder (stub) ──────────────────────────────────────────

/// Stub frame encoder — PNM does not support animation.
pub struct PnmFrameEncoder;

impl zencodec_types::FrameEncoder for PnmFrameEncoder {
    type Error = BitmapError;

    fn push_frame(
        &mut self,
        _pixels: PixelSlice<'_>,
        _duration_ms: u32,
    ) -> Result<(), BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }

    fn finish(self) -> Result<EncodeOutput, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
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
    type Error = BitmapError;
    type Job<'a> = PnmDecodeJob<'a>;

    fn format() -> ImageFormat {
        ImageFormat::Pnm
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        PNM_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static CodecCapabilities {
        &PNM_DECODE_CAPS
    }

    fn job(&self) -> PnmDecodeJob<'_> {
        PnmDecodeJob {
            config: self,
            limits: None,
        }
    }
}

// ── PnmDecodeJob ─────────────────────────────────────────────────────

/// Per-operation PNM decode job.
pub struct PnmDecodeJob<'a> {
    config: &'a PnmDecoderConfig,
    limits: Option<Limits>,
}

impl<'a> zencodec_types::DecodeJob<'a> for PnmDecodeJob<'a> {
    type Error = BitmapError;
    type Dec = PnmDecoder<'a>;
    type StreamDec = NoStreamingDecode;
    type FrameDec = PnmFrameDecoder;

    fn with_stop(self, _stop: &'a dyn Stop) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(convert_limits(&limits));
        self
    }

    fn probe(&self, data: &[u8]) -> Result<ImageInfo, BitmapError> {
        let header = pnm::decode::parse_header(data)?;
        Ok(header_to_image_info(&header))
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, BitmapError> {
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

    fn decoder(
        self,
        data: &'a [u8],
        _preferred: &[PixelDescriptor],
    ) -> Result<PnmDecoder<'a>, BitmapError> {
        Ok(PnmDecoder {
            config: self.config,
            limits: self.limits,
            data,
        })
    }

    fn streaming_decoder(
        self,
        _data: &'a [u8],
        _preferred: &[PixelDescriptor],
    ) -> Result<NoStreamingDecode, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "PNM does not support streaming decode".into(),
        ))
    }

    fn frame_decoder(
        self,
        _data: &'a [u8],
        _preferred: &[PixelDescriptor],
    ) -> Result<PnmFrameDecoder, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }
}

// ── PnmDecoder ───────────────────────────────────────────────────────

/// Single-image PNM decoder.
pub struct PnmDecoder<'a> {
    config: &'a PnmDecoderConfig,
    limits: Option<Limits>,
    data: &'a [u8],
}

impl PnmDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec_types::Decode for PnmDecoder<'_> {
    type Error = BitmapError;

    fn decode(self) -> Result<DecodeOutput, BitmapError> {
        let limits = self.effective_limits();
        let decoded = crate::pnm::decode(self.data, limits, &enough::Unstoppable)?;
        decode_output_from_internal(&decoded, ImageFormat::Pnm)
    }
}
// ── PnmFrameDecoder (stub) ──────────────────────────────────────────

/// Stub frame decoder — PNM does not support animation.
pub struct PnmFrameDecoder;

impl zencodec_types::FrameDecode for PnmFrameDecoder {
    type Error = BitmapError;

    fn next_frame(&mut self) -> Result<Option<DecodeFrame>, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "PNM does not support animation".into(),
        ))
    }
}

// ══════════════════════════════════════════════════════════════════════
// BMP codec (cfg-gated)
// ══════════════════════════════════════════════════════════════════════

#[cfg(feature = "bmp")]
mod bmp_codec {
    use super::*;

    // ── BmpEncoderConfig ─────────────────────────────────────────────

    /// Encoding configuration for BMP format.
    ///
    /// Supports 24-bit RGB and 32-bit RGBA BMP output.
    #[derive(Clone, Debug)]
    pub struct BmpEncoderConfig {
        limits: ResourceLimits,
    }

    impl Default for BmpEncoderConfig {
        fn default() -> Self {
            Self::new()
        }
    }

    impl BmpEncoderConfig {
        /// Create a new BMP encoder config with default settings.
        pub fn new() -> Self {
            Self {
                limits: ResourceLimits::none(),
            }
        }
    }

    impl zencodec_types::EncoderConfig for BmpEncoderConfig {
        type Error = BitmapError;
        type Job<'a> = BmpEncodeJob<'a>;

        fn format() -> ImageFormat {
            ImageFormat::Bmp
        }

        fn supported_descriptors() -> &'static [PixelDescriptor] {
            BMP_ENCODE_DESCRIPTORS
        }

        fn capabilities() -> &'static CodecCapabilities {
            &BMP_ENCODE_CAPS
        }

        fn job(&self) -> BmpEncodeJob<'_> {
            BmpEncodeJob {
                config: self,
                limits: None,
            }
        }
    }

    // ── BmpEncodeJob ─────────────────────────────────────────────────

    /// Per-operation BMP encode job.
    pub struct BmpEncodeJob<'a> {
        config: &'a BmpEncoderConfig,
        limits: Option<ResourceLimits>,
    }

    impl<'a> zencodec_types::EncodeJob<'a> for BmpEncodeJob<'a> {
        type Error = BitmapError;
        type Enc = BmpEncoder<'a>;
        type FrameEnc = BmpFrameEncoder;

        fn with_stop(self, _stop: &'a dyn Stop) -> Self {
            self
        }

        fn with_metadata(self, _meta: &'a MetadataView<'a>) -> Self {
            self
        }

        fn with_limits(mut self, limits: ResourceLimits) -> Self {
            self.limits = Some(limits);
            self
        }

        fn encoder(self) -> Result<BmpEncoder<'a>, BitmapError> {
            Ok(BmpEncoder {
                config: self.config,
                limits: self.limits,
            })
        }

        fn frame_encoder(self) -> Result<BmpFrameEncoder, BitmapError> {
            Err(BitmapError::UnsupportedVariant(
                "BMP does not support animation".into(),
            ))
        }
    }

    // ── BmpEncoder ───────────────────────────────────────────────────

    /// Single-image BMP encoder.
    pub struct BmpEncoder<'a> {
        config: &'a BmpEncoderConfig,
        limits: Option<ResourceLimits>,
    }

    impl BmpEncoder<'_> {
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

    impl zencodec_types::Encoder for BmpEncoder<'_> {
        type Error = BitmapError;

        fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, BitmapError> {
            let desc = pixels.descriptor();
            let w = pixels.width();
            let h = pixels.rows();

            if let Some(limits) = self.effective_limits() {
                limits.check(w, h)?;
            }

            let bytes = pixels.contiguous_bytes();
            let (layout, alpha) = match (desc.channel_type(), desc.layout()) {
                (ChannelType::U8, ChannelLayout::Rgb) => {
                    (crate::PixelLayout::Rgb8, false)
                }
                (ChannelType::U8, ChannelLayout::Rgba) => {
                    (crate::PixelLayout::Rgba8, true)
                }
                (ChannelType::U8, ChannelLayout::Bgra) => {
                    (crate::PixelLayout::Bgra8, true)
                }
                _ => {
                    return Err(BitmapError::UnsupportedVariant(alloc::format!(
                        "BMP encode: unsupported pixel format: {:?}",
                        desc
                    )));
                }
            };

            let encoded = crate::bmp::encode(&bytes, w, h, layout, alpha, &enough::Unstoppable)?;
            Ok(EncodeOutput::new(encoded, ImageFormat::Bmp))
        }
    }

    // ── BmpFrameEncoder (stub) ───────────────────────────────────────

    /// Stub frame encoder — BMP does not support animation.
    pub struct BmpFrameEncoder;

    impl zencodec_types::FrameEncoder for BmpFrameEncoder {
        type Error = BitmapError;

        fn push_frame(
            &mut self,
            _pixels: PixelSlice<'_>,
            _duration_ms: u32,
        ) -> Result<(), BitmapError> {
            Err(BitmapError::UnsupportedVariant(
                "BMP does not support animation".into(),
            ))
        }

        fn finish(self) -> Result<EncodeOutput, BitmapError> {
            Err(BitmapError::UnsupportedVariant(
                "BMP does not support animation".into(),
            ))
        }
    }

    // ── BmpDecoderConfig ─────────────────────────────────────────────

    /// Decoding configuration for BMP format.
    #[derive(Clone, Debug)]
    pub struct BmpDecoderConfig {
        limits: Option<Limits>,
    }

    impl Default for BmpDecoderConfig {
        fn default() -> Self {
            Self::new()
        }
    }

    impl BmpDecoderConfig {
        /// Create a new BMP decoder config with default settings.
        pub fn new() -> Self {
            Self { limits: None }
        }
    }

    impl zencodec_types::DecoderConfig for BmpDecoderConfig {
        type Error = BitmapError;
        type Job<'a> = BmpDecodeJob<'a>;

        fn format() -> ImageFormat {
            ImageFormat::Bmp
        }

        fn supported_descriptors() -> &'static [PixelDescriptor] {
            BMP_DECODE_DESCRIPTORS
        }

        fn capabilities() -> &'static CodecCapabilities {
            &BMP_DECODE_CAPS
        }

        fn job(&self) -> BmpDecodeJob<'_> {
            BmpDecodeJob {
                config: self,
                limits: None,
            }
        }
    }

    // ── BmpDecodeJob ─────────────────────────────────────────────────

    /// Per-operation BMP decode job.
    pub struct BmpDecodeJob<'a> {
        config: &'a BmpDecoderConfig,
        limits: Option<Limits>,
    }

    impl<'a> zencodec_types::DecodeJob<'a> for BmpDecodeJob<'a> {
        type Error = BitmapError;
        type Dec = BmpDecoder<'a>;
        type StreamDec = NoStreamingDecode;
        type FrameDec = BmpFrameDecoder;

        fn with_stop(self, _stop: &'a dyn Stop) -> Self {
            self
        }

        fn with_limits(mut self, limits: ResourceLimits) -> Self {
            self.limits = Some(convert_limits(&limits));
            self
        }

        fn probe(&self, data: &[u8]) -> Result<ImageInfo, BitmapError> {
            let header = crate::bmp::decode::parse_bmp_header(data)?;
            let has_alpha = matches!(
                header.layout,
                crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
            );
            Ok(ImageInfo::new(header.width, header.height, ImageFormat::Bmp).with_alpha(has_alpha))
        }

        fn output_info(&self, data: &[u8]) -> Result<OutputInfo, BitmapError> {
            let header = crate::bmp::decode::parse_bmp_header(data)?;
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

        fn decoder(
            self,
            data: &'a [u8],
            _preferred: &[PixelDescriptor],
        ) -> Result<BmpDecoder<'a>, BitmapError> {
            Ok(BmpDecoder {
                config: self.config,
                limits: self.limits,
                data,
            })
        }

        fn streaming_decoder(
            self,
            _data: &'a [u8],
            _preferred: &[PixelDescriptor],
        ) -> Result<NoStreamingDecode, BitmapError> {
            Err(BitmapError::UnsupportedVariant(
                "BMP does not support streaming decode".into(),
            ))
        }

        fn frame_decoder(
            self,
            _data: &'a [u8],
            _preferred: &[PixelDescriptor],
        ) -> Result<BmpFrameDecoder, BitmapError> {
            Err(BitmapError::UnsupportedVariant(
                "BMP does not support animation".into(),
            ))
        }
    }

    // ── BmpDecoder ───────────────────────────────────────────────────

    /// Single-image BMP decoder.
    pub struct BmpDecoder<'a> {
        config: &'a BmpDecoderConfig,
        limits: Option<Limits>,
        data: &'a [u8],
    }

    impl BmpDecoder<'_> {
        fn effective_limits(&self) -> Option<&Limits> {
            self.limits.as_ref().or(self.config.limits.as_ref())
        }
    }

    impl zencodec_types::Decode for BmpDecoder<'_> {
        type Error = BitmapError;

        fn decode(self) -> Result<DecodeOutput, BitmapError> {
            let limits = self.effective_limits();
            let decoded = crate::bmp::decode(self.data, limits, &enough::Unstoppable)?;
            decode_output_from_internal(&decoded, ImageFormat::Bmp)
        }
    }

    // ── BmpFrameDecoder (stub) ───────────────────────────────────────

    /// Stub frame decoder — BMP does not support animation.
    pub struct BmpFrameDecoder;

    impl zencodec_types::FrameDecode for BmpFrameDecoder {
        type Error = BitmapError;

        fn next_frame(&mut self) -> Result<Option<DecodeFrame>, BitmapError> {
            Err(BitmapError::UnsupportedVariant(
                "BMP does not support animation".into(),
            ))
        }
    }
}

#[cfg(feature = "bmp")]
pub use bmp_codec::*;

// ══════════════════════════════════════════════════════════════════════
// Farbfeld codec
// ══════════════════════════════════════════════════════════════════════

// ── FarbfeldEncoderConfig ────────────────────────────────────────────

/// Encoding configuration for farbfeld format.
///
/// Accepts Rgba16 (direct), Rgba8 (expand), Rgb8 (expand + alpha), Gray8 (expand).
#[derive(Clone, Debug)]
pub struct FarbfeldEncoderConfig {
    limits: ResourceLimits,
}

impl Default for FarbfeldEncoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl FarbfeldEncoderConfig {
    /// Create a new farbfeld encoder config with default settings.
    pub fn new() -> Self {
        Self {
            limits: ResourceLimits::none(),
        }
    }
}

impl zencodec_types::EncoderConfig for FarbfeldEncoderConfig {
    type Error = BitmapError;
    type Job<'a> = FarbfeldEncodeJob<'a>;

    fn format() -> ImageFormat {
        ImageFormat::Farbfeld
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        FF_ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static CodecCapabilities {
        &FF_ENCODE_CAPS
    }

    fn job(&self) -> FarbfeldEncodeJob<'_> {
        FarbfeldEncodeJob {
            config: self,
            limits: None,
        }
    }
}

// ── FarbfeldEncodeJob ────────────────────────────────────────────────

/// Per-operation farbfeld encode job.
pub struct FarbfeldEncodeJob<'a> {
    config: &'a FarbfeldEncoderConfig,
    limits: Option<ResourceLimits>,
}

impl<'a> zencodec_types::EncodeJob<'a> for FarbfeldEncodeJob<'a> {
    type Error = BitmapError;
    type Enc = FarbfeldEncoder<'a>;
    type FrameEnc = FarbfeldFrameEncoder;

    fn with_stop(self, _stop: &'a dyn Stop) -> Self {
        self
    }

    fn with_metadata(self, _meta: &'a MetadataView<'a>) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(limits);
        self
    }

    fn encoder(self) -> Result<FarbfeldEncoder<'a>, BitmapError> {
        Ok(FarbfeldEncoder {
            config: self.config,
            limits: self.limits,
        })
    }

    fn frame_encoder(self) -> Result<FarbfeldFrameEncoder, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "farbfeld does not support animation".into(),
        ))
    }
}

// ── FarbfeldEncoder ──────────────────────────────────────────────────

/// Single-image farbfeld encoder.
pub struct FarbfeldEncoder<'a> {
    config: &'a FarbfeldEncoderConfig,
    limits: Option<ResourceLimits>,
}

impl FarbfeldEncoder<'_> {
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

impl zencodec_types::Encoder for FarbfeldEncoder<'_> {
    type Error = BitmapError;

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, BitmapError> {
        let desc = pixels.descriptor();
        let w = pixels.width();
        let h = pixels.rows();

        if let Some(limits) = self.effective_limits() {
            limits.check(w, h)?;
        }

        let bytes = pixels.contiguous_bytes();
        let layout = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U16, ChannelLayout::Rgba) => {
                crate::PixelLayout::Rgba16
            }
            (ChannelType::U8, ChannelLayout::Rgba) => {
                crate::PixelLayout::Rgba8
            }
            (ChannelType::U8, ChannelLayout::Rgb) => {
                crate::PixelLayout::Rgb8
            }
            (ChannelType::U8, ChannelLayout::Gray) => {
                crate::PixelLayout::Gray8
            }
            _ => {
                return Err(BitmapError::UnsupportedVariant(alloc::format!(
                    "farbfeld encode: unsupported pixel format: {:?}",
                    desc
                )));
            }
        };

        let encoded = crate::farbfeld::encode(&bytes, w, h, layout, &enough::Unstoppable)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Farbfeld))
    }
}

// ── FarbfeldFrameEncoder (stub) ──────────────────────────────────────

/// Stub frame encoder — farbfeld does not support animation.
pub struct FarbfeldFrameEncoder;

impl zencodec_types::FrameEncoder for FarbfeldFrameEncoder {
    type Error = BitmapError;

    fn push_frame(
        &mut self,
        _pixels: PixelSlice<'_>,
        _duration_ms: u32,
    ) -> Result<(), BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "farbfeld does not support animation".into(),
        ))
    }

    fn finish(self) -> Result<EncodeOutput, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "farbfeld does not support animation".into(),
        ))
    }
}

// ── FarbfeldDecoderConfig ────────────────────────────────────────────

/// Decoding configuration for farbfeld format.
#[derive(Clone, Debug)]
pub struct FarbfeldDecoderConfig {
    limits: Option<Limits>,
}

impl Default for FarbfeldDecoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl FarbfeldDecoderConfig {
    /// Create a new farbfeld decoder config with default settings.
    pub fn new() -> Self {
        Self { limits: None }
    }
}

impl zencodec_types::DecoderConfig for FarbfeldDecoderConfig {
    type Error = BitmapError;
    type Job<'a> = FarbfeldDecodeJob<'a>;

    fn format() -> ImageFormat {
        ImageFormat::Farbfeld
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        FF_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static CodecCapabilities {
        &FF_DECODE_CAPS
    }

    fn job(&self) -> FarbfeldDecodeJob<'_> {
        FarbfeldDecodeJob {
            config: self,
            limits: None,
        }
    }
}

// ── FarbfeldDecodeJob ────────────────────────────────────────────────

/// Per-operation farbfeld decode job.
pub struct FarbfeldDecodeJob<'a> {
    config: &'a FarbfeldDecoderConfig,
    limits: Option<Limits>,
}

impl<'a> zencodec_types::DecodeJob<'a> for FarbfeldDecodeJob<'a> {
    type Error = BitmapError;
    type Dec = FarbfeldDecoder<'a>;
    type StreamDec = NoStreamingDecode;
    type FrameDec = FarbfeldFrameDecoder;

    fn with_stop(self, _stop: &'a dyn Stop) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(convert_limits(&limits));
        self
    }

    fn probe(&self, data: &[u8]) -> Result<ImageInfo, BitmapError> {
        let (width, height) = crate::farbfeld::decode::parse_header(data)?;
        Ok(ImageInfo::new(width, height, ImageFormat::Farbfeld).with_alpha(true))
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, BitmapError> {
        let (width, height) = crate::farbfeld::decode::parse_header(data)?;
        Ok(OutputInfo::full_decode(width, height, PixelDescriptor::RGBA16_SRGB).with_alpha(true))
    }

    fn decoder(
        self,
        data: &'a [u8],
        _preferred: &[PixelDescriptor],
    ) -> Result<FarbfeldDecoder<'a>, BitmapError> {
        Ok(FarbfeldDecoder {
            config: self.config,
            limits: self.limits,
            data,
        })
    }

    fn streaming_decoder(
        self,
        _data: &'a [u8],
        _preferred: &[PixelDescriptor],
    ) -> Result<NoStreamingDecode, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "farbfeld does not support streaming decode".into(),
        ))
    }

    fn frame_decoder(
        self,
        _data: &'a [u8],
        _preferred: &[PixelDescriptor],
    ) -> Result<FarbfeldFrameDecoder, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "farbfeld does not support animation".into(),
        ))
    }
}

// ── FarbfeldDecoder ──────────────────────────────────────────────────

/// Single-image farbfeld decoder.
pub struct FarbfeldDecoder<'a> {
    config: &'a FarbfeldDecoderConfig,
    limits: Option<Limits>,
    data: &'a [u8],
}

impl FarbfeldDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec_types::Decode for FarbfeldDecoder<'_> {
    type Error = BitmapError;

    fn decode(self) -> Result<DecodeOutput, BitmapError> {
        let limits = self.effective_limits();
        let decoded = crate::farbfeld::decode(self.data, limits, &enough::Unstoppable)?;
        decode_output_from_internal(&decoded, ImageFormat::Farbfeld)
    }
}

// ── FarbfeldFrameDecoder (stub) ──────────────────────────────────────

/// Stub frame decoder — farbfeld does not support animation.
pub struct FarbfeldFrameDecoder;

impl zencodec_types::FrameDecode for FarbfeldFrameDecoder {
    type Error = BitmapError;

    fn next_frame(&mut self) -> Result<Option<DecodeFrame>, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "farbfeld does not support animation".into(),
        ))
    }
}

// ══════════════════════════════════════════════════════════════════════
// Streaming decode stub
// ══════════════════════════════════════════════════════════════════════

/// Unit struct for codecs that do not support streaming decode.
///
/// `streaming_decoder()` always returns `Err`, so these methods are
/// unreachable in practice.
pub struct NoStreamingDecode;

impl zencodec_types::StreamingDecode for NoStreamingDecode {
    type Error = BitmapError;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, BitmapError> {
        Err(BitmapError::UnsupportedVariant(
            "streaming decode not supported".into(),
        ))
    }

    fn info(&self) -> &ImageInfo {
        panic!("streaming decode not supported — streaming_decoder() returns Err")
    }
}

// ══════════════════════════════════════════════════════════════════════
// Shared helpers
// ══════════════════════════════════════════════════════════════════════

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

fn layout_to_pixel_buffer(
    decoded: &crate::decode::DecodeOutput<'_>,
) -> Result<PixelBuffer, BitmapError> {
    use crate::PixelLayout;
    use rgb::AsPixels as _;

    let w = decoded.width as usize;
    let h = decoded.height as usize;
    let bytes = decoded.pixels();

    match decoded.layout {
        PixelLayout::Gray8 => {
            let pixels: &[rgb::Gray<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::Gray16 => {
            let pixels: Vec<rgb::Gray<u16>> = bytes
                .chunks_exact(2)
                .map(|c| rgb::Gray::new(u16::from_ne_bytes([c[0], c[1]])))
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::Rgb8 => {
            let pixels: &[rgb::Rgb<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::Rgba8 => {
            let pixels: &[rgb::Rgba<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::GrayF32 => {
            let pixels: Vec<rgb::Gray<f32>> = bytes
                .chunks_exact(4)
                .map(|c| rgb::Gray::new(f32::from_ne_bytes([c[0], c[1], c[2], c[3]])))
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::RgbF32 => {
            // RgbF32 → promote to RgbaF32 (PFM has no alpha concept)
            let pixels: Vec<rgb::Rgba<f32>> = bytes
                .chunks_exact(12)
                .map(|c| {
                    let r = f32::from_ne_bytes([c[0], c[1], c[2], c[3]]);
                    let g = f32::from_ne_bytes([c[4], c[5], c[6], c[7]]);
                    let b = f32::from_ne_bytes([c[8], c[9], c[10], c[11]]);
                    rgb::Rgba { r, g, b, a: 1.0 }
                })
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::Bgr8 => {
            // BGR → convert to RGB
            let pixels: Vec<rgb::Rgb<u8>> = bytes
                .chunks_exact(3)
                .map(|c| rgb::Rgb {
                    r: c[2],
                    g: c[1],
                    b: c[0],
                })
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::Bgra8 => {
            let pixels: &[rgb::alt::BGRA<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::Bgrx8 => {
            let pixels: &[rgb::alt::BGRA<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::Rgba16 => {
            let pixels: Vec<rgb::Rgba<u16>> = bytes
                .chunks_exact(8)
                .map(|c| rgb::Rgba {
                    r: u16::from_ne_bytes([c[0], c[1]]),
                    g: u16::from_ne_bytes([c[2], c[3]]),
                    b: u16::from_ne_bytes([c[4], c[5]]),
                    a: u16::from_ne_bytes([c[6], c[7]]),
                })
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
    }
}

/// Build a zencodec DecodeOutput from an internal DecodeOutput.
fn decode_output_from_internal(
    decoded: &crate::decode::DecodeOutput<'_>,
    format: ImageFormat,
) -> Result<DecodeOutput, BitmapError> {
    let has_alpha = matches!(
        decoded.layout,
        crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
    );
    let info = ImageInfo::new(decoded.width, decoded.height, format).with_alpha(has_alpha);
    let pixels = layout_to_pixel_buffer(decoded)?;
    Ok(DecodeOutput::new(pixels, info))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use zencodec_types::{Decode, DecodeJob, DecoderConfig, EncodeJob, Encoder, EncoderConfig};

    /// Helper: encode via the four-layer flow (type-erased).
    fn encode_pixels(slice: PixelSlice<'_>) -> EncodeOutput {
        let config = PnmEncoderConfig::new();
        config.job().encoder().unwrap().encode(slice).unwrap()
    }

    /// Helper: decode via the four-layer flow.
    fn decode_bytes(data: &[u8]) -> DecodeOutput {
        let config = PnmDecoderConfig::new();
        config
            .job()
            .decoder(data, &[])
            .unwrap()
            .decode()
            .unwrap()
    }

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
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());
        assert_eq!(output.format(), ImageFormat::Pnm);

        let decoded = decode_bytes(output.data());
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
        let rgb_img = decoded.into_rgb8();
        assert_eq!(rgb_img.as_imgref().buf(), &pixels);
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
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        let gray_img = decoded.into_gray8();
        assert_eq!(gray_img.as_imgref().buf(), &pixels);
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
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        assert!(decoded.has_alpha());
        let rgba_img = decoded.into_rgba8();
        assert_eq!(rgba_img.as_imgref().buf(), &pixels);
    }

    #[test]
    fn encode_bgra8_no_double_swizzle() {
        // BGRA encode should go directly to PPM via zenbitmaps's native BGRA→RGB
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
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        let rgb_img = decoded.into_rgb8();
        let rgb_ref = rgb_img.as_imgref();
        let buf = rgb_ref.buf();
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
    fn probe_extracts_info() {
        let pixels = vec![rgb::Rgb::<u8> { r: 1, g: 2, b: 3 }; 6];
        let img = imgref::ImgVec::new(pixels, 3, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let dec = PnmDecoderConfig::new();
        let info = dec.job().probe(output.data()).unwrap();
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 2);
        assert_eq!(info.format, ImageFormat::Pnm);
        assert!(!info.has_alpha);
    }

    #[test]
    fn capabilities_are_correct() {
        let enc_caps = PnmEncoderConfig::capabilities();
        assert!(enc_caps.native_gray());
        assert!(!enc_caps.cheap_probe());
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

        let big_pixels = vec![rgb::Rgb::<u8> { r: 0, g: 0, b: 0 }; 100 * 100];
        let img = imgref::ImgVec::new(big_pixels, 100, 100);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let dec = PnmDecoderConfig::new();
        let result = dec
            .job()
            .with_limits(limits)
            .decoder(output.data(), &[])
            .unwrap()
            .decode();
        assert!(result.is_err());
    }

    #[test]
    fn decode_to_bgra8_from_rgb() {
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
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        let bgra_img = decoded.to_bgra8();
        let bgra_ref = bgra_img.as_imgref();
        let buf = bgra_ref.buf();
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
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());
        assert_eq!(output.format(), ImageFormat::Pnm);

        let decoded = decode_bytes(output.data());
        // RgbF32 gets promoted to RgbaF32 in the decode path
        let rgba_img = decoded.into_rgba8();
        // The data was f32 -> stored as PFM -> decoded as RgbaF32.
        // We just check it decoded without error; pixel values
        // already verified by the underlying pnm roundtrip.
        assert_eq!(rgba_img.width(), 2);
        assert_eq!(rgba_img.height(), 2);
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
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        // GrayF32 decoded as PixelBuffer -- verify dimensions
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
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
    fn output_info_matches_decode() {
        let pixels = vec![rgb::Rgb::<u8> { r: 1, g: 2, b: 3 }; 6];
        let img = imgref::ImgVec::new(pixels, 3, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let dec = PnmDecoderConfig::new();
        let info = dec.job().output_info(output.data()).unwrap();
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 2);

        let decoded = decode_bytes(output.data());
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

        let slice = PixelSlice::from(img.as_ref()).erase();
        let output = config.job().encoder().unwrap().encode(slice).unwrap();
        assert_eq!(output.format(), ImageFormat::Pnm);
        assert!(!output.data().is_empty());
    }

    #[test]
    fn four_layer_decode_flow() {
        let pixels = vec![
            rgb::Rgb::<u8> {
                r: 100,
                g: 200,
                b: 50
            };
            4
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let config = PnmDecoderConfig::new();
        let decoded = config
            .job()
            .decoder(output.data(), &[])
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
    }
}
