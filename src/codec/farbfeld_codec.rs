use super::*;
use whereat::At;

use crate::alloc_util::AllocPref;

// ══════════════════════════════════════════════════════════════════════
// Farbfeld capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static FF_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_push_rows(true)
    .with_streaming_output(true)
    .with_native_alpha(true)
    .with_native_16bit(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

static FF_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_native_alpha(true)
    .with_native_16bit(true)
    .with_stop(true)
    .with_incremental(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true)
    .with_inventory(cfg!(feature = "inventory"));

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
    type Error = At<zencodec::CodecError>;
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

    fn estimate_encode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // Raw RGBA16 farbfeld: output ≈ input (no entropy coding).
        trivial_encode_resources(image, compute, 1.0)
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
    type Error = At<zencodec::CodecError>;
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

    fn encoder(self) -> Result<FarbfeldEncoder, Self::Error> {
        Ok(FarbfeldEncoder {
            config: self.config,
            limits: self.limits,
            stop: self.stop,
            stream: None,
        })
    }

    fn animation_frame_encoder(self) -> Result<(), Self::Error> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationEncode).into())
    }
}

// ── FarbfeldEncoder ──────────────────────────────────────────────────

/// Single-image farbfeld encoder.
pub struct FarbfeldEncoder {
    config: FarbfeldEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
    stream: Option<super::row_stream::RowStream>,
}

/// The farbfeld layout `encode` writes `desc` from.
fn farbfeld_file(
    desc: PixelDescriptor,
) -> Result<super::row_stream::RowFile, At<zencodec::CodecError>> {
    let layout = match (desc.channel_type(), desc.layout()) {
        (ChannelType::U16, ChannelLayout::Rgba) => crate::PixelLayout::Rgba16,
        (ChannelType::U8, ChannelLayout::Rgba) => crate::PixelLayout::Rgba8,
        (ChannelType::U8, ChannelLayout::Rgb) => crate::PixelLayout::Rgb8,
        (ChannelType::U8, ChannelLayout::Gray) => crate::PixelLayout::Gray8,
        _ => return Err(BitmapError::from(zencodec::UnsupportedOperation::PixelFormat).into()),
    };
    Ok(super::row_stream::RowFile::Farbfeld(layout))
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
    type Error = At<zencodec::CodecError>;

    fn reject(op: zencodec::UnsupportedOperation) -> At<zencodec::CodecError> {
        BitmapError::from(op).into()
    }

    fn begin(
        &mut self,
        width: u32,
        height: u32,
        descriptor: PixelDescriptor,
    ) -> Result<(), Self::Error> {
        if self.stream.is_some() {
            return Err(BitmapError::InvalidData("begin after rows were pushed".into()).into());
        }
        let file = farbfeld_file(descriptor)?;
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        self.stream = Some(super::row_stream::RowStream::start(
            file,
            width,
            Some(height),
            descriptor,
            limits,
            stop,
        )?);
        Ok(())
    }

    /// Encode a strip as it arrives; after `begin` the bytes stream through
    /// `pending_output`. Byte-identical to `encode` on the same pixels.
    fn push_rows(&mut self, rows: PixelSlice<'_>) -> Result<(), Self::Error> {
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        if self.stream.is_none() {
            let file = farbfeld_file(rows.descriptor())?;
            let limits = self.effective_limits();
            self.stream = Some(super::row_stream::RowStream::start(
                file,
                rows.width(),
                None,
                rows.descriptor(),
                limits,
                stop,
            )?);
        }
        self.stream
            .as_mut()
            .expect("started above")
            .push(rows, stop)
    }

    fn pending_output(&self) -> &[u8] {
        self.stream.as_ref().map_or(&[], |s| s.pending())
    }

    fn consume_output(&mut self, n: usize) {
        if let Some(s) = &mut self.stream {
            s.consume(n);
        }
    }

    fn finish(self) -> Result<EncodeOutput, Self::Error> {
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let stream = self.stream.ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::InvalidData(
                "finish() without push_rows()".into(),
            ))
        })?;
        Ok(EncodeOutput::new(
            stream.finish(stop)?,
            ImageFormat::Farbfeld,
        ))
    }

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, Self::Error> {
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let desc = pixels.descriptor();
        let w = pixels.width();
        let h = pixels.rows();

        if let Some(limits) = self.effective_limits() {
            limits.check(w, h).map_err(zencodec::CodecError::of)?;
        }

        let bytes = pixels.contiguous_bytes();
        let layout = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U16, ChannelLayout::Rgba) => crate::PixelLayout::Rgba16,
            (ChannelType::U8, ChannelLayout::Rgba) => crate::PixelLayout::Rgba8,
            (ChannelType::U8, ChannelLayout::Rgb) => crate::PixelLayout::Rgb8,
            (ChannelType::U8, ChannelLayout::Gray) => crate::PixelLayout::Gray8,
            // A well-formed request for a pixel format this encoder does not
            // support — a caller-request-origin failure (routed via the
            // existing zencodec::UnsupportedOperation::PixelFormat axis),
            // not an image-bytes-origin `UnsupportedVariant`.
            _ => {
                return Err(Self::reject(zencodec::UnsupportedOperation::PixelFormat));
            }
        };

        let encoded = crate::farbfeld::encode(&bytes, w, h, layout, stop)
            .map_err(zencodec::CodecError::of)?;
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
    type Error = At<zencodec::CodecError>;
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

    fn estimate_decode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // Farbfeld working set ≈ output buffer only (serial, big-endian swap).
        super::trivial_decode_resources(image, compute)
    }

    fn job<'a>(self) -> Self::Job<'a> {
        FarbfeldDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
            alloc_pref: AllocPref::CodecDefault,
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
    /// Allocation-fallibility preference from
    /// [`ResourceLimits::prefer_fallible_allocations`].
    alloc_pref: AllocPref,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for FarbfeldDecodeJob {
    type Error = At<zencodec::CodecError>;
    type Dec = FarbfeldDecoder<'a>;
    type StreamDec = zencodec::Unsupported<At<zencodec::CodecError>>;
    type AnimationFrameDec = zencodec::Unsupported<At<zencodec::CodecError>>;

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.max_input_bytes = limits.max_input_bytes;
        self.alloc_pref = AllocPref::from(limits.prefer_fallible_allocations);
        self.limits = Some(convert_limits(&limits));
        self
    }

    fn with_policy(mut self, policy: DecodePolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    fn probe(&self, data: &[u8]) -> Result<ImageInfo, Self::Error> {
        let (width, height) =
            crate::farbfeld::decode::parse_header(data).map_err(zencodec::CodecError::of)?;
        Ok(farbfeld_info(width, height))
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, Self::Error> {
        let (width, height) =
            crate::farbfeld::decode::parse_header(data).map_err(zencodec::CodecError::of)?;
        Ok(OutputInfo::full_decode(width, height, PixelDescriptor::RGBA16_SRGB).with_alpha(true))
    }

    #[cfg(feature = "inventory")]
    fn inventory(&self, data: &[u8]) -> Result<zencodec::inventory::Inventory, Self::Error> {
        let limits = self.limits.as_ref().or(self.config.limits.as_ref());
        let output = || {
            crate::farbfeld::decode::parse_header(data)
                .ok()
                .map(|(w, h)| (w, h, Some(8)))
        };
        super::job_inventory(
            data,
            self.stop.as_ref(),
            self.max_input_bytes,
            limits,
            output,
            |data, _control| crate::inventory::farbfeld::walk(data),
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<FarbfeldDecoder<'a>, Self::Error> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))
            .into());
        }
        Ok(FarbfeldDecoder {
            config: self.config,
            limits: self.limits,
            data,
            stop: self.stop,
            alloc_pref: self.alloc_pref,
        })
    }

    fn push_decoder(
        self,
        data: Cow<'a, [u8]>,
        sink: &mut dyn zencodec::decode::DecodeRowSink,
        preferred: &[PixelDescriptor],
    ) -> Result<OutputInfo, Self::Error> {
        zencodec::helpers::copy_decode_to_sink(self, data, sink, preferred, |e| {
            BitmapError::InvalidData(e.to_string()).into()
        })
    }

    fn streaming_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<zencodec::Unsupported<At<zencodec::CodecError>>, Self::Error> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::RowLevelDecode).into())
    }

    fn animation_frame_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<zencodec::Unsupported<At<zencodec::CodecError>>, Self::Error> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationDecode).into())
    }

    fn dyn_incremental_decoder(
        self,
        _preferred: &[PixelDescriptor],
    ) -> Result<
        alloc::boxed::Box<zencodec::decode::DynIncrementalDecoder>,
        zencodec::decode::BoxedError,
    > {
        let limits = self.limits.clone().or_else(|| self.config.limits.clone());
        Ok(zencodec::decode::erase_incremental(
            super::strips::StripDecoder::new(
                FarbfeldStrips {
                    limits,
                    alloc_pref: self.alloc_pref,
                    width: 0,
                },
                self.stop,
                self.max_input_bytes,
            ),
        ))
    }
}

fn farbfeld_info(width: u32, height: u32) -> ImageInfo {
    ImageInfo::new(width, height, ImageFormat::Farbfeld)
        .with_alpha(true)
        .with_bit_depth(16)
        .with_channel_count(4)
        .with_cicp(zencodec::Cicp::SRGB)
        .with_source_encoding_details(BitmapSourceEncoding)
}

/// Farbfeld on the strip decoder: 16-byte header, then big-endian RGBA16
/// rows, converted a strip at a time with the one-shot row function.
struct FarbfeldStrips {
    limits: Option<Limits>,
    alloc_pref: AllocPref,
    width: u32,
}

impl super::strips::StripFormat for FarbfeldStrips {
    fn geometry(
        &mut self,
        prefix: &[u8],
        at_end: bool,
    ) -> crate::Result<Option<super::strips::Geometry>> {
        use crate::limits;
        let (width, height) = match crate::farbfeld::decode::parse_header(prefix) {
            Ok(wh) => wh,
            Err(e) if !at_end && matches!(e.error(), BitmapError::UnexpectedEof) => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let lim = self.limits.as_ref();
        limits::check_dimensions(width, height, lim)?;
        let out_bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|px| px.checked_mul(8))
            .ok_or_else(|| {
                whereat::at!(BitmapError::OutOfMemory(
                    "output size overflows usize".into()
                ))
            })?;
        limits::check_output_size(out_bytes, lim)?;
        self.width = width;
        Ok(Some(super::strips::Geometry {
            width,
            height,
            layout: crate::PixelLayout::Rgba16,
            info: farbfeld_info(width, height),
            body: super::strips::Body::Rows {
                data_offset: 16,
                row_bytes: width as usize * 8,
                bottom_up: false,
            },
        }))
    }

    fn decode_rows<'s>(
        &mut self,
        src: &'s [u8],
        n: u32,
        stop: &dyn Stop,
    ) -> crate::Result<(crate::decode::DecodeOutput<'s>, usize)> {
        stop.check()
            .map_err(|r| whereat::at!(BitmapError::from(r)))?;
        let rows = crate::farbfeld::decode::decode_rows(src, self.alloc_pref)?;
        Ok((
            crate::decode::DecodeOutput::owned(rows, self.width, n, crate::PixelLayout::Rgba16),
            src.len(),
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
    alloc_pref: AllocPref,
}

impl FarbfeldDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for FarbfeldDecoder<'_> {
    type Error = At<zencodec::CodecError>;

    fn decode(self) -> Result<DecodeOutput, Self::Error> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded =
            crate::farbfeld::decode_with_alloc_pref(&self.data, limits, self.alloc_pref, stop)
                .map_err(zencodec::CodecError::of)?;
        decode_output_from_internal(&decoded, ImageFormat::Farbfeld)
            .map_err(zencodec::CodecError::of)
    }
}
