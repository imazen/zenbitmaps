//! zencodec trait implementations for zenbitmaps.
//!
//! Provides per-format codec pairs:
//! - PNM: PnmEncoderConfig / PnmDecoderConfig (always available)
//! - BMP: BmpEncoderConfig / BmpDecoderConfig (requires `bmp` feature)
//! - Farbfeld: FarbfeldEncoderConfig / FarbfeldDecoderConfig (always available)

use alloc::borrow::Cow;
use alloc::string::ToString as _;
use alloc::vec::Vec;
use enough::Stop;
use zencodec::decode::{DecodeCapabilities, DecodeOutput, OutputInfo};
use zencodec::encode::{EncodeCapabilities, EncodeOutput};
use zencodec::{ImageFormat, ImageInfo, Metadata, ResourceLimits};
use zenpixels::{ChannelLayout, ChannelType, PixelBuffer, PixelDescriptor, PixelSlice};

use crate::error::BitmapError;
use crate::limits::Limits;
use crate::pnm;

// ══════════════════════════════════════════════════════════════════════
// Shared capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static PNM_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_native_gray(true)
    .with_native_alpha(true)
    .with_native_f32(true)
    .with_hdr(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

static PNM_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_native_gray(true)
    .with_native_alpha(true)
    .with_native_16bit(true)
    .with_native_f32(true)
    .with_hdr(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

// Note: U16 encode is not implemented — RGBA16_SRGB intentionally absent.
static PNM_ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
    PixelDescriptor::RGBF32_LINEAR,
    PixelDescriptor::RGBAF32_LINEAR,
    PixelDescriptor::GRAYF32_LINEAR,
];

// Note: RgbF32 is promoted to RgbaF32 in decode, so RGBF32_LINEAR is absent.
// Note: RGBA16_SRGB is absent because the PNM decoder downscales non-gray
// 16-bit to 8-bit (only Gray16 is preserved at 16-bit).
static PNM_DECODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::GRAY16_SRGB,
    PixelDescriptor::BGRA8_SRGB,
    PixelDescriptor::RGBAF32_LINEAR,
    PixelDescriptor::GRAYF32_LINEAR,
];

#[cfg(feature = "bmp")]
static BMP_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_native_alpha(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

#[cfg(feature = "bmp")]
static BMP_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_native_gray(true)
    .with_native_alpha(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

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
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
];

static FF_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_native_alpha(true)
    .with_native_16bit(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

static FF_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_native_alpha(true)
    .with_native_16bit(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

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
/// Implements [`zencodec::encode::EncoderConfig`] for the PNM family.
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

impl zencodec::encode::EncoderConfig for PnmEncoderConfig {
    type Error = BitmapError;
    type Job = PnmEncodeJob;

    fn format() -> ImageFormat {
        ImageFormat::Pnm
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        PNM_ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static EncodeCapabilities {
        &PNM_ENCODE_CAPS
    }

    fn is_lossless(&self) -> Option<bool> {
        Some(true)
    }

    fn job(self) -> PnmEncodeJob {
        PnmEncodeJob {
            config: self,
            limits: None,
            stop: None,
        }
    }
}

// ── PnmEncodeJob ─────────────────────────────────────────────────────

/// Per-operation PNM encode job.
pub struct PnmEncodeJob {
    config: PnmEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
}

impl zencodec::encode::EncodeJob for PnmEncodeJob {
    type Error = BitmapError;
    type Enc = PnmEncoder;
    type FullFrameEnc = ();

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_metadata(self, _meta: &Metadata) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(limits);
        self
    }

    fn encoder(self) -> Result<PnmEncoder, BitmapError> {
        Ok(PnmEncoder {
            config: self.config,
            limits: self.limits,
            stop: self.stop,
        })
    }

    fn full_frame_encoder(self) -> Result<(), BitmapError> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationEncode))
    }
}

// ── PnmEncoder ───────────────────────────────────────────────────────

/// Single-image PNM encoder.
pub struct PnmEncoder {
    config: PnmEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
}

impl PnmEncoder {
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

impl zencodec::encode::Encoder for PnmEncoder {
    type Error = BitmapError;

    fn reject(op: zencodec::UnsupportedOperation) -> BitmapError {
        BitmapError::from(op)
    }

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, BitmapError> {
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
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
                    stop,
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
                    stop,
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
                    stop,
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
                    stop,
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
                    stop,
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
                    stop,
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
                    stop,
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

// ── PnmDecoderConfig ─────────────────────────────────────────────────

/// Decoding configuration for PNM formats.
///
/// Implements [`zencodec::decode::DecoderConfig`] for the PNM family.
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

impl zencodec::decode::DecoderConfig for PnmDecoderConfig {
    type Error = BitmapError;
    type Job<'a> = PnmDecodeJob<'a>;

    fn formats() -> &'static [ImageFormat] {
        &[ImageFormat::Pnm]
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        PNM_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static DecodeCapabilities {
        &PNM_DECODE_CAPS
    }

    fn job(&self) -> PnmDecodeJob<'_> {
        PnmDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
        }
    }
}

// ── PnmDecodeJob ─────────────────────────────────────────────────────

/// Per-operation PNM decode job.
pub struct PnmDecodeJob<'a> {
    config: &'a PnmDecoderConfig,
    limits: Option<Limits>,
    stop: Option<zencodec::StopToken>,
    max_input_bytes: Option<u64>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for PnmDecodeJob<'a> {
    type Error = BitmapError;
    type Dec = PnmDecoder<'a>;
    type StreamDec = zencodec::Unsupported<BitmapError>;
    type FullFrameDec = zencodec::Unsupported<BitmapError>;

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.max_input_bytes = limits.max_input_bytes;
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
            crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8 | crate::PixelLayout::Rgba16
        );
        let native_format = layout_to_descriptor(header.layout);
        Ok(
            OutputInfo::full_decode(header.width, header.height, native_format)
                .with_alpha(has_alpha),
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<PnmDecoder<'a>, BitmapError> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            )));
        }
        Ok(PnmDecoder {
            config: self.config,
            limits: self.limits,
            data,
            stop: self.stop,
        })
    }

    fn push_decoder(
        self,
        data: Cow<'a, [u8]>,
        sink: &mut dyn zencodec::decode::DecodeRowSink,
        preferred: &[PixelDescriptor],
    ) -> Result<OutputInfo, Self::Error> {
        zencodec::helpers::copy_decode_to_sink(self, data, sink, preferred, |e| {
            BitmapError::InvalidData(e.to_string())
        })
    }

    fn streaming_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<zencodec::Unsupported<BitmapError>, BitmapError> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::RowLevelDecode))
    }

    fn full_frame_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<zencodec::Unsupported<BitmapError>, BitmapError> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationDecode))
    }
}

// ── PnmDecoder ───────────────────────────────────────────────────────

/// Single-image PNM decoder.
pub struct PnmDecoder<'a> {
    config: &'a PnmDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    stop: Option<zencodec::StopToken>,
}

impl PnmDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for PnmDecoder<'_> {
    type Error = BitmapError;

    fn decode(self) -> Result<DecodeOutput, BitmapError> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::pnm::decode(&self.data, limits, stop)?;
        decode_output_from_internal(&decoded, ImageFormat::Pnm)
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

    impl zencodec::encode::EncoderConfig for BmpEncoderConfig {
        type Error = BitmapError;
        type Job = BmpEncodeJob;

        fn format() -> ImageFormat {
            ImageFormat::Bmp
        }

        fn supported_descriptors() -> &'static [PixelDescriptor] {
            BMP_ENCODE_DESCRIPTORS
        }

        fn capabilities() -> &'static EncodeCapabilities {
            &BMP_ENCODE_CAPS
        }

        fn is_lossless(&self) -> Option<bool> {
            Some(true)
        }

        fn job(self) -> BmpEncodeJob {
            BmpEncodeJob {
                config: self,
                limits: None,
                stop: None,
            }
        }
    }

    // ── BmpEncodeJob ─────────────────────────────────────────────────

    /// Per-operation BMP encode job.
    pub struct BmpEncodeJob {
        config: BmpEncoderConfig,
        limits: Option<ResourceLimits>,
        stop: Option<zencodec::StopToken>,
    }

    impl zencodec::encode::EncodeJob for BmpEncodeJob {
        type Error = BitmapError;
        type Enc = BmpEncoder;
        type FullFrameEnc = ();

        fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
            self.stop = Some(stop);
            self
        }

        fn with_metadata(self, _meta: &Metadata) -> Self {
            self
        }

        fn with_limits(mut self, limits: ResourceLimits) -> Self {
            self.limits = Some(limits);
            self
        }

        fn encoder(self) -> Result<BmpEncoder, BitmapError> {
            Ok(BmpEncoder {
                config: self.config,
                limits: self.limits,
                stop: self.stop,
            })
        }

        fn full_frame_encoder(self) -> Result<(), BitmapError> {
            Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationEncode))
        }
    }

    // ── BmpEncoder ───────────────────────────────────────────────────

    /// Single-image BMP encoder.
    pub struct BmpEncoder {
        config: BmpEncoderConfig,
        limits: Option<ResourceLimits>,
        stop: Option<zencodec::StopToken>,
    }

    impl BmpEncoder {
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

    impl zencodec::encode::Encoder for BmpEncoder {
        type Error = BitmapError;

        fn reject(op: zencodec::UnsupportedOperation) -> BitmapError {
            BitmapError::from(op)
        }

        fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, BitmapError> {
            let stop: &dyn Stop = match &self.stop {
                Some(s) => s,
                None => &enough::Unstoppable,
            };
            let desc = pixels.descriptor();
            let w = pixels.width();
            let h = pixels.rows();

            if let Some(limits) = self.effective_limits() {
                limits.check(w, h)?;
            }

            let bytes = pixels.contiguous_bytes();
            let (layout, alpha) = match (desc.channel_type(), desc.layout()) {
                (ChannelType::U8, ChannelLayout::Rgb) => (crate::PixelLayout::Rgb8, false),
                (ChannelType::U8, ChannelLayout::Rgba) => (crate::PixelLayout::Rgba8, true),
                (ChannelType::U8, ChannelLayout::Bgra) => (crate::PixelLayout::Bgra8, true),
                _ => {
                    return Err(BitmapError::UnsupportedVariant(alloc::format!(
                        "BMP encode: unsupported pixel format: {:?}",
                        desc
                    )));
                }
            };

            let encoded = crate::bmp::encode(&bytes, w, h, layout, alpha, stop)?;
            Ok(EncodeOutput::new(encoded, ImageFormat::Bmp))
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

    impl zencodec::decode::DecoderConfig for BmpDecoderConfig {
        type Error = BitmapError;
        type Job<'a> = BmpDecodeJob<'a>;

        fn formats() -> &'static [ImageFormat] {
            &[ImageFormat::Bmp]
        }

        fn supported_descriptors() -> &'static [PixelDescriptor] {
            BMP_DECODE_DESCRIPTORS
        }

        fn capabilities() -> &'static DecodeCapabilities {
            &BMP_DECODE_CAPS
        }

        fn job(&self) -> BmpDecodeJob<'_> {
            BmpDecodeJob {
                config: self,
                limits: None,
                stop: None,
                max_input_bytes: None,
            }
        }
    }

    // ── BmpDecodeJob ─────────────────────────────────────────────────

    /// Per-operation BMP decode job.
    pub struct BmpDecodeJob<'a> {
        config: &'a BmpDecoderConfig,
        limits: Option<Limits>,
        stop: Option<zencodec::StopToken>,
        max_input_bytes: Option<u64>,
    }

    impl<'a> zencodec::decode::DecodeJob<'a> for BmpDecodeJob<'a> {
        type Error = BitmapError;
        type Dec = BmpDecoder<'a>;
        type StreamDec = zencodec::Unsupported<BitmapError>;
        type FullFrameDec = zencodec::Unsupported<BitmapError>;

        fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
            self.stop = Some(stop);
            self
        }

        fn with_limits(mut self, limits: ResourceLimits) -> Self {
            self.max_input_bytes = limits.max_input_bytes;
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
            data: Cow<'a, [u8]>,
            _preferred: &[PixelDescriptor],
        ) -> Result<BmpDecoder<'a>, BitmapError> {
            if let Some(max) = self.max_input_bytes
                && data.len() as u64 > max
            {
                return Err(BitmapError::LimitExceeded(alloc::format!(
                    "input size {} exceeds limit {max}",
                    data.len()
                )));
            }
            Ok(BmpDecoder {
                config: self.config,
                limits: self.limits,
                data,
                stop: self.stop,
            })
        }

        fn push_decoder(
            self,
            data: Cow<'a, [u8]>,
            sink: &mut dyn zencodec::decode::DecodeRowSink,
            preferred: &[PixelDescriptor],
        ) -> Result<OutputInfo, Self::Error> {
            zencodec::helpers::copy_decode_to_sink(self, data, sink, preferred, |e| {
                BitmapError::InvalidData(e.to_string())
            })
        }

        fn streaming_decoder(
            self,
            _data: Cow<'a, [u8]>,
            _preferred: &[PixelDescriptor],
        ) -> Result<zencodec::Unsupported<BitmapError>, BitmapError> {
            Err(BitmapError::from(zencodec::UnsupportedOperation::RowLevelDecode))
        }

        fn full_frame_decoder(
            self,
            _data: Cow<'a, [u8]>,
            _preferred: &[PixelDescriptor],
        ) -> Result<zencodec::Unsupported<BitmapError>, BitmapError> {
            Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationDecode))
        }
    }

    // ── BmpDecoder ───────────────────────────────────────────────────

    /// Single-image BMP decoder.
    pub struct BmpDecoder<'a> {
        config: &'a BmpDecoderConfig,
        limits: Option<Limits>,
        data: Cow<'a, [u8]>,
        stop: Option<zencodec::StopToken>,
    }

    impl BmpDecoder<'_> {
        fn effective_limits(&self) -> Option<&Limits> {
            self.limits.as_ref().or(self.config.limits.as_ref())
        }
    }

    impl zencodec::decode::Decode for BmpDecoder<'_> {
        type Error = BitmapError;

        fn decode(self) -> Result<DecodeOutput, BitmapError> {
            let limits = self.effective_limits();
            let stop: &dyn Stop = match &self.stop {
                Some(s) => s,
                None => &enough::Unstoppable,
            };
            let decoded = crate::bmp::decode(&self.data, limits, stop)?;
            decode_output_from_internal(&decoded, ImageFormat::Bmp)
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

impl zencodec::encode::EncoderConfig for FarbfeldEncoderConfig {
    type Error = BitmapError;
    type Job = FarbfeldEncodeJob;

    fn format() -> ImageFormat {
        ImageFormat::Farbfeld
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        FF_ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static EncodeCapabilities {
        &FF_ENCODE_CAPS
    }

    fn is_lossless(&self) -> Option<bool> {
        Some(true)
    }

    fn job(self) -> FarbfeldEncodeJob {
        FarbfeldEncodeJob {
            config: self,
            limits: None,
            stop: None,
        }
    }
}

// ── FarbfeldEncodeJob ────────────────────────────────────────────────

/// Per-operation farbfeld encode job.
pub struct FarbfeldEncodeJob {
    config: FarbfeldEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
}

impl zencodec::encode::EncodeJob for FarbfeldEncodeJob {
    type Error = BitmapError;
    type Enc = FarbfeldEncoder;
    type FullFrameEnc = ();

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_metadata(self, _meta: &Metadata) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(limits);
        self
    }

    fn encoder(self) -> Result<FarbfeldEncoder, BitmapError> {
        Ok(FarbfeldEncoder {
            config: self.config,
            limits: self.limits,
            stop: self.stop,
        })
    }

    fn full_frame_encoder(self) -> Result<(), BitmapError> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationEncode))
    }
}

// ── FarbfeldEncoder ──────────────────────────────────────────────────

/// Single-image farbfeld encoder.
pub struct FarbfeldEncoder {
    config: FarbfeldEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
}

impl FarbfeldEncoder {
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

impl zencodec::encode::Encoder for FarbfeldEncoder {
    type Error = BitmapError;

    fn reject(op: zencodec::UnsupportedOperation) -> BitmapError {
        BitmapError::from(op)
    }

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, BitmapError> {
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let desc = pixels.descriptor();
        let w = pixels.width();
        let h = pixels.rows();

        if let Some(limits) = self.effective_limits() {
            limits.check(w, h)?;
        }

        let bytes = pixels.contiguous_bytes();
        let layout = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U16, ChannelLayout::Rgba) => crate::PixelLayout::Rgba16,
            (ChannelType::U8, ChannelLayout::Rgba) => crate::PixelLayout::Rgba8,
            (ChannelType::U8, ChannelLayout::Rgb) => crate::PixelLayout::Rgb8,
            (ChannelType::U8, ChannelLayout::Gray) => crate::PixelLayout::Gray8,
            _ => {
                return Err(BitmapError::UnsupportedVariant(alloc::format!(
                    "farbfeld encode: unsupported pixel format: {:?}",
                    desc
                )));
            }
        };

        let encoded = crate::farbfeld::encode(&bytes, w, h, layout, stop)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Farbfeld))
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

impl zencodec::decode::DecoderConfig for FarbfeldDecoderConfig {
    type Error = BitmapError;
    type Job<'a> = FarbfeldDecodeJob<'a>;

    fn formats() -> &'static [ImageFormat] {
        &[ImageFormat::Farbfeld]
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        FF_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static DecodeCapabilities {
        &FF_DECODE_CAPS
    }

    fn job(&self) -> FarbfeldDecodeJob<'_> {
        FarbfeldDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
        }
    }
}

// ── FarbfeldDecodeJob ────────────────────────────────────────────────

/// Per-operation farbfeld decode job.
pub struct FarbfeldDecodeJob<'a> {
    config: &'a FarbfeldDecoderConfig,
    limits: Option<Limits>,
    stop: Option<zencodec::StopToken>,
    max_input_bytes: Option<u64>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for FarbfeldDecodeJob<'a> {
    type Error = BitmapError;
    type Dec = FarbfeldDecoder<'a>;
    type StreamDec = zencodec::Unsupported<BitmapError>;
    type FullFrameDec = zencodec::Unsupported<BitmapError>;

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.max_input_bytes = limits.max_input_bytes;
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
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<FarbfeldDecoder<'a>, BitmapError> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            )));
        }
        Ok(FarbfeldDecoder {
            config: self.config,
            limits: self.limits,
            data,
            stop: self.stop,
        })
    }

    fn push_decoder(
        self,
        data: Cow<'a, [u8]>,
        sink: &mut dyn zencodec::decode::DecodeRowSink,
        preferred: &[PixelDescriptor],
    ) -> Result<OutputInfo, Self::Error> {
        zencodec::helpers::copy_decode_to_sink(self, data, sink, preferred, |e| {
            BitmapError::InvalidData(e.to_string())
        })
    }

    fn streaming_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<zencodec::Unsupported<BitmapError>, BitmapError> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::RowLevelDecode))
    }

    fn full_frame_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<zencodec::Unsupported<BitmapError>, BitmapError> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationDecode))
    }
}

// ── FarbfeldDecoder ──────────────────────────────────────────────────

/// Single-image farbfeld decoder.
pub struct FarbfeldDecoder<'a> {
    config: &'a FarbfeldDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    stop: Option<zencodec::StopToken>,
}

impl FarbfeldDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for FarbfeldDecoder<'_> {
    type Error = BitmapError;

    fn decode(self) -> Result<DecodeOutput, BitmapError> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::farbfeld::decode(&self.data, limits, stop)?;
        decode_output_from_internal(&decoded, ImageFormat::Farbfeld)
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
    let has_alpha = matches!(
        header.layout,
        PixelLayout::Rgba8 | PixelLayout::Bgra8 | PixelLayout::Rgba16
    );
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
        PixelLayout::RgbF32 => PixelDescriptor::RGBAF32_LINEAR,
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
            // BGRX → convert to RGB (strip padding byte, swizzle BGR→RGB)
            let pixels: Vec<rgb::Rgb<u8>> = bytes
                .chunks_exact(4)
                .map(|c| rgb::Rgb {
                    r: c[2],
                    g: c[1],
                    b: c[0],
                })
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
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
        crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8 | crate::PixelLayout::Rgba16
    );
    let info = ImageInfo::new(decoded.width, decoded.height, format).with_alpha(has_alpha);
    let pixels = layout_to_pixel_buffer(decoded)?;
    Ok(DecodeOutput::new(pixels, info))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
    use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

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
            .decoder(Cow::Borrowed(data), &[])
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
        let buf = decoded.into_buffer();
        let rgb_img = buf.try_as_imgref::<rgb::Rgb<u8>>().unwrap();
        assert_eq!(rgb_img.buf(), &pixels);
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
        let buf = decoded.into_buffer();
        let gray_img = buf.try_as_imgref::<rgb::Gray<u8>>().unwrap();
        assert_eq!(gray_img.buf(), &pixels);
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
        let buf = decoded.into_buffer();
        let rgba_img = buf.try_as_imgref::<rgb::Rgba<u8>>().unwrap();
        assert_eq!(rgba_img.buf(), &pixels);
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
        let buf = decoded.into_buffer();
        let rgb_img = buf.try_as_imgref::<rgb::Rgb<u8>>().unwrap();
        let rgb_buf = rgb_img.buf();
        assert_eq!(rgb_buf[0], rgb::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(rgb_buf[1], rgb::Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(rgb_buf[2], rgb::Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(
            rgb_buf[3],
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
        assert!(enc_caps.native_alpha());
        assert!(enc_caps.native_f32());
        assert!(enc_caps.hdr());
        assert!(enc_caps.lossless());
        assert!(enc_caps.stop());
        assert!(enc_caps.enforces_max_pixels());
        assert!(!enc_caps.icc());
        assert!(!enc_caps.lossy());

        let dec_caps = PnmDecoderConfig::capabilities();
        assert!(dec_caps.native_gray());
        assert!(dec_caps.native_alpha());
        assert!(dec_caps.native_16bit());
        assert!(dec_caps.native_f32());
        assert!(dec_caps.hdr());
        assert!(dec_caps.cheap_probe());
        assert!(dec_caps.stop());
        assert!(dec_caps.enforces_max_pixels());
        assert!(!dec_caps.icc());
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
            .decoder(Cow::Borrowed(output.data()), &[])
            .unwrap()
            .decode();
        assert!(result.is_err());
    }

    #[test]
    fn decode_rgb_pixel_data() {
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
        // Verify pixel data through PixelSlice
        let ps = decoded.pixels();
        assert_eq!(ps.width(), 2);
        assert_eq!(ps.rows(), 2);
        // First pixel should be red (255, 0, 0)
        let row0 = ps.row(0);
        assert_eq!(row0[0], 255); // R
        assert_eq!(row0[1], 0); // G
        assert_eq!(row0[2], 0); // B
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
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());
        assert_eq!(output.format(), ImageFormat::Pnm);

        let decoded = decode_bytes(output.data());
        // RgbF32 gets promoted to RgbaF32 in the decode path
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
    }

    #[test]
    fn encode_decode_gray_f32_roundtrip() {
        let pixels = vec![
            rgb::Gray::new(0.0f32),
            rgb::Gray::new(0.25),
            rgb::Gray::new(0.5),
            rgb::Gray::new(1.0),
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
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
        let img = imgref::ImgVec::new(pixels, 2, 2);
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
            .decoder(Cow::Borrowed(output.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
    }

    #[test]
    fn farbfeld_decode_has_alpha() {
        // Farbfeld is always RGBA16 — decoded output must report has_alpha: true.
        let pixels = vec![
            rgb::Rgba::<u8> {
                r: 255,
                g: 0,
                b: 0,
                a: 128,
            },
            rgb::Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
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
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let ff_config = FarbfeldEncoderConfig::new();
        let encoded = ff_config
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();
        assert_eq!(encoded.format(), ImageFormat::Farbfeld);

        let dec_config = FarbfeldDecoderConfig::new();
        let decoded = dec_config
            .job()
            .decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        assert!(
            decoded.has_alpha(),
            "farbfeld RGBA16 decode must report has_alpha"
        );
    }

    #[test]
    fn farbfeld_probe_has_alpha() {
        // Farbfeld probe should also report has_alpha: true.
        let pixels = vec![
            rgb::Rgba::<u8> {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            };
            4
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let encoded = FarbfeldEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let info = FarbfeldDecoderConfig::new()
            .job()
            .probe(encoded.data())
            .unwrap();
        assert!(info.has_alpha, "farbfeld probe must report has_alpha");
    }

    #[test]
    fn pnm_decode_descriptors_exclude_rgba16() {
        // PNM decoder downscales non-gray 16-bit to 8-bit, so RGBA16 must
        // not appear in the supported decode descriptors.
        let descs = PnmDecoderConfig::supported_descriptors();
        assert!(
            !descs.contains(&PixelDescriptor::RGBA16_SRGB),
            "PNM_DECODE_DESCRIPTORS should not contain RGBA16_SRGB"
        );
        // Gray16 IS preserved, so it should be present.
        assert!(
            descs.contains(&PixelDescriptor::GRAY16_SRGB),
            "PNM_DECODE_DESCRIPTORS should contain GRAY16_SRGB"
        );
    }

    #[cfg(feature = "bmp")]
    #[test]
    fn bmp_capabilities_include_native_gray() {
        let caps = BmpDecoderConfig::capabilities();
        assert!(
            caps.native_gray(),
            "BMP decode capabilities should include native_gray"
        );
        assert!(caps.native_alpha());
        assert!(caps.cheap_probe());
    }

    #[cfg(feature = "bmp")]
    #[test]
    fn bmp_decode_descriptors_include_gray8() {
        let descs = BmpDecoderConfig::supported_descriptors();
        assert!(
            descs.contains(&PixelDescriptor::GRAY8_SRGB),
            "BMP_DECODE_DESCRIPTORS should contain GRAY8_SRGB"
        );
    }
}
