use super::*;
use whereat::At;

use crate::alloc_util::AllocPref;

// ══════════════════════════════════════════════════════════════════════
// HDR capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static HDR_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_hdr(true)
    .with_push_rows(true)
    .with_streaming_output(true)
    .with_native_f32(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

static HDR_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_hdr(true)
    .with_native_f32(true)
    .with_streaming(true)
    .with_incremental(true)
    .with_stop(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true)
    .with_inventory(cfg!(feature = "inventory"));

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
    type Error = At<zencodec::CodecError>;
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

    fn estimate_encode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // Radiance RGBE (4 bytes/pixel): treat output ≈ input as a structural
        // upper bound (any RLE run-encoding only ever shrinks it).
        trivial_encode_resources(image, compute, 1.0)
    }

    fn job(self) -> HdrEncodeJob {
        HdrEncodeJob {
            config: self,
            limits: None,
            pulse: zencodec::SharedPulse::default(),
        }
    }
}

// ── HdrEncodeJob ─────────────────────────────────────────────────

/// Per-operation HDR encode job.
pub struct HdrEncodeJob {
    config: HdrEncoderConfig,
    limits: Option<ResourceLimits>,
    pulse: zencodec::SharedPulse,
}

impl zencodec::encode::EncodeJob for HdrEncodeJob {
    type Error = At<zencodec::CodecError>;
    type Enc = HdrEncoder;
    type AnimationFrameEnc = ();

    fn with_pulse(mut self, pulse: zencodec::SharedPulse) -> Self {
        self.pulse = pulse;
        self
    }

    fn with_metadata(self, _meta: Metadata) -> Self {
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(limits);
        self
    }

    fn encoder(self) -> Result<HdrEncoder, Self::Error> {
        Ok(HdrEncoder {
            config: self.config,
            limits: self.limits,
            pulse: self.pulse,
            stream: None,
            sent: 0,
            begun: false,
        })
    }

    fn animation_frame_encoder(self) -> Result<(), Self::Error> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationEncode).into())
    }
}

// ── HdrEncoder ───────────────────────────────────────────────────

/// Single-image HDR encoder.
pub struct HdrEncoder {
    config: HdrEncoderConfig,
    limits: Option<ResourceLimits>,
    pulse: zencodec::SharedPulse,
    stream: Option<super::row_stream::RowStream>,
    /// Bytes handed out through `consume_output` (whole-file output limit).
    sent: u64,
    /// `begin` succeeded.
    begun: bool,
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

fn hdr_file(desc: PixelDescriptor) -> Result<super::row_stream::RowFile, At<zencodec::CodecError>> {
    Ok(super::row_stream::RowFile::Hdr(pixel_slice_to_hdr_layout(
        desc,
    )?))
}

fn pixel_slice_to_hdr_layout(
    desc: PixelDescriptor,
) -> Result<crate::PixelLayout, At<zencodec::CodecError>> {
    match (desc.channel_type(), desc.layout()) {
        (ChannelType::F32, ChannelLayout::Rgb) => Ok(crate::PixelLayout::RgbF32),
        (ChannelType::U8, ChannelLayout::Rgb) => Ok(crate::PixelLayout::Rgb8),
        // A well-formed request for a pixel format this encoder does not
        // support — caller-request-origin, routed via the existing
        // zencodec::UnsupportedOperation::PixelFormat axis.
        _ => Err(BitmapError::from(zencodec::UnsupportedOperation::PixelFormat).into()),
    }
}

impl zencodec::encode::Encoder for HdrEncoder {
    type Error = At<zencodec::CodecError>;

    fn reject(op: zencodec::UnsupportedOperation) -> At<zencodec::CodecError> {
        BitmapError::from(op).into()
    }

    fn preferred_strip_height(&self) -> u32 {
        1
    }

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, Self::Error> {
        // The usage paths are exclusive.
        if self.begun || self.stream.is_some() {
            return Err(BitmapError::InvalidData("encode after begin or push_rows".into()).into());
        }
        let limits = self.limits;
        let out = self.encode_inner(pixels)?;
        super::check_output_total(limits.as_ref(), out.data().len() as u64)?;
        Ok(out)
    }

    fn begin(
        &mut self,
        width: u32,
        height: u32,
        descriptor: PixelDescriptor,
    ) -> Result<zencodec::encode::OutputTiming, Self::Error> {
        if self.begun {
            return Err(BitmapError::InvalidData("begin called twice".into()).into());
        }
        let timing = self.begin_inner(width, height, descriptor)?;
        self.begun = true;
        super::check_output_total(self.limits.as_ref(), self.pending_output().len() as u64)?;
        Ok(timing)
    }

    /// Encode a strip as it arrives; after `begin` the bytes stream through
    /// `pending_output`. Byte-identical to `encode` on the same pixels.
    fn push_rows(&mut self, rows: PixelSlice<'_>) -> Result<(), Self::Error> {
        self.push_rows_inner(rows)?;
        let total = self.sent + self.pending_output().len() as u64;
        super::check_output_total(self.limits.as_ref(), total)
    }

    fn pending_output(&self) -> &[u8] {
        self.stream.as_ref().map_or(&[], |s| s.pending())
    }

    fn consume_output(&mut self, n: usize) {
        self.sent += n.min(self.pending_output().len()) as u64;
        if let Some(s) = &mut self.stream {
            s.consume(n);
        }
    }

    fn take_output(&mut self, into: &mut Vec<u8>) {
        self.sent += self.pending_output().len() as u64;
        if let Some(s) = &mut self.stream {
            s.take_into(into);
        }
    }

    fn finish(self) -> Result<EncodeOutput, Self::Error> {
        let (sent, limits) = (self.sent, self.limits);
        let out = self.finish_inner()?;
        super::check_output_total(limits.as_ref(), sent + out.data().len() as u64)?;
        Ok(out)
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
    type Error = At<zencodec::CodecError>;
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

    fn estimate_decode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // HDR working set ≈ output f32 buffer + one bounded RLE scanline (serial).
        super::trivial_decode_resources(image, compute)
    }

    fn job<'a>(self) -> Self::Job<'a> {
        HdrDecodeJob {
            config: self,
            limits: None,
            pulse: zencodec::SharedPulse::default(),
            max_input_bytes: None,
            alloc_pref: AllocPref::CodecDefault,
            policy: None,
        }
    }
}

// ── HdrDecodeJob ─────────────────────────────────────────────────

/// Per-operation HDR decode job.
pub struct HdrDecodeJob {
    config: HdrDecoderConfig,
    limits: Option<Limits>,
    pulse: zencodec::SharedPulse,
    max_input_bytes: Option<u64>,
    /// Allocation-fallibility preference from
    /// [`ResourceLimits::prefer_fallible_allocations`].
    alloc_pref: AllocPref,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for HdrDecodeJob {
    type Error = At<zencodec::CodecError>;
    type Dec = HdrDecoder<'a>;
    type StreamDec = HdrStreamingDecoder;
    type AnimationFrameDec = zencodec::Unsupported<At<zencodec::CodecError>>;

    fn with_pulse(mut self, pulse: zencodec::SharedPulse) -> Self {
        self.pulse = pulse;
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
        let (width, height, _offset) =
            crate::hdr::decode::parse_header(data).map_err(zencodec::CodecError::of)?;
        Ok(hdr_info(width, height))
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, Self::Error> {
        let (width, height, _offset) =
            crate::hdr::decode::parse_header(data).map_err(zencodec::CodecError::of)?;
        Ok(
            OutputInfo::full_decode(width, height, PixelDescriptor::RGBF32_LINEAR)
                .with_alpha(false),
        )
    }

    #[cfg(feature = "inventory")]
    fn inventory(&self, data: &[u8]) -> Result<zencodec::inventory::Inventory, Self::Error> {
        let limits = self.limits.as_ref().or(self.config.limits.as_ref());
        let output = || {
            crate::hdr::decode::parse_header(data)
                .ok()
                .map(|(w, h, _)| (w, h, Some(12)))
        };
        super::job_inventory(
            data,
            &self.pulse,
            self.max_input_bytes,
            limits,
            output,
            crate::inventory::hdr::walk,
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<HdrDecoder<'a>, Self::Error> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))
            .into());
        }
        Ok(HdrDecoder {
            config: self.config,
            limits: self.limits,
            data,
            pulse: self.pulse,
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
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<HdrStreamingDecoder, Self::Error> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))
            .into());
        }
        let (width, height, _offset) =
            crate::hdr::decode::parse_header(&data).map_err(zencodec::CodecError::of)?;

        let limits = self.limits.or(self.config.limits);
        if let Some(ref lim) = limits {
            lim.check(width, height).map_err(zencodec::CodecError::of)?;
        }

        let row_bytes = (width as usize).checked_mul(12).ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::DimensionsTooLarge { width, height })
        })?;

        let total_bytes = row_bytes.checked_mul(height as usize).ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::DimensionsTooLarge { width, height })
        })?;

        crate::limits::check_output_size(total_bytes, limits.as_ref())
            .map_err(zencodec::CodecError::of)?;

        let cicp = zencodec::Cicp::new(1, 8, 0, true);
        let info = ImageInfo::new(width, height, ImageFormat::Hdr)
            .with_alpha(false)
            .with_bit_depth(32)
            .with_channel_count(3)
            .with_cicp(cicp)
            .with_source_encoding_details(BitmapSourceEncoding);

        let stop: &dyn Stop = &self.pulse;
        let decoded =
            crate::hdr::decode_with_alloc_pref(&data, limits.as_ref(), self.alloc_pref, stop)
                .map_err(zencodec::CodecError::of)?;
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
                HdrStrips {
                    limits,
                    alloc_pref: self.alloc_pref,
                    width: 0,
                    scanline: Vec::new(),
                },
                self.pulse,
                self.max_input_bytes,
            ),
        ))
    }
}

fn hdr_info(width: u32, height: u32) -> ImageInfo {
    ImageInfo::new(width, height, ImageFormat::Hdr)
        .with_alpha(false)
        .with_bit_depth(32)
        .with_channel_count(3)
        .with_cicp(zencodec::Cicp::new(1, 8, 0, true))
        .with_source_encoding_details(BitmapSourceEncoding)
}

/// Radiance HDR on the strip decoder: scanlines vary in length (RLE), so
/// each step decodes the scanlines the offered bytes guarantee, with the
/// one-shot scanline decoder.
struct HdrStrips {
    limits: Option<Limits>,
    alloc_pref: AllocPref,
    width: u32,
    /// New-style RLE scratch, `4 * width` bytes, reused across steps.
    scanline: Vec<u8>,
}

impl super::strips::StripFormat for HdrStrips {
    fn geometry(
        &mut self,
        prefix: &[u8],
        at_end: bool,
    ) -> crate::Result<Option<super::strips::Geometry>> {
        use crate::limits;
        // A prefix fails to parse until its resolution line has arrived; only
        // a foreign magic is final before the end of the input.
        let (width, height, offset) = match crate::hdr::decode::parse_header(prefix) {
            Ok(h) => h,
            Err(e) if matches!(e.error(), BitmapError::UnrecognizedFormat) => return Err(e),
            Err(_) if !at_end => return Ok(None),
            Err(e) => return Err(e),
        };
        let lim = self.limits.as_ref();
        limits::check_dimensions(width, height, lim)?;
        let out_bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|px| px.checked_mul(12))
            .ok_or_else(|| {
                whereat::at!(BitmapError::OutOfMemory(
                    "output size overflows usize".into()
                ))
            })?;
        limits::check_output_size(out_bytes, lim)?;
        self.width = width;
        self.scanline =
            crate::alloc_util::alloc_zeroed(self.alloc_pref, false, width as usize * 4)?;
        Ok(Some(super::strips::Geometry {
            width,
            height,
            layout: crate::PixelLayout::RgbF32,
            info: hdr_info(width, height),
            body: super::strips::Body::VarRows {
                data_offset: offset as u64,
                worst_row_bytes: crate::hdr::decode::worst_scanline_bytes(width as usize),
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
        let w = self.width as usize;
        let mut out = crate::alloc_util::alloc_zeroed(self.alloc_pref, true, w * 12 * n as usize)?;
        let mut pos = 0;
        for row_out in out.chunks_exact_mut(w * 12) {
            pos = crate::hdr::decode::decode_scanline(src, pos, w, &mut self.scanline, row_out)?;
        }
        Ok((
            crate::decode::DecodeOutput::owned(out, self.width, n, crate::PixelLayout::RgbF32),
            pos,
        ))
    }
}

// ── HdrDecoder ───────────────────────────────────────────────────

/// Single-image HDR decoder.
pub struct HdrDecoder<'a> {
    config: HdrDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    pulse: zencodec::SharedPulse,
    alloc_pref: AllocPref,
}

impl HdrDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for HdrDecoder<'_> {
    type Error = At<zencodec::CodecError>;

    fn decode(self) -> Result<DecodeOutput, Self::Error> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = &self.pulse;
        let decoded = crate::hdr::decode_with_alloc_pref(&self.data, limits, self.alloc_pref, stop)
            .map_err(zencodec::CodecError::of)?;
        decode_output_from_internal(&decoded, ImageFormat::Hdr).map_err(zencodec::CodecError::of)
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
    type Error = At<zencodec::CodecError>;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, Self::Error> {
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
        .map_err(|inner| {
            At::<zencodec::CodecError>::from(BitmapError::InvalidData(inner.to_string()))
        })?;

        self.current_row += 1;
        Ok(Some((y, slice)))
    }

    fn info(&self) -> &ImageInfo {
        &self.info
    }
}

impl HdrEncoder {
    fn begin_inner(
        &mut self,
        width: u32,
        height: u32,
        descriptor: PixelDescriptor,
    ) -> Result<zencodec::encode::OutputTiming, At<zencodec::CodecError>> {
        if self.stream.is_some() {
            return Err(BitmapError::InvalidData("begin after rows were pushed".into()).into());
        }
        let file = hdr_file(descriptor)?;
        let limits = self.effective_limits();
        let stop: &dyn Stop = &self.pulse;
        self.stream = Some(super::row_stream::RowStream::start(
            file,
            width,
            Some(height),
            descriptor,
            limits,
            stop,
        )?);
        // Each strip is written as it arrives.
        Ok(zencodec::encode::OutputTiming::streaming(0))
    }

    fn push_rows_inner(&mut self, rows: PixelSlice<'_>) -> Result<(), At<zencodec::CodecError>> {
        let stop: &dyn Stop = &self.pulse;
        if self.stream.is_none() {
            let file = hdr_file(rows.descriptor())?;
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

    fn finish_inner(self) -> Result<EncodeOutput, At<zencodec::CodecError>> {
        let stop: &dyn Stop = &self.pulse;
        let stream = self.stream.ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::InvalidData(
                "finish() without push_rows()".into(),
            ))
        })?;
        Ok(EncodeOutput::new(stream.finish(stop)?, ImageFormat::Hdr))
    }

    fn encode_inner(
        self,
        pixels: PixelSlice<'_>,
    ) -> Result<EncodeOutput, At<zencodec::CodecError>> {
        let stop: &dyn Stop = &self.pulse;
        let w = pixels.width();
        let h = pixels.rows();

        if let Some(limits) = self.effective_limits() {
            limits.check(w, h).map_err(zencodec::CodecError::of)?;
        }

        let layout = pixel_slice_to_hdr_layout(pixels.descriptor())?;
        let bytes = pixels.contiguous_bytes();
        let encoded =
            crate::hdr::encode(&bytes, w, h, layout, stop).map_err(zencodec::CodecError::of)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Hdr))
    }
}
