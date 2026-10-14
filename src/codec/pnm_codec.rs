use super::*;
use whereat::At;

use crate::alloc_util::AllocPref;
use crate::pnm;

// ══════════════════════════════════════════════════════════════════════
// PNM capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static PNM_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_push_rows(true)
    .with_streaming_output(true)
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
    .with_incremental(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true)
    .with_inventory(cfg!(feature = "inventory"));

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
    type Error = At<zencodec::CodecError>;
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

    fn estimate_encode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // Raw binary PNM/PAM/PFM: output ≈ input (no entropy coding).
        trivial_encode_resources(image, compute, 1.0)
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
    type Error = At<zencodec::CodecError>;
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

    fn encoder(self) -> Result<PnmEncoder, Self::Error> {
        Ok(PnmEncoder {
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

// ── PnmEncoder ───────────────────────────────────────────────────────

/// Single-image PNM encoder.
pub struct PnmEncoder {
    config: PnmEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
    stream: Option<PnmStream>,
}

/// Row-level encode state.
enum PnmStream {
    /// PGM/PPM/PAM: rows are encoded as they arrive.
    Rows(super::row_stream::RowStream),
    /// PFM stores rows bottom-up, so the pixels wait for `finish` and go
    /// through `encode`.
    Buffered {
        data: Vec<u8>,
        width: u32,
        rows: u32,
        declared: Option<u32>,
        descriptor: PixelDescriptor,
    },
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

impl PnmEncoder {
    fn stop(&self) -> &dyn Stop {
        match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        }
    }

    fn start_stream(
        &mut self,
        width: u32,
        height: Option<u32>,
        desc: PixelDescriptor,
    ) -> Result<(), At<zencodec::CodecError>> {
        let limits = self.effective_limits();
        let stream = match super::row_stream::pnm_file(desc) {
            Some(file) => PnmStream::Rows(super::row_stream::RowStream::start(
                file,
                width,
                height,
                desc,
                limits,
                self.stop(),
            )?),
            None if matches!(desc.channel_type(), ChannelType::F32) => {
                if let (Some(l), Some(h)) = (&limits, height) {
                    l.check(width, h).map_err(zencodec::CodecError::of)?;
                }
                PnmStream::Buffered {
                    data: Vec::new(),
                    width,
                    rows: 0,
                    declared: height,
                    descriptor: desc,
                }
            }
            None => {
                return Err(<Self as zencodec::encode::Encoder>::reject(
                    zencodec::UnsupportedOperation::PixelFormat,
                ));
            }
        };
        self.stream = Some(stream);
        Ok(())
    }
}

impl zencodec::encode::Encoder for PnmEncoder {
    type Error = At<zencodec::CodecError>;

    fn reject(op: zencodec::UnsupportedOperation) -> At<zencodec::CodecError> {
        BitmapError::from(op).into()
    }

    fn begin(
        &mut self,
        width: u32,
        height: u32,
        descriptor: PixelDescriptor,
    ) -> Result<zencodec::encode::OutputTiming, Self::Error> {
        if self.stream.is_some() {
            return Err(BitmapError::InvalidData("begin after rows were pushed".into()).into());
        }
        self.start_stream(width, Some(height), descriptor)?;
        Ok(match self.stream {
            // PGM/PPM/PAM: each strip is written as it arrives.
            Some(PnmStream::Rows(_)) => zencodec::encode::OutputTiming::streaming(0),
            // PFM is stored bottom-up: nothing can be written before the last row.
            _ => zencodec::encode::OutputTiming::AT_FINISH,
        })
    }

    /// Encode a strip as it arrives (PGM/PPM/PAM; PFM waits for `finish`).
    /// Unlike `encode`, rows are written in the layout given: whether alpha
    /// is dead cannot be known until the last row.
    fn push_rows(&mut self, rows: PixelSlice<'_>) -> Result<(), Self::Error> {
        if self.stream.is_none() {
            self.start_stream(rows.width(), None, rows.descriptor())?;
        }
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        match self.stream.as_mut().expect("started above") {
            PnmStream::Rows(s) => s.push(rows, stop),
            PnmStream::Buffered {
                data,
                width,
                rows: n,
                declared,
                descriptor,
            } => {
                if rows.width() != *width || rows.descriptor() != *descriptor {
                    return Err(BitmapError::InvalidData(
                        "push_rows: width or pixel format changed".into(),
                    )
                    .into());
                }
                let total = n.saturating_add(rows.rows());
                if declared.is_some_and(|d| total > d) {
                    return Err(BitmapError::InvalidData(
                        "push_rows: more rows than declared at begin".into(),
                    )
                    .into());
                }
                let row_bytes = *width as usize * descriptor.bytes_per_pixel();
                for y in 0..rows.rows() {
                    data.extend_from_slice(&rows.row(y)[..row_bytes]);
                }
                *n = total;
                Ok(())
            }
        }
    }

    fn pending_output(&self) -> &[u8] {
        match &self.stream {
            Some(PnmStream::Rows(s)) => s.pending(),
            _ => &[],
        }
    }

    fn consume_output(&mut self, n: usize) {
        if let Some(PnmStream::Rows(s)) = &mut self.stream {
            s.consume(n);
        }
    }

    fn finish(mut self) -> Result<EncodeOutput, Self::Error> {
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        match self.stream.take() {
            None => Err(BitmapError::InvalidData("finish() without push_rows()".into()).into()),
            Some(PnmStream::Rows(s)) => Ok(EncodeOutput::new(s.finish(stop)?, ImageFormat::Pnm)),
            Some(PnmStream::Buffered {
                data,
                width,
                rows,
                declared,
                descriptor,
            }) => {
                if declared.is_some_and(|d| d != rows) {
                    return Err(BitmapError::InvalidData(
                        "finish: rows pushed differ from the height declared at begin".into(),
                    )
                    .into());
                }
                let row_bytes = width as usize * descriptor.bytes_per_pixel();
                let slice =
                    PixelSlice::new(&data, width, rows, row_bytes, descriptor).map_err(|e| {
                        At::<zencodec::CodecError>::from(BitmapError::InvalidData(e.to_string()))
                    })?;
                self.encode(slice)
            }
        }
    }

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, Self::Error> {
        // Bit-exact load-bearing narrowing (dead alpha / chroma-free /
        // replicated-low-bits) before format mapping — see
        // `super::reduce_for_raw_encode`. PNM encodes grayscale, so every
        // narrowing target is encodable and the reduction is always safe.
        let reduced = super::reduce_for_raw_encode(&pixels, |_| true);
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
            limits.check(w, h).map_err(zencodec::CodecError::of)?;
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
                )
                .map_err(zencodec::CodecError::of)?;
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
                )
                .map_err(zencodec::CodecError::of)?;
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
                )
                .map_err(zencodec::CodecError::of)?;
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
                )
                .map_err(zencodec::CodecError::of)?;
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
                )
                .map_err(zencodec::CodecError::of)?;
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
                )
                .map_err(zencodec::CodecError::of)?;
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
                )
                .map_err(zencodec::CodecError::of)?;
                Ok(EncodeOutput::new(encoded, ImageFormat::Pnm))
            }
            // A well-formed request for a pixel format this encoder does not
            // support — caller-request-origin, routed via the existing
            // zencodec::UnsupportedOperation::PixelFormat axis.
            _ => Err(Self::reject(zencodec::UnsupportedOperation::PixelFormat)),
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
    type Error = At<zencodec::CodecError>;
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

    fn estimate_decode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // PNM working set ≈ output buffer only (serial, no entropy coder).
        super::trivial_decode_resources(image, compute)
    }

    fn job<'a>(self) -> Self::Job<'a> {
        PnmDecodeJob {
            config: self,
            limits: None,
            stop: None,
            max_input_bytes: None,
            alloc_pref: AllocPref::CodecDefault,
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
    /// Allocation-fallibility preference, taken from
    /// [`ResourceLimits::prefer_fallible_allocations`] at the decode boundary
    /// (`CodecDefault` until `with_limits` is called).
    alloc_pref: AllocPref,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for PnmDecodeJob {
    type Error = At<zencodec::CodecError>;
    type Dec = PnmDecoder<'a>;
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
        let header = pnm::decode::parse_header(data).map_err(zencodec::CodecError::of)?;
        Ok(header_to_image_info(&header))
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, Self::Error> {
        let header = pnm::decode::parse_header(data).map_err(zencodec::CodecError::of)?;
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

    #[cfg(feature = "inventory")]
    fn inventory(&self, data: &[u8]) -> Result<zencodec::inventory::Inventory, Self::Error> {
        let limits = self.limits.as_ref().or(self.config.limits.as_ref());
        let output = || {
            pnm::decode::parse_header(data).ok().map(|h| {
                let binary_8bit = !matches!(data.get(1), Some(b'1' | b'2' | b'3'))
                    && h.maxval == 255
                    && matches!(
                        h.format,
                        pnm::PnmFormat::Pgm | pnm::PnmFormat::Ppm | pnm::PnmFormat::Pam
                    );
                let bpp = match h.format {
                    pnm::PnmFormat::Pbm => Some(1),
                    pnm::PnmFormat::Pfm => Some(h.depth as usize * 4),
                    // Binary 8-bit data is borrowed, so the decoder skips the output-size check.
                    _ if binary_8bit => None,
                    _ => Some(h.depth as usize),
                };
                (h.width, h.height, bpp)
            })
        };
        super::job_inventory(
            data,
            self.stop.as_ref(),
            self.max_input_bytes,
            limits,
            output,
            crate::inventory::pnm::walk,
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<PnmDecoder<'a>, Self::Error> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))
            .into());
        }
        Ok(PnmDecoder {
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
        let (stop, max_input_bytes, alloc_pref) =
            (self.stop.clone(), self.max_input_bytes, self.alloc_pref);
        Ok(zencodec::decode::erase_incremental(
            super::strips::StripDecoder::new(
                PnmStrips {
                    limits,
                    alloc_pref,
                    header: None,
                    job: Some(self),
                },
                stop,
                max_input_bytes,
            ),
        ))
    }
}

// ── Incremental decode ───────────────────────────────────────────────

/// PNM on the strip decoder: binary P4-P7 and PFM decode a strip at a time
/// with the one-shot row functions; ASCII P1-P3 decode from the whole file.
struct PnmStrips {
    limits: Option<Limits>,
    alloc_pref: AllocPref,
    header: Option<pnm::PnmHeader>,
    /// For the whole-file path.
    job: Option<PnmDecodeJob>,
}

impl super::strips::StripFormat for PnmStrips {
    fn geometry(
        &mut self,
        prefix: &[u8],
        at_end: bool,
    ) -> crate::Result<Option<super::strips::Geometry>> {
        use super::strips::{Body, Geometry};
        use crate::limits;
        if prefix.len() < 3 {
            return if at_end {
                Err(whereat::at!(BitmapError::UnexpectedEof))
            } else {
                Ok(None)
            };
        }
        match &prefix[..2] {
            b"P1" | b"P2" | b"P3" | b"P4" | b"P5" | b"P6" | b"P7" | b"Pf" | b"PF" => {}
            _ => return Err(whereat::at!(BitmapError::UnrecognizedFormat)),
        }
        let header = match pnm::decode::parse_header(prefix) {
            Ok(h) if h.data_offset <= prefix.len() => h,
            Ok(_) if !at_end => return Ok(None),
            Ok(_) => return Err(whereat::at!(BitmapError::UnexpectedEof)),
            Err(e) if !at_end && matches!(e.error(), BitmapError::UnexpectedEof) => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let limits = self.limits.as_ref();
        limits::check_dimensions(header.width, header.height, limits)?;
        let (w, h, depth) = (
            header.width as usize,
            header.height as usize,
            header.depth as usize,
        );
        let too_large = || {
            whereat::at!(BitmapError::DimensionsTooLarge {
                width: header.width,
                height: header.height,
            })
        };
        let ascii = matches!(prefix[1], b'1' | b'2' | b'3');
        let data_offset = header.data_offset as u64;
        let body = match header.format {
            _ if ascii => Body::Whole,
            pnm::PnmFormat::Pbm => {
                limits::check_output_size(w.checked_mul(h).ok_or_else(too_large)?, limits)?;
                Body::Rows {
                    data_offset,
                    row_bytes: w.div_ceil(8),
                    bottom_up: false,
                }
            }
            pnm::PnmFormat::Pfm => {
                let out = w
                    .checked_mul(h)
                    .and_then(|wh| wh.checked_mul(depth))
                    .and_then(|whd| whd.checked_mul(4))
                    .ok_or_else(too_large)?;
                limits::check_output_size(out, limits)?;
                Body::Rows {
                    data_offset,
                    row_bytes: w.checked_mul(depth * 4).ok_or_else(too_large)?,
                    bottom_up: true,
                }
            }
            pnm::PnmFormat::Pgm | pnm::PnmFormat::Ppm | pnm::PnmFormat::Pam => {
                let is_16bit = header.maxval > 255;
                let src_bps = if is_16bit { 2 } else { 1 };
                // The 8-bit maxval-255 path borrows the input and skips the
                // output-size check, as the one-shot decoder does.
                if is_16bit || header.maxval != 255 {
                    let out = w
                        .checked_mul(h)
                        .and_then(|wh| wh.checked_mul(depth))
                        .ok_or_else(too_large)?;
                    limits::check_output_size(out, limits)?;
                }
                Body::Rows {
                    data_offset,
                    row_bytes: w
                        .checked_mul(depth)
                        .and_then(|wd| wd.checked_mul(src_bps))
                        .ok_or_else(too_large)?,
                    bottom_up: false,
                }
            }
        };
        let layout = match header.format {
            pnm::PnmFormat::Pbm => crate::PixelLayout::Gray8,
            _ => header.layout,
        };
        let info = super::header_to_image_info(&header);
        let geometry = Geometry {
            width: header.width,
            height: header.height,
            layout,
            info,
            body,
        };
        self.header = Some(header);
        Ok(Some(geometry))
    }

    fn decode_rows<'s>(
        &mut self,
        src: &'s [u8],
        n: u32,
        stop: &dyn Stop,
    ) -> crate::Result<(crate::decode::DecodeOutput<'s>, usize)> {
        let header = self.header.as_ref().expect("geometry first");
        // The one-shot row functions, run on a strip as an n-row image.
        let mut strip = header.clone();
        strip.height = n;
        let w = header.width;
        let out = match header.format {
            pnm::PnmFormat::Pbm => crate::decode::DecodeOutput::owned(
                pnm::decode::decode_p4_bitpacked(src, &strip, self.alloc_pref, stop)?,
                w,
                n,
                crate::PixelLayout::Gray8,
            ),
            pnm::PnmFormat::Pfm => crate::decode::DecodeOutput::owned(
                pnm::decode::decode_pfm(src, &strip, self.alloc_pref, stop)?,
                w,
                n,
                header.layout,
            ),
            _ if header.maxval == 255 => {
                crate::decode::DecodeOutput::borrowed(src, w, n, header.layout)
            }
            _ => crate::decode::DecodeOutput::owned(
                pnm::decode::decode_integer_transform(
                    src,
                    &strip,
                    src.len(),
                    self.alloc_pref,
                    stop,
                )?,
                w,
                n,
                header.layout,
            ),
        };
        Ok((out, src.len()))
    }

    fn decode_whole(
        &mut self,
        file: &[u8],
        _stop: &dyn Stop,
    ) -> Result<DecodeOutput, At<zencodec::CodecError>> {
        use zencodec::decode::{Decode as _, DecodeJob as _};
        let job = self.job.take().ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::InvalidData("decoded twice".into()))
        })?;
        job.decoder(Cow::Borrowed(file), &[])?.decode()
    }
}

// ── PnmDecoder ───────────────────────────────────────────────────────

/// Single-image PNM decoder.
pub struct PnmDecoder<'a> {
    config: PnmDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    stop: Option<zencodec::StopToken>,
    alloc_pref: AllocPref,
}

impl PnmDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for PnmDecoder<'_> {
    type Error = At<zencodec::CodecError>;

    fn decode(self) -> Result<DecodeOutput, Self::Error> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::pnm::decode_with_alloc_pref(&self.data, limits, self.alloc_pref, stop)
            .map_err(zencodec::CodecError::of)?;
        decode_output_from_internal(&decoded, ImageFormat::Pnm).map_err(zencodec::CodecError::of)
    }
}
