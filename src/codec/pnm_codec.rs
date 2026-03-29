use super::*;

use crate::pnm;

// ══════════════════════════════════════════════════════════════════════
// PNM capabilities and descriptors
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
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true);

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
    type AnimationFrameEnc = ();

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_metadata(self, _meta: Metadata) -> Self {
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

    fn animation_frame_encoder(self) -> Result<(), BitmapError> {
        Err(BitmapError::from(
            zencodec::UnsupportedOperation::AnimationEncode,
        ))
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
    type Job<'a> = PnmDecodeJob;

    fn formats() -> &'static [ImageFormat] {
        &[ImageFormat::Pnm]
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        PNM_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static DecodeCapabilities {
        &PNM_DECODE_CAPS
    }

    fn job<'a>(self) -> Self::Job<'a> {
        PnmDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
            policy: None,
        }
    }
}

// ── PnmDecodeJob ─────────────────────────────────────────────────────

/// Per-operation PNM decode job.
pub struct PnmDecodeJob {
    config: PnmDecoderConfig,
    limits: Option<Limits>,
    stop: Option<zencodec::StopToken>,
    max_input_bytes: Option<u64>,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for PnmDecodeJob {
    type Error = BitmapError;
    type Dec = PnmDecoder<'a>;
    type StreamDec = zencodec::Unsupported<BitmapError>;
    type AnimationFrameDec = zencodec::Unsupported<BitmapError>;

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.max_input_bytes = limits.max_input_bytes;
        self.limits = Some(convert_limits(&limits));
        self
    }

    fn with_policy(mut self, policy: DecodePolicy) -> Self {
        self.policy = Some(policy);
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
        Err(BitmapError::from(
            zencodec::UnsupportedOperation::RowLevelDecode,
        ))
    }

    fn animation_frame_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<zencodec::Unsupported<BitmapError>, BitmapError> {
        Err(BitmapError::from(
            zencodec::UnsupportedOperation::AnimationDecode,
        ))
    }
}

// ── PnmDecoder ───────────────────────────────────────────────────────

/// Single-image PNM decoder.
pub struct PnmDecoder<'a> {
    config: PnmDecoderConfig,
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
