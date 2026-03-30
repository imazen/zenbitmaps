use super::*;

// ══════════════════════════════════════════════════════════════════════
// TGA capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static TGA_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_native_gray(true)
    .with_native_alpha(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

static TGA_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_native_gray(true)
    .with_native_alpha(true)
    .with_streaming(true)
    .with_stop(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true);

static TGA_ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
];

static TGA_DECODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::GRAY8_SRGB,
];

// ══════════════════════════════════════════════════════════════════════
// TGA codec
// ══════════════════════════════════════════════════════════════════════

// ── TgaEncoderConfig ─────────────────────────────────────────────

/// Encoding configuration for TGA format.
#[derive(Clone, Debug)]
pub struct TgaEncoderConfig {
    limits: ResourceLimits,
}

impl Default for TgaEncoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl TgaEncoderConfig {
    /// Create a new TGA encoder config with default settings.
    pub fn new() -> Self {
        Self {
            limits: ResourceLimits::none(),
        }
    }
}

impl zencodec::encode::EncoderConfig for TgaEncoderConfig {
    type Error = BitmapError;
    type Job = TgaEncodeJob;

    fn format() -> ImageFormat {
        ImageFormat::Tga
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        TGA_ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static EncodeCapabilities {
        &TGA_ENCODE_CAPS
    }

    fn is_lossless(&self) -> Option<bool> {
        Some(true)
    }

    fn job(self) -> TgaEncodeJob {
        TgaEncodeJob {
            config: self,
            limits: None,
            stop: None,
        }
    }
}

// ── TgaEncodeJob ─────────────────────────────────────────────────

/// Per-operation TGA encode job.
pub struct TgaEncodeJob {
    config: TgaEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
}

impl zencodec::encode::EncodeJob for TgaEncodeJob {
    type Error = BitmapError;
    type Enc = TgaEncoder;
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

    fn encoder(self) -> Result<TgaEncoder, BitmapError> {
        Ok(TgaEncoder {
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

// ── TgaEncoder ───────────────────────────────────────────────────

struct TgaEncodeAccumulator {
    data: Vec<u8>,
    width: u32,
    total_rows: u32,
    layout: crate::PixelLayout,
}

/// Single-image TGA encoder.
pub struct TgaEncoder {
    config: TgaEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
    accumulator: Option<TgaEncodeAccumulator>,
}

impl TgaEncoder {
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

fn pixel_slice_to_tga_layout(desc: PixelDescriptor) -> Result<crate::PixelLayout, BitmapError> {
    match (desc.channel_type(), desc.layout()) {
        (ChannelType::U8, ChannelLayout::Rgb) => Ok(crate::PixelLayout::Rgb8),
        (ChannelType::U8, ChannelLayout::Rgba) => Ok(crate::PixelLayout::Rgba8),
        (ChannelType::U8, ChannelLayout::Gray) => Ok(crate::PixelLayout::Gray8),
        (ChannelType::U8, ChannelLayout::Bgra) => Ok(crate::PixelLayout::Bgra8),
        _ => Err(BitmapError::UnsupportedVariant(alloc::format!(
            "TGA encode: unsupported pixel format: {desc:?}"
        ))),
    }
}

impl zencodec::encode::Encoder for TgaEncoder {
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

        let layout = pixel_slice_to_tga_layout(pixels.descriptor())?;
        let bytes = pixels.contiguous_bytes();
        let encoded = crate::tga::encode(&bytes, w, h, layout, stop)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Tga))
    }

    fn push_rows(&mut self, rows: PixelSlice<'_>) -> Result<(), BitmapError> {
        let layout = pixel_slice_to_tga_layout(rows.descriptor())?;

        let acc = self
            .accumulator
            .get_or_insert_with(|| TgaEncodeAccumulator {
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

        let encoded = crate::tga::encode(&acc.data, acc.width, acc.total_rows, acc.layout, stop)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Tga))
    }
}

// ── TgaDecoderConfig ─────────────────────────────────────────────

/// Decoding configuration for TGA format.
#[derive(Clone, Debug)]
pub struct TgaDecoderConfig {
    limits: Option<Limits>,
}

impl Default for TgaDecoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl TgaDecoderConfig {
    /// Create a new TGA decoder config with default settings.
    pub fn new() -> Self {
        Self { limits: None }
    }
}

impl zencodec::decode::DecoderConfig for TgaDecoderConfig {
    type Error = BitmapError;
    type Job<'a> = TgaDecodeJob;

    fn formats() -> &'static [ImageFormat] {
        &[ImageFormat::Tga]
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        TGA_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static DecodeCapabilities {
        &TGA_DECODE_CAPS
    }

    fn job<'a>(self) -> Self::Job<'a> {
        TgaDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
            policy: None,
        }
    }
}

// ── TgaDecodeJob ─────────────────────────────────────────────────

/// Per-operation TGA decode job.
pub struct TgaDecodeJob {
    config: TgaDecoderConfig,
    limits: Option<Limits>,
    stop: Option<zencodec::StopToken>,
    max_input_bytes: Option<u64>,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for TgaDecodeJob {
    type Error = BitmapError;
    type Dec = TgaDecoder<'a>;
    type StreamDec = TgaStreamingDecoder;
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
        let header = crate::tga::decode::parse_header(data)?;
        let has_alpha = header.pixel_depth == 32
            || (header.is_color_mapped() && header.color_map_depth == 32)
            || header.alpha_bits() > 0;
        let channel_count: u8 = if header.is_grayscale() {
            1
        } else if has_alpha {
            4
        } else {
            3
        };
        Ok(
            ImageInfo::new(header.width as u32, header.height as u32, ImageFormat::Tga)
                .with_alpha(has_alpha)
                .with_bit_depth(header.pixel_depth)
                .with_channel_count(channel_count)
                .with_cicp(zencodec::Cicp::SRGB)
                .with_source_encoding_details(BitmapSourceEncoding),
        )
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, BitmapError> {
        let header = crate::tga::decode::parse_header(data)?;
        let has_alpha = header.pixel_depth == 32
            || (header.is_color_mapped() && header.color_map_depth == 32)
            || header.alpha_bits() > 0;
        let desc = if header.is_grayscale() {
            PixelDescriptor::GRAY8_SRGB
        } else if has_alpha {
            PixelDescriptor::RGBA8_SRGB
        } else {
            PixelDescriptor::RGB8_SRGB
        };
        Ok(
            OutputInfo::full_decode(header.width as u32, header.height as u32, desc)
                .with_alpha(has_alpha),
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<TgaDecoder<'a>, BitmapError> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            )));
        }
        Ok(TgaDecoder {
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
    ) -> Result<TgaStreamingDecoder, BitmapError> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            )));
        }

        let limits = self.limits.or(self.config.limits);

        // Buffer-and-yield: decode full image, yield rows from next_batch()
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::tga::decode(&data, limits.as_ref(), stop)?;

        let width = decoded.width;
        let height = decoded.height;
        let layout = decoded.layout;
        let out_channels = layout.bytes_per_pixel();
        let row_bytes = width as usize * out_channels;

        let descriptor = layout_to_descriptor(layout);
        let has_alpha = matches!(layout, crate::PixelLayout::Rgba8);
        let info = ImageInfo::new(width, height, ImageFormat::Tga)
            .with_alpha(has_alpha)
            .with_bit_depth(8)
            .with_channel_count(out_channels as u8)
            .with_cicp(zencodec::Cicp::SRGB)
            .with_source_encoding_details(BitmapSourceEncoding);

        let pixels_owned = decoded.pixels().to_vec();

        Ok(TgaStreamingDecoder {
            info,
            descriptor,
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

// ── TgaDecoder ───────────────────────────────────────────────────

/// Single-image TGA decoder.
pub struct TgaDecoder<'a> {
    config: TgaDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    stop: Option<zencodec::StopToken>,
}

impl TgaDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for TgaDecoder<'_> {
    type Error = BitmapError;

    fn decode(self) -> Result<DecodeOutput, BitmapError> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::tga::decode(&self.data, limits, stop)?;
        decode_output_from_internal(&decoded, ImageFormat::Tga)
    }
}

// ── TgaStreamingDecoder ──────────────────────────────────────────

/// Streaming scanline-batch TGA decoder.
///
/// Decodes the full image eagerly (TGA's bottom-up origin and RLE make true
/// row-level streaming impractical), then yields one row at a time.
pub struct TgaStreamingDecoder {
    info: ImageInfo,
    descriptor: PixelDescriptor,
    width: u32,
    height: u32,
    decoded_bytes: Vec<u8>,
    row_bytes: usize,
    current_row: u32,
}

impl zencodec::decode::StreamingDecode for TgaStreamingDecoder {
    type Error = BitmapError;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, BitmapError> {
        if self.current_row >= self.height {
            return Ok(None);
        }

        let y = self.current_row;
        let offset = (y as usize) * self.row_bytes;
        let row_data = &self.decoded_bytes[offset..offset + self.row_bytes];

        let slice = PixelSlice::new(row_data, self.width, 1, self.row_bytes, self.descriptor)
            .map_err(|e| BitmapError::InvalidData(e.to_string()))?;

        self.current_row += 1;
        Ok(Some((y, slice)))
    }

    fn info(&self) -> &ImageInfo {
        &self.info
    }
}
