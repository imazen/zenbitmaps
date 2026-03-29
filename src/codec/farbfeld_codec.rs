use super::*;

// ══════════════════════════════════════════════════════════════════════
// Farbfeld capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

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
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true);

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

    fn encoder(self) -> Result<FarbfeldEncoder, BitmapError> {
        Ok(FarbfeldEncoder {
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
    type Job<'a> = FarbfeldDecodeJob;

    fn formats() -> &'static [ImageFormat] {
        &[ImageFormat::Farbfeld]
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        FF_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static DecodeCapabilities {
        &FF_DECODE_CAPS
    }

    fn job<'a>(self) -> Self::Job<'a> {
        FarbfeldDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
            policy: None,
        }
    }
}

// ── FarbfeldDecodeJob ────────────────────────────────────────────────

/// Per-operation farbfeld decode job.
pub struct FarbfeldDecodeJob {
    config: FarbfeldDecoderConfig,
    limits: Option<Limits>,
    stop: Option<zencodec::StopToken>,
    max_input_bytes: Option<u64>,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for FarbfeldDecodeJob {
    type Error = BitmapError;
    type Dec = FarbfeldDecoder<'a>;
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
        let (width, height) = crate::farbfeld::decode::parse_header(data)?;
        Ok(ImageInfo::new(width, height, ImageFormat::Farbfeld)
            .with_alpha(true)
            .with_bit_depth(16)
            .with_channel_count(4)
            .with_source_encoding_details(BitmapSourceEncoding))
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

// ── FarbfeldDecoder ──────────────────────────────────────────────────

/// Single-image farbfeld decoder.
pub struct FarbfeldDecoder<'a> {
    config: FarbfeldDecoderConfig,
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
