use super::*;
use whereat::{At, ResultAtExt, at};

// ══════════════════════════════════════════════════════════════════════
// QOI capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static QOI_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_native_alpha(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

static QOI_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_native_alpha(true)
    .with_streaming(true)
    .with_stop(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true);

static QOI_ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
];

static QOI_DECODE_DESCRIPTORS: &[PixelDescriptor] =
    &[PixelDescriptor::RGB8_SRGB, PixelDescriptor::RGBA8_SRGB];

// ══════════════════════════════════════════════════════════════════════
// QOI codec
// ══════════════════════════════════════════════════════════════════════

// ── QoiEncoderConfig ─────────────────────────────────────────────

/// Encoding configuration for QOI format.
///
/// Accepts Rgb8, Rgba8, Bgr8, Bgra8 input layouts.
#[derive(Clone, Debug)]
pub struct QoiEncoderConfig {
    limits: ResourceLimits,
}

impl Default for QoiEncoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl QoiEncoderConfig {
    /// Create a new QOI encoder config with default settings.
    pub fn new() -> Self {
        Self {
            limits: ResourceLimits::none(),
        }
    }
}

impl zencodec::encode::EncoderConfig for QoiEncoderConfig {
    type Error = At<BitmapError>;
    type Job = QoiEncodeJob;

    fn format() -> ImageFormat {
        ImageFormat::Qoi
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        QOI_ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static EncodeCapabilities {
        &QOI_ENCODE_CAPS
    }

    fn is_lossless(&self) -> Option<bool> {
        Some(true)
    }

    fn estimate_encode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // QOI runs/indices/diffs compress typical content; ~0.6× is a coarse
        // structural guess (worst case is still bounded by input + a small
        // per-pixel overhead, but typical output is smaller than the raw input).
        trivial_encode_resources(image, compute, 0.6)
    }

    fn job(self) -> QoiEncodeJob {
        QoiEncodeJob {
            config: self,
            limits: None,
            stop: None,
        }
    }
}

// ── QoiEncodeJob ─────────────────────────────────────────────────

/// Per-operation QOI encode job.
pub struct QoiEncodeJob {
    config: QoiEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
}

impl zencodec::encode::EncodeJob for QoiEncodeJob {
    type Error = At<BitmapError>;
    type Enc = QoiEncoder;
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

    fn encoder(self) -> crate::Result<QoiEncoder> {
        Ok(QoiEncoder {
            config: self.config,
            limits: self.limits,
            stop: self.stop,
            accumulator: None,
        })
    }

    fn animation_frame_encoder(self) -> crate::Result<()> {
        Err(at!(BitmapError::from(
            zencodec::UnsupportedOperation::AnimationEncode,
        )))
    }
}

// ── QoiEncoder ───────────────────────────────────────────────────

/// Accumulator for streaming QOI encode via `push_rows`/`finish`.
struct QoiEncodeAccumulator {
    data: Vec<u8>,
    width: u32,
    total_rows: u32,
    channels: usize,
    needs_swizzle: bool,
}

/// Single-image QOI encoder.
pub struct QoiEncoder {
    config: QoiEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
    accumulator: Option<QoiEncodeAccumulator>,
}

impl QoiEncoder {
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

impl zencodec::encode::Encoder for QoiEncoder {
    type Error = At<BitmapError>;

    fn reject(op: zencodec::UnsupportedOperation) -> At<BitmapError> {
        at!(BitmapError::from(op))
    }

    fn preferred_strip_height(&self) -> u32 {
        1 // QOI is scanline-oriented
    }

    fn encode(self, pixels: PixelSlice<'_>) -> crate::Result<EncodeOutput> {
        // Bit-exact load-bearing narrowing (dead alpha / chroma-free /
        // replicated-low-bits) before format mapping — see
        // `super::reduce_for_raw_encode`. QOI has no grayscale layout, so
        // the predicate forbids the →Gray narrowing (chroma-free RGB stays
        // RGB) while still allowing dead-alpha and bit-depth reductions.
        let reduced = super::reduce_for_raw_encode(&pixels, |d| {
            matches!(
                (d.channel_type(), d.layout()),
                (ChannelType::U8, ChannelLayout::Rgb)
                    | (ChannelType::U8, ChannelLayout::Rgba)
                    | (ChannelType::U8, ChannelLayout::Bgra)
            )
        });
        let pixels = match &reduced {
            Some(buf) => buf.as_slice(),
            None => pixels,
        };
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
            (ChannelType::U8, ChannelLayout::Rgb) => crate::PixelLayout::Rgb8,
            (ChannelType::U8, ChannelLayout::Rgba) => crate::PixelLayout::Rgba8,
            (ChannelType::U8, ChannelLayout::Bgra) => crate::PixelLayout::Bgra8,
            _ => {
                return Err(at!(BitmapError::UnsupportedVariant(alloc::format!(
                    "QOI encode: unsupported pixel format: {desc:?}"
                ))));
            }
        };

        let encoded = crate::qoi::encode(&bytes, w, h, layout, stop)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Qoi))
    }

    fn push_rows(&mut self, rows: PixelSlice<'_>) -> crate::Result<()> {
        let desc = rows.descriptor();
        let channels: usize = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U8, ChannelLayout::Rgb) => 3,
            (ChannelType::U8, ChannelLayout::Rgba) => 4,
            (ChannelType::U8, ChannelLayout::Bgra) => 4,
            _ => {
                return Err(at!(BitmapError::UnsupportedVariant(alloc::format!(
                    "QOI push_rows: unsupported pixel format: {desc:?}"
                ))));
            }
        };

        let acc = self
            .accumulator
            .get_or_insert_with(|| QoiEncodeAccumulator {
                data: Vec::new(),
                width: rows.width(),
                total_rows: 0,
                channels,
                needs_swizzle: desc.layout() == ChannelLayout::Bgra,
            });

        if acc.width != rows.width() || acc.channels != channels {
            return Err(at!(BitmapError::InvalidData(
                "push_rows: width or channel count changed".into(),
            )));
        }

        let bytes = rows.contiguous_bytes();
        if acc.needs_swizzle {
            // BGRA → RGBA swizzle
            for pixel in bytes.chunks_exact(4) {
                acc.data.push(pixel[2]);
                acc.data.push(pixel[1]);
                acc.data.push(pixel[0]);
                acc.data.push(pixel[3]);
            }
        } else {
            acc.data.extend_from_slice(&bytes);
        }
        acc.total_rows += rows.rows();
        Ok(())
    }

    fn finish(self) -> crate::Result<EncodeOutput> {
        let acc = self.accumulator.ok_or_else(|| {
            at!(BitmapError::InvalidData(
                "finish() without push_rows()".into()
            ))
        })?;

        let colors = if acc.channels == 4 {
            crate::qoi::rapid_qoi::Colors::SrgbLinA
        } else {
            crate::qoi::rapid_qoi::Colors::Srgb
        };
        let qoi = crate::qoi::rapid_qoi::Qoi {
            width: acc.width,
            height: acc.total_rows,
            colors,
        };
        let encoded = qoi
            .encode_alloc(&acc.data)
            .map_err(|e| at!(BitmapError::InvalidData(e.to_string())))?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Qoi))
    }
}

// ── QoiDecoderConfig ─────────────────────────────────────────────

/// Decoding configuration for QOI format.
#[derive(Clone, Debug)]
pub struct QoiDecoderConfig {
    limits: Option<Limits>,
}

impl Default for QoiDecoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl QoiDecoderConfig {
    /// Create a new QOI decoder config with default settings.
    pub fn new() -> Self {
        Self { limits: None }
    }
}

impl zencodec::decode::DecoderConfig for QoiDecoderConfig {
    type Error = At<BitmapError>;
    type Job<'a> = QoiDecodeJob;

    fn formats() -> &'static [ImageFormat] {
        &[ImageFormat::Qoi]
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        QOI_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static DecodeCapabilities {
        &QOI_DECODE_CAPS
    }

    fn job<'a>(self) -> Self::Job<'a> {
        QoiDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
            policy: None,
        }
    }
}

// ── QoiDecodeJob ─────────────────────────────────────────────────

/// Per-operation QOI decode job.
pub struct QoiDecodeJob {
    config: QoiDecoderConfig,
    limits: Option<Limits>,
    stop: Option<zencodec::StopToken>,
    max_input_bytes: Option<u64>,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for QoiDecodeJob {
    type Error = At<BitmapError>;
    type Dec = QoiDecoder<'a>;
    type StreamDec = QoiStreamingDecoder<'a>;
    type AnimationFrameDec = zencodec::Unsupported<At<BitmapError>>;

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

    fn probe(&self, data: &[u8]) -> crate::Result<ImageInfo> {
        let hdr = crate::qoi::decode::parse_header(data)?;
        let cicp = if hdr.is_linear {
            zencodec::Cicp::new(1, 8, 0, true) // BT.709 primaries, Linear transfer
        } else {
            zencodec::Cicp::SRGB
        };
        Ok(ImageInfo::new(hdr.width, hdr.height, ImageFormat::Qoi)
            .with_alpha(hdr.has_alpha)
            .with_bit_depth(8)
            .with_channel_count(if hdr.has_alpha { 4 } else { 3 })
            .with_cicp(cicp)
            .with_source_encoding_details(BitmapSourceEncoding))
    }

    fn output_info(&self, data: &[u8]) -> crate::Result<OutputInfo> {
        let hdr = crate::qoi::decode::parse_header(data)?;
        let (width, height, has_alpha) = (hdr.width, hdr.height, hdr.has_alpha);
        let desc = if has_alpha {
            PixelDescriptor::RGBA8_SRGB
        } else {
            PixelDescriptor::RGB8_SRGB
        };
        Ok(OutputInfo::full_decode(width, height, desc).with_alpha(has_alpha))
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> crate::Result<QoiDecoder<'a>> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(at!(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))));
        }
        Ok(QoiDecoder {
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
            at!(BitmapError::InvalidData(e.to_string()))
        })
    }

    fn streaming_decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> crate::Result<QoiStreamingDecoder<'a>> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(at!(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))));
        }
        let hdr_info = crate::qoi::decode::parse_header(&data)?;
        let (width, height, has_alpha) = (hdr_info.width, hdr_info.height, hdr_info.has_alpha);

        let limits = self.limits.or(self.config.limits);
        if let Some(ref lim) = limits {
            lim.check(width, height)?;
        }

        let channels: usize = if has_alpha { 4 } else { 3 };
        let row_bytes = (width as usize)
            .checked_mul(channels)
            .ok_or_else(|| at!(BitmapError::DimensionsTooLarge { width, height }))?;

        crate::limits::check_output_size(
            row_bytes.saturating_mul(height as usize),
            limits.as_ref(),
        )?;

        let descriptor = if has_alpha {
            PixelDescriptor::RGBA8_SRGB
        } else {
            PixelDescriptor::RGB8_SRGB
        };
        let info = ImageInfo::new(width, height, ImageFormat::Qoi)
            .with_alpha(has_alpha)
            .with_bit_depth(8)
            .with_channel_count(channels as u8)
            .with_source_encoding_details(BitmapSourceEncoding);

        QoiStreamingDecoder::create(
            data, info, descriptor, width, height, has_alpha, row_bytes, self.stop,
        )
    }

    fn animation_frame_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> crate::Result<zencodec::Unsupported<At<BitmapError>>> {
        Err(at!(BitmapError::from(
            zencodec::UnsupportedOperation::AnimationDecode,
        )))
    }
}

// ── QoiDecoder ───────────────────────────────────────────────────

/// Single-image QOI decoder.
pub struct QoiDecoder<'a> {
    config: QoiDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    stop: Option<zencodec::StopToken>,
}

impl QoiDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for QoiDecoder<'_> {
    type Error = At<BitmapError>;

    fn decode(self) -> crate::Result<DecodeOutput> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::qoi::decode(&self.data, limits, stop)?;
        decode_output_from_internal(&decoded, ImageFormat::Qoi)
    }
}

// ── QoiStreamingDecoder ──────────────────────────────────────────

/// Streaming scanline-batch QOI decoder.
///
/// Yields one row at a time via `next_batch()`, carrying decode state across
/// rows via the vendored-kernel wrapper [`crate::qoi::decode::QoiDecodeState`].
pub struct QoiStreamingDecoder<'a> {
    data: Cow<'a, [u8]>,
    info: ImageInfo,
    descriptor: PixelDescriptor,
    width: u32,
    height: u32,
    has_alpha: bool,
    row_buf: Vec<u8>,
    current_row: u32,
    byte_offset: usize,
    // Streaming decode state over the vendored kernel (runs are clamped and
    // carried across rows — see `crate::qoi::decode::QoiDecodeState`).
    state_rgb: crate::qoi::decode::QoiDecodeState<3>,
    state_rgba: crate::qoi::decode::QoiDecodeState<4>,
    stop: Option<zencodec::StopToken>,
}

impl<'a> QoiStreamingDecoder<'a> {
    #[allow(clippy::too_many_arguments)]
    fn create(
        data: Cow<'a, [u8]>,
        info: ImageInfo,
        descriptor: PixelDescriptor,
        width: u32,
        height: u32,
        has_alpha: bool,
        row_bytes: usize,
        stop: Option<zencodec::StopToken>,
    ) -> crate::Result<Self> {
        Ok(Self {
            data,
            info,
            descriptor,
            width,
            height,
            has_alpha,
            row_buf: alloc::vec![0u8; row_bytes],
            current_row: 0,
            byte_offset: 14, // skip QOI header
            state_rgb: crate::qoi::decode::QoiDecodeState::<3>::new(),
            state_rgba: crate::qoi::decode::QoiDecodeState::<4>::new(),
            stop,
        })
    }
}

impl zencodec::decode::StreamingDecode for QoiStreamingDecoder<'_> {
    type Error = At<BitmapError>;

    fn next_batch(&mut self) -> crate::Result<Option<(u32, PixelSlice<'_>)>> {
        if self.current_row >= self.height {
            return Ok(None);
        }

        if let Some(ref stop) = self.stop {
            stop.check().map_err(|r| at!(BitmapError::from(r)))?;
        }

        let encoded = self
            .data
            .get(self.byte_offset..)
            .ok_or_else(|| at!(BitmapError::UnexpectedEof))?;

        if self.has_alpha {
            let consumed = self
                .state_rgba
                .decode_into(encoded, &mut self.row_buf)
                .map_err(|()| at!(BitmapError::UnexpectedEof))?;
            self.byte_offset += consumed;
        } else {
            let consumed = self
                .state_rgb
                .decode_into(encoded, &mut self.row_buf)
                .map_err(|()| at!(BitmapError::UnexpectedEof))?;
            self.byte_offset += consumed;
        }

        let y = self.current_row;
        self.current_row += 1;

        let stride = self.row_buf.len();
        let slice = PixelSlice::new(&self.row_buf, self.width, 1, stride, self.descriptor)
            .map_err_at(|inner| BitmapError::InvalidData(inner.to_string()))?;

        Ok(Some((y, slice)))
    }

    fn info(&self) -> &ImageInfo {
        &self.info
    }
}
