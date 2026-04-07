use super::*;

// ══════════════════════════════════════════════════════════════════════
// HDR capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static HDR_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_hdr(true)
    .with_native_f32(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

static HDR_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_hdr(true)
    .with_native_f32(true)
    .with_streaming(true)
    .with_stop(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true);

static HDR_ENCODE_DESCRIPTORS: &[PixelDescriptor] =
    &[PixelDescriptor::RGBF32_LINEAR, PixelDescriptor::RGB8_SRGB];

static HDR_DECODE_DESCRIPTORS: &[PixelDescriptor] = &[PixelDescriptor::RGBF32_LINEAR];

// ══════════════════════════════════════════════════════════════════════
// HDR codec
// ══════════════════════════════════════════════════════════════════════

// ── HdrEncoderConfig ─────────────────────────────────────────────

/// Encoding configuration for Radiance HDR (RGBE) format.
///
/// Accepts `RgbF32` (native) or `Rgb8` (converted via /255.0) input layouts.
#[derive(Clone, Debug)]
pub struct HdrEncoderConfig {
    limits: ResourceLimits,
}

impl Default for HdrEncoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl HdrEncoderConfig {
    /// Create a new HDR encoder config with default settings.
    pub fn new() -> Self {
        Self {
            limits: ResourceLimits::none(),
        }
    }
}

impl zencodec::encode::EncoderConfig for HdrEncoderConfig {
    type Error = BitmapError;
    type Job = HdrEncodeJob;

    fn format() -> ImageFormat {
        ImageFormat::Hdr
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        HDR_ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static EncodeCapabilities {
        &HDR_ENCODE_CAPS
    }

    fn is_lossless(&self) -> Option<bool> {
        Some(false) // RGBE shared exponent is technically lossy
    }

    fn job(self) -> HdrEncodeJob {
        HdrEncodeJob {
            config: self,
            limits: None,
            stop: None,
        }
    }
}

// ── HdrEncodeJob ─────────────────────────────────────────────────

/// Per-operation HDR encode job.
pub struct HdrEncodeJob {
    config: HdrEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
}

impl zencodec::encode::EncodeJob for HdrEncodeJob {
    type Error = BitmapError;
    type Enc = HdrEncoder;
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

    fn encoder(self) -> Result<HdrEncoder, BitmapError> {
        Ok(HdrEncoder {
            config: self.config,
            limits: self.limits,
            stop: self.stop,
            accumulator: None,
        })
    }

    fn animation_frame_encoder(self) -> Result<(), BitmapError> {
        Err(BitmapError::from(
            zencodec::UnsupportedOperation::AnimationEncode,
        ))
    }
}

// ── HdrEncoder ───────────────────────────────────────────────────

/// Accumulator for streaming HDR encode via `push_rows`/`finish`.
struct HdrEncodeAccumulator {
    data: Vec<u8>,
    width: u32,
    total_rows: u32,
    layout: crate::PixelLayout,
}

/// Single-image HDR encoder.
pub struct HdrEncoder {
    config: HdrEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
    accumulator: Option<HdrEncodeAccumulator>,
}

impl HdrEncoder {
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

fn pixel_slice_to_hdr_layout(desc: PixelDescriptor) -> Result<crate::PixelLayout, BitmapError> {
    match (desc.channel_type(), desc.layout()) {
        (ChannelType::F32, ChannelLayout::Rgb) => Ok(crate::PixelLayout::RgbF32),
        (ChannelType::U8, ChannelLayout::Rgb) => Ok(crate::PixelLayout::Rgb8),
        _ => Err(BitmapError::UnsupportedVariant(alloc::format!(
            "HDR encode: unsupported pixel format: {desc:?}"
        ))),
    }
}

impl zencodec::encode::Encoder for HdrEncoder {
    type Error = BitmapError;

    fn reject(op: zencodec::UnsupportedOperation) -> BitmapError {
        BitmapError::from(op)
    }

    fn preferred_strip_height(&self) -> u32 {
        1
    }

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, BitmapError> {
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let w = pixels.width();
        let h = pixels.rows();

        if let Some(limits) = self.effective_limits() {
            limits.check(w, h)?;
        }

        let layout = pixel_slice_to_hdr_layout(pixels.descriptor())?;
        let bytes = pixels.contiguous_bytes();
        let encoded = crate::hdr::encode(&bytes, w, h, layout, stop)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Hdr))
    }

    fn push_rows(&mut self, rows: PixelSlice<'_>) -> Result<(), BitmapError> {
        let layout = pixel_slice_to_hdr_layout(rows.descriptor())?;

        let acc = self
            .accumulator
            .get_or_insert_with(|| HdrEncodeAccumulator {
                data: Vec::new(),
                width: rows.width(),
                total_rows: 0,
                layout,
            });

        if acc.width != rows.width() || acc.layout != layout {
            return Err(BitmapError::InvalidData(
                "push_rows: width or pixel format changed".into(),
            ));
        }

        let bytes = rows.contiguous_bytes();
        acc.data.extend_from_slice(&bytes);
        acc.total_rows += rows.rows();
        Ok(())
    }

    fn finish(self) -> Result<EncodeOutput, BitmapError> {
        let acc = self
            .accumulator
            .ok_or_else(|| BitmapError::InvalidData("finish() without push_rows()".into()))?;

        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };

        let encoded = crate::hdr::encode(&acc.data, acc.width, acc.total_rows, acc.layout, stop)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Hdr))
    }
}

// ── HdrDecoderConfig ─────────────────────────────────────────────

/// Decoding configuration for Radiance HDR (RGBE) format.
#[derive(Clone, Debug)]
pub struct HdrDecoderConfig {
    limits: Option<Limits>,
}

impl Default for HdrDecoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl HdrDecoderConfig {
    /// Create a new HDR decoder config with default settings.
    pub fn new() -> Self {
        Self { limits: None }
    }
}

impl zencodec::decode::DecoderConfig for HdrDecoderConfig {
    type Error = BitmapError;
    type Job<'a> = HdrDecodeJob;

    fn formats() -> &'static [ImageFormat] {
        &[ImageFormat::Hdr]
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        HDR_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static DecodeCapabilities {
        &HDR_DECODE_CAPS
    }

    fn job<'a>(self) -> Self::Job<'a> {
        HdrDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
            policy: None,
        }
    }
}

// ── HdrDecodeJob ─────────────────────────────────────────────────

/// Per-operation HDR decode job.
pub struct HdrDecodeJob {
    config: HdrDecoderConfig,
    limits: Option<Limits>,
    stop: Option<zencodec::StopToken>,
    max_input_bytes: Option<u64>,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for HdrDecodeJob {
    type Error = BitmapError;
    type Dec = HdrDecoder<'a>;
    type StreamDec = HdrStreamingDecoder;
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
        let (width, height, _offset) = crate::hdr::decode::parse_header(data)?;
        let cicp = zencodec::Cicp::new(1, 8, 0, true);
        Ok(ImageInfo::new(width, height, ImageFormat::Hdr)
            .with_alpha(false)
            .with_bit_depth(32)
            .with_channel_count(3)
            .with_cicp(cicp)
            .with_source_encoding_details(BitmapSourceEncoding))
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, BitmapError> {
        let (width, height, _offset) = crate::hdr::decode::parse_header(data)?;
        Ok(
            OutputInfo::full_decode(width, height, PixelDescriptor::RGBF32_LINEAR)
                .with_alpha(false),
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<HdrDecoder<'a>, BitmapError> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            )));
        }
        Ok(HdrDecoder {
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
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<HdrStreamingDecoder, BitmapError> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            )));
        }
        let (width, height, _offset) = crate::hdr::decode::parse_header(&data)?;

        let limits = self.limits.or(self.config.limits);
        if let Some(ref lim) = limits {
            lim.check(width, height)?;
        }

        let row_bytes = (width as usize)
            .checked_mul(12)
            .ok_or(BitmapError::DimensionsTooLarge { width, height })?;

        let total_bytes = row_bytes
            .checked_mul(height as usize)
            .ok_or(BitmapError::DimensionsTooLarge { width, height })?;

        crate::limits::check_output_size(total_bytes, limits.as_ref())?;

        let cicp = zencodec::Cicp::new(1, 8, 0, true);
        let info = ImageInfo::new(width, height, ImageFormat::Hdr)
            .with_alpha(false)
            .with_bit_depth(32)
            .with_channel_count(3)
            .with_cicp(cicp)
            .with_source_encoding_details(BitmapSourceEncoding);

        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::hdr::decode(&data, limits.as_ref(), stop)?;
        let pixels_owned: Vec<u8> = decoded.pixels().to_vec();

        Ok(HdrStreamingDecoder {
            info,
            width,
            height,
            decoded_bytes: pixels_owned,
            row_bytes,
            current_row: 0,
        })
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

// ── HdrDecoder ───────────────────────────────────────────────────

/// Single-image HDR decoder.
pub struct HdrDecoder<'a> {
    config: HdrDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    stop: Option<zencodec::StopToken>,
}

impl HdrDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for HdrDecoder<'_> {
    type Error = BitmapError;

    fn decode(self) -> Result<DecodeOutput, BitmapError> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::hdr::decode(&self.data, limits, stop)?;
        decode_output_from_internal(&decoded, ImageFormat::Hdr)
    }
}

// ── HdrStreamingDecoder ──────────────────────────────────────────

/// Streaming scanline-batch HDR decoder.
pub struct HdrStreamingDecoder {
    info: ImageInfo,
    width: u32,
    height: u32,
    decoded_bytes: Vec<u8>,
    row_bytes: usize,
    current_row: u32,
}

impl zencodec::decode::StreamingDecode for HdrStreamingDecoder {
    type Error = BitmapError;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, BitmapError> {
        if self.current_row >= self.height {
            return Ok(None);
        }

        let y = self.current_row;
        let offset = (y as usize) * self.row_bytes;
        let row_data = &self.decoded_bytes[offset..offset + self.row_bytes];

        let slice = PixelSlice::new(
            row_data,
            self.width,
            1,
            self.row_bytes,
            PixelDescriptor::RGBF32_LINEAR,
        )
        .map_err(|e| BitmapError::InvalidData(e.to_string()))?;

        self.current_row += 1;
        Ok(Some((y, slice)))
    }

    fn info(&self) -> &ImageInfo {
        &self.info
    }
}
