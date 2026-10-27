use super::*;
use whereat::At;
use zencodec::encode::Encoder as _;

use crate::alloc_util::AllocPref;

// ══════════════════════════════════════════════════════════════════════
// QOI capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static QOI_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_native_alpha(true)
    .with_stop(true)
    .with_push_rows(true)
    .with_streaming_output(true)
    .with_enforces_max_pixels(true);

static QOI_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_native_alpha(true)
    .with_streaming(true)
    .with_incremental(true)
    .with_stop(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true)
    .with_inventory(cfg!(feature = "inventory"));

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
    type Error = At<zencodec::CodecError>;
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
            pulse: zencodec::SharedPulse::default(),
        }
    }
}

// ── QoiEncodeJob ─────────────────────────────────────────────────

/// Per-operation QOI encode job.
pub struct QoiEncodeJob {
    config: QoiEncoderConfig,
    limits: Option<ResourceLimits>,
    pulse: zencodec::SharedPulse,
}

impl zencodec::encode::EncodeJob for QoiEncodeJob {
    type Error = At<zencodec::CodecError>;
    type Enc = QoiEncoder;
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

    fn encoder(self) -> Result<QoiEncoder, Self::Error> {
        Ok(QoiEncoder {
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

// ── QoiEncoder ───────────────────────────────────────────────────

/// Streaming QOI encode state: the kernel's running index, previous
/// pixel and pending run carried across `push_rows` calls, so the bytes
/// are identical to a one-shot encode (the run is flushed once, at
/// `finish`).
struct QoiStreamState {
    out: Vec<u8>,
    index: [[u8; 4]; 64],
    px_rgb: [u8; 3],
    px_rgba: [u8; 4],
    run: usize,
    width: u32,
    rows: u32,
    channels: usize,
    needs_swizzle: bool,
    /// Scratch for BGRA input, swizzled to RGBA before encoding.
    swizzle_buf: Vec<u8>,
    /// Bytes of `out` that hold encoded data; `out[len..]` is headroom the
    /// next strip encodes into directly (the kernel overwrites every byte
    /// it reports, so stale headroom is fine).
    len: usize,
    /// `out[..drained]` was handed out through `pending_output`.
    drained: usize,
    /// Height declared by `begin`. With it the header is final from the
    /// start and output streams; without it the height is patched in at
    /// `finish`, so nothing is released before then.
    declared_height: Option<u32>,
    limits: Option<Limits>,
}

/// Single-image QOI encoder.
pub struct QoiEncoder {
    config: QoiEncoderConfig,
    limits: Option<ResourceLimits>,
    pulse: zencodec::SharedPulse,
    stream: Option<QoiStreamState>,
    /// Bytes handed out through `consume_output` (whole-file output limit).
    sent: u64,
    /// `begin` succeeded.
    begun: bool,
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
    type Error = At<zencodec::CodecError>;

    fn reject(op: zencodec::UnsupportedOperation) -> At<zencodec::CodecError> {
        BitmapError::from(op).into()
    }

    fn preferred_strip_height(&self) -> u32 {
        16
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

    /// Encode `rows` as they arrive. The first call fixes width and
    /// channel count and writes the header with a placeholder height,
    /// patched at `finish`. Output grows by at most `pixels * (channels + 1)`
    /// bytes per call; the kernel's run state carries across calls, so the
    /// result is byte-identical to `encode`.
    fn push_rows(&mut self, rows: PixelSlice<'_>) -> Result<(), Self::Error> {
        self.push_rows_inner(rows)?;
        let total = self.sent + self.pending_output().len() as u64;
        super::check_output_total(self.limits.as_ref(), total)
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

    fn pending_output(&self) -> &[u8] {
        match &self.stream {
            Some(st) if st.declared_height.is_some() => &st.out[st.drained..st.len],
            _ => &[],
        }
    }

    fn consume_output(&mut self, n: usize) {
        self.sent += n.min(self.pending_output().len()) as u64;
        let Some(st) = self.stream.as_mut() else {
            return;
        };
        if st.declared_height.is_none() {
            return;
        }
        st.drained = (st.drained + n).min(st.len);
        if st.drained == st.len {
            // Everything sent: encode the next strip from the start.
            st.len = 0;
            st.drained = 0;
        } else if st.drained >= 64 * 1024 && st.drained > st.len / 2 {
            st.out.copy_within(st.drained..st.len, 0);
            st.len -= st.drained;
            st.drained = 0;
        }
    }

    /// Hands over the output buffer itself when `into` is empty and
    /// nothing was partly consumed; the next strip then encodes into a new
    /// buffer.
    fn take_output(&mut self, into: &mut Vec<u8>) {
        let n = self.pending_output().len();
        if n == 0 {
            return;
        }
        self.sent += n as u64;
        let st = self
            .stream
            .as_mut()
            .expect("bytes pending only while streaming");
        if into.is_empty() && st.drained == 0 {
            st.out.truncate(st.len);
            core::mem::swap(into, &mut st.out);
            st.out.clear();
        } else {
            into.extend_from_slice(&st.out[st.drained..st.len]);
        }
        st.len = 0;
        st.drained = 0;
    }

    fn finish(self) -> Result<EncodeOutput, Self::Error> {
        let (sent, limits) = (self.sent, self.limits);
        let out = self.finish_inner()?;
        super::check_output_total(limits.as_ref(), sent + out.data().len() as u64)?;
        Ok(out)
    }
}

/// Encode one strip. The kernel runs on stack copies of its state, as in
/// the one-shot encoder, so the compiler keeps the previous pixel and run
/// count in registers; the state is written back afterwards. Kept out of
/// line so the kernel's code generation does not depend on `push_rows`
/// around it: inlined there, 16-row strips of 2048-wide photos measured
/// +11.0 to +13.2% against the one-shot encoder; out of line, +0.6 to +2.6%
/// (benches/qoi_photos.rs, M4 Pro, 2026-10-14).
#[inline(never)]
fn encode_strip<const N: usize>(
    index: &mut [[u8; 4]; 64],
    px: &mut [u8; N],
    run: &mut usize,
    pixels: &[u8],
    dst: &mut [u8],
) -> Result<usize, BitmapError>
where
    [u8; N]: crate::qoi::rapid_qoi::Pixel,
{
    let mut i = *index;
    let mut p = *px;
    let mut r = *run;
    let res =
        crate::qoi::rapid_qoi::Qoi::encode_range::<N>(&mut i, &mut p, &mut r, pixels, dst, false);
    *index = i;
    *px = p;
    *run = r;
    res.map_err(|e| BitmapError::InvalidData(e.to_string()))
}

fn total_rows_would_exceed(done: u32, more: u32, declared: u32) -> bool {
    done.checked_add(more).is_none_or(|t| t > declared)
}

impl QoiEncoder {
    /// Write the header and set up the strip state. `height` is the
    /// declared height, or `None` for a placeholder patched at `finish`.
    fn start_stream(
        &mut self,
        width: u32,
        desc: PixelDescriptor,
        height: Option<u32>,
    ) -> Result<(), At<zencodec::CodecError>> {
        use crate::qoi::rapid_qoi::{QOI_HEADER_SIZE, QOI_MAGIC};
        let channels: usize = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U8, ChannelLayout::Rgb) => 3,
            (ChannelType::U8, ChannelLayout::Rgba | ChannelLayout::Bgra) => 4,
            _ => {
                return Err(<Self as zencodec::encode::Encoder>::reject(
                    zencodec::UnsupportedOperation::PixelFormat,
                ));
            }
        };
        let mut out = Vec::with_capacity(QOI_HEADER_SIZE + 64 * 1024);
        out.extend_from_slice(&QOI_MAGIC.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&height.unwrap_or(0).to_be_bytes());
        out.push(channels as u8);
        out.push(0); // sRGB colour space, as the one-shot path writes
        let len = out.len();
        let limits = self.effective_limits();
        self.stream = Some(QoiStreamState {
            out,
            index: [[0; 4]; 64],
            px_rgb: [0, 0, 0],
            px_rgba: [0, 0, 0, 255],
            run: 0,
            width,
            rows: 0,
            channels,
            needs_swizzle: desc.layout() == ChannelLayout::Bgra,
            swizzle_buf: Vec::new(),
            len,
            drained: 0,
            declared_height: height,
            limits,
        });
        Ok(())
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
    type Error = At<zencodec::CodecError>;
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

    fn estimate_decode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // QOI working set ≈ output buffer only (serial; runs decode in place).
        super::trivial_decode_resources(image, compute)
    }

    fn job<'a>(self) -> Self::Job<'a> {
        QoiDecodeJob {
            config: self,
            limits: None,
            pulse: zencodec::SharedPulse::default(),
            max_input_bytes: None,
            alloc_pref: AllocPref::CodecDefault,
            policy: None,
        }
    }
}

// ── QoiDecodeJob ─────────────────────────────────────────────────

/// Per-operation QOI decode job.
pub struct QoiDecodeJob {
    config: QoiDecoderConfig,
    limits: Option<Limits>,
    pulse: zencodec::SharedPulse,
    max_input_bytes: Option<u64>,
    /// Allocation-fallibility preference from
    /// [`ResourceLimits::prefer_fallible_allocations`].
    alloc_pref: AllocPref,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for QoiDecodeJob {
    type Error = At<zencodec::CodecError>;
    type Dec = QoiDecoder<'a>;
    type StreamDec = QoiStreamingDecoder<'a>;
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
        let hdr = crate::qoi::decode::parse_header(data).map_err(zencodec::CodecError::of)?;
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

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, Self::Error> {
        let hdr = crate::qoi::decode::parse_header(data).map_err(zencodec::CodecError::of)?;
        let (width, height, has_alpha) = (hdr.width, hdr.height, hdr.has_alpha);
        let desc = if has_alpha {
            PixelDescriptor::RGBA8_SRGB
        } else {
            PixelDescriptor::RGB8_SRGB
        };
        Ok(OutputInfo::full_decode(width, height, desc).with_alpha(has_alpha))
    }

    #[cfg(feature = "inventory")]
    fn inventory(&self, data: &[u8]) -> Result<zencodec::inventory::Inventory, Self::Error> {
        let limits = self.limits.as_ref().or(self.config.limits.as_ref());
        let output = || {
            crate::qoi::decode::parse_header(data)
                .ok()
                .map(|h| (h.width, h.height, Some(if h.has_alpha { 4 } else { 3 })))
        };
        super::job_inventory(
            data,
            &self.pulse,
            self.max_input_bytes,
            limits,
            output,
            crate::inventory::qoi::walk,
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<QoiDecoder<'a>, Self::Error> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))
            .into());
        }
        Ok(QoiDecoder {
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
    ) -> Result<QoiStreamingDecoder<'a>, Self::Error> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))
            .into());
        }
        let hdr_info = crate::qoi::decode::parse_header(&data).map_err(zencodec::CodecError::of)?;
        let (width, height, has_alpha) = (hdr_info.width, hdr_info.height, hdr_info.has_alpha);

        let limits = self.limits.or(self.config.limits);
        if let Some(ref lim) = limits {
            lim.check(width, height).map_err(zencodec::CodecError::of)?;
        }

        let channels: usize = if has_alpha { 4 } else { 3 };
        let row_bytes = (width as usize).checked_mul(channels).ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::DimensionsTooLarge { width, height })
        })?;

        crate::limits::check_output_size(
            row_bytes.saturating_mul(height as usize),
            limits.as_ref(),
        )
        .map_err(zencodec::CodecError::of)?;

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
            data, info, descriptor, width, height, has_alpha, row_bytes, self.pulse,
        )
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
        preferred: &[PixelDescriptor],
    ) -> Result<
        alloc::boxed::Box<zencodec::decode::DynIncrementalDecoder>,
        zencodec::decode::BoxedError,
    > {
        let dec = self
            .into_incremental_decoder(preferred)
            .map_err(|e| alloc::boxed::Box::new(e) as zencodec::decode::BoxedError)?;
        Ok(zencodec::decode::erase_incremental(dec))
    }
}

impl QoiDecodeJob {
    /// Create an incremental (sans-I/O) decoder.
    pub fn into_incremental_decoder(
        self,
        _preferred: &[PixelDescriptor],
    ) -> Result<QoiIncrementalDecoder, At<zencodec::CodecError>> {
        let limits = self.limits.or(self.config.limits);
        Ok(QoiIncrementalDecoder {
            pos: 0,
            info: None,
            descriptor: None,
            width: 0,
            height: 0,
            has_alpha: false,
            row_bytes: 0,
            state_rgb: crate::qoi::decode::QoiDecodeState::<3>::new(),
            state_rgba: crate::qoi::decode::QoiDecodeState::<4>::new(),
            next_row: 0,
            done: None,
            failed: false,
            need: [zencodec::decode::InputRequest::at_least(
                0,
                QOI_HEADER_BYTES,
            )],
            pulse: self.pulse,
            limits,
            max_input_bytes: self.max_input_bytes,
        })
    }
}

// ── QoiIncrementalDecoder ────────────────────────────────────────

const QOI_HEADER_BYTES: usize = 14;
/// Rows decoded straight into one sink strip per step, at most.
const QOI_MAX_STRIP_ROWS: u32 = 64;
/// Largest QOI opcode: one `QOI_OP_RGBA`. A row of `width` pixels never
/// needs more than `width * 5` input bytes.
const QOI_MAX_OP_BYTES: usize = 5;

/// Incremental (sans-I/O) QOI decoder.
///
/// It holds no input. Each step decodes, straight from the driver's input
/// into the sink's strip, every row the input is guaranteed to complete:
/// `width * 5` bytes per row (the largest opcode is 5 bytes), or all
/// remaining rows once the input reaches the end of the source. With less
/// than one guaranteed row it takes nothing and asks for one worst-case
/// row from its position, so drivers hand it at least that much and every
/// row decodes in place. The trailing end marker is not required, as in
/// the one-shot decoder.
pub struct QoiIncrementalDecoder {
    /// Absolute offset of the next byte to decode.
    pos: u64,
    info: Option<ImageInfo>,
    descriptor: Option<PixelDescriptor>,
    width: u32,
    height: u32,
    has_alpha: bool,
    row_bytes: usize,

    state_rgb: crate::qoi::decode::QoiDecodeState<3>,
    state_rgba: crate::qoi::decode::QoiDecodeState<4>,

    next_row: u32,
    done: Option<zencodec::decode::OutputInfo>,
    failed: bool,
    need: [zencodec::decode::InputRequest; 1],

    pulse: zencodec::SharedPulse,
    limits: Option<Limits>,
    max_input_bytes: Option<u64>,
}

type QoiStepError = zencodec::decode::StepError<At<zencodec::CodecError>>;

fn codec(e: BitmapError) -> QoiStepError {
    zencodec::decode::StepError::Codec(At::<zencodec::CodecError>::from(e))
}

impl QoiIncrementalDecoder {
    fn worst_row(&self) -> usize {
        self.width as usize * QOI_MAX_OP_BYTES
    }

    /// Ask for `min_len` bytes from the current position.
    fn need(&mut self, min_len: usize) -> zencodec::decode::Progress {
        self.need = [zencodec::decode::InputRequest::at_least(self.pos, min_len)];
        zencodec::decode::Progress::need(0)
    }

    fn parse_header(
        &mut self,
        bytes: &[u8],
        sink: &mut dyn zencodec::decode::DecodeRowSink,
    ) -> Result<(), QoiStepError> {
        let hdr = crate::qoi::decode::parse_header(&bytes[..QOI_HEADER_BYTES])
            .map_err(|e| zencodec::decode::StepError::Codec(zencodec::CodecError::of(e)))?;
        let (width, height, has_alpha) = (hdr.width, hdr.height, hdr.has_alpha);
        if let Some(lim) = &self.limits {
            lim.check(width, height)
                .map_err(|e| zencodec::decode::StepError::Codec(zencodec::CodecError::of(e)))?;
        }
        let channels: usize = if has_alpha { 4 } else { 3 };
        let row_bytes = (width as usize)
            .checked_mul(channels)
            .ok_or_else(|| codec(BitmapError::DimensionsTooLarge { width, height }))?;
        crate::limits::check_output_size(
            row_bytes.saturating_mul(height as usize),
            self.limits.as_ref(),
        )
        .map_err(|e| zencodec::decode::StepError::Codec(zencodec::CodecError::of(e)))?;

        // Same labelling as the one-shot path (`layout_to_pixel_buffer`
        // builds from typed `rgb` pixels, whose descriptor leaves the
        // transfer unknown), so every decode path agrees on the descriptor.
        let descriptor = if has_alpha {
            PixelDescriptor::RGBA8
        } else {
            PixelDescriptor::RGB8
        };
        self.info = Some(
            ImageInfo::new(width, height, ImageFormat::Qoi)
                .with_alpha(has_alpha)
                .with_bit_depth(8)
                .with_channel_count(channels as u8)
                .with_source_encoding_details(BitmapSourceEncoding),
        );
        self.width = width;
        self.height = height;
        self.has_alpha = has_alpha;
        self.row_bytes = row_bytes;
        self.descriptor = Some(descriptor);
        sink.begin(width, height, descriptor)
            .map_err(zencodec::decode::StepError::Sink)
    }

    /// Advance past `n` decoded bytes, enforcing the input limit.
    fn advance(&mut self, n: usize) -> Result<(), QoiStepError> {
        self.pos += n as u64;
        if let Some(max) = self.max_input_bytes
            && self.pos > max
        {
            let total = self.pos;
            return Err(codec(BitmapError::LimitExceeded(alloc::format!(
                "input size {total} exceeds limit {max}"
            ))));
        }
        Ok(())
    }

    fn step_inner(
        &mut self,
        input: zencodec::decode::Input<'_>,
        sink: &mut dyn zencodec::decode::DecodeRowSink,
        pulse: &dyn zencodec::Pulse,
    ) -> Result<zencodec::decode::Progress, QoiStepError> {
        use zencodec::decode::{BatchMeta, Event, MetadataUpdate, Progress};

        pulse.check()?;
        self.pulse.check()?;
        // Only input that starts where decoding stands is usable.
        let here: &[u8] = if input.offset == self.pos {
            input.bytes
        } else {
            &[]
        };
        let at_end = input.offset == self.pos && input.reaches_end()
            || input.total_len.is_some_and(|t| t <= self.pos);

        if self.info.is_none() {
            if here.len() >= QOI_HEADER_BYTES {
                self.parse_header(here, sink)?;
                self.advance(QOI_HEADER_BYTES)?;
                return Ok(Progress::new(
                    QOI_HEADER_BYTES,
                    Event::Metadata(MetadataUpdate::HeaderReady),
                ));
            }
            if at_end {
                return Err(codec(BitmapError::UnexpectedEof));
            }
            return Ok(self.need(QOI_HEADER_BYTES));
        }
        let desc = self.descriptor.expect("set with info");
        let (width, height, row_bytes) = (self.width, self.height, self.row_bytes);

        if self.next_row >= height {
            sink.finish().map_err(zencodec::decode::StepError::Sink)?;
            let info = zencodec::decode::OutputInfo::full_decode(width, height, desc);
            self.done = Some(info.clone());
            return Ok(Progress::new(0, Event::Done(info)));
        }

        let remaining = height - self.next_row;
        let guaranteed = if at_end {
            remaining
        } else {
            (here.len() / self.worst_row()).min(remaining as usize) as u32
        };
        let rows = guaranteed.min(QOI_MAX_STRIP_ROWS);
        if rows == 0 {
            if at_end {
                return Err(codec(BitmapError::UnexpectedEof));
            }
            return Ok(self.need(self.worst_row()));
        }

        // Every row of this strip completes from `here` (or `here` is the
        // whole rest of the source): decode straight into the sink.
        let y = self.next_row;
        let mut used = 0usize;
        {
            let mut dst = sink
                .provide_next_buffer(y, rows, width, desc)
                .map_err(zencodec::decode::StepError::Sink)?;
            for r in 0..rows {
                let out = &mut dst.row_mut(r)[..row_bytes];
                let decoded = if self.has_alpha {
                    self.state_rgba.decode_into(&here[used..], out)
                } else {
                    self.state_rgb.decode_into(&here[used..], out)
                };
                match decoded {
                    Ok(c) => used += c,
                    // Only reachable at the end of a truncated stream: the
                    // input bound guarantees completion otherwise.
                    Err(()) => return Err(codec(BitmapError::UnexpectedEof)),
                }
            }
        }
        self.advance(used)?;
        self.next_row += rows;
        Ok(Progress::new(
            used,
            Event::Rows {
                meta: BatchMeta::final_rows(y),
                rows,
            },
        ))
    }
}

impl zencodec::decode::IncrementalDecode for QoiIncrementalDecoder {
    type Error = At<zencodec::CodecError>;

    fn step(
        &mut self,
        input: zencodec::decode::Input<'_>,
        sink: &mut dyn zencodec::decode::DecodeRowSink,
        pulse: &dyn zencodec::Pulse,
    ) -> Result<zencodec::decode::Progress, QoiStepError> {
        if let Some(done) = &self.done {
            return Ok(zencodec::decode::Progress::new(
                0,
                zencodec::decode::Event::Done(done.clone()),
            ));
        }
        if self.failed {
            return Err(zencodec::decode::StepError::Finished);
        }
        let r = self.step_inner(input, sink, pulse);
        self.failed = r.is_err();
        r
    }

    fn requested(&self) -> &[zencodec::decode::InputRequest] {
        &self.need
    }

    fn info(&self) -> Option<&ImageInfo> {
        self.info.as_ref()
    }

    fn output_descriptor(&self) -> Option<PixelDescriptor> {
        self.descriptor
    }

    fn access_pattern(&self) -> zencodec::source::AccessPattern {
        zencodec::source::AccessPattern::Sequential
    }
}

// ── QoiDecoder ───────────────────────────────────────────────────

/// Single-image QOI decoder.
pub struct QoiDecoder<'a> {
    config: QoiDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    pulse: zencodec::SharedPulse,
    alloc_pref: AllocPref,
}

impl QoiDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for QoiDecoder<'_> {
    type Error = At<zencodec::CodecError>;

    fn decode(self) -> Result<DecodeOutput, Self::Error> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = &self.pulse;
        let decoded = crate::qoi::decode_with_alloc_pref(&self.data, limits, self.alloc_pref, stop)
            .map_err(zencodec::CodecError::of)?;
        decode_output_from_internal(&decoded, ImageFormat::Qoi).map_err(zencodec::CodecError::of)
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
    pulse: zencodec::SharedPulse,
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
        pulse: zencodec::SharedPulse,
    ) -> Result<Self, At<zencodec::CodecError>> {
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
            pulse,
        })
    }
}

impl zencodec::decode::StreamingDecode for QoiStreamingDecoder<'_> {
    type Error = At<zencodec::CodecError>;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, Self::Error> {
        if self.current_row >= self.height {
            return Ok(None);
        }

        self.pulse
            .check()
            .map_err(|r| At::<zencodec::CodecError>::from(BitmapError::from(r)))?;

        let encoded = self
            .data
            .get(self.byte_offset..)
            .ok_or_else(|| At::<zencodec::CodecError>::from(BitmapError::UnexpectedEof))?;

        if self.has_alpha {
            let consumed = self
                .state_rgba
                .decode_into(encoded, &mut self.row_buf)
                .map_err(|()| At::<zencodec::CodecError>::from(BitmapError::UnexpectedEof))?;
            self.byte_offset += consumed;
        } else {
            let consumed = self
                .state_rgb
                .decode_into(encoded, &mut self.row_buf)
                .map_err(|()| At::<zencodec::CodecError>::from(BitmapError::UnexpectedEof))?;
            self.byte_offset += consumed;
        }

        let y = self.current_row;
        self.current_row += 1;
        let stride = self.row_buf.len();
        let slice = PixelSlice::new(&self.row_buf, self.width, 1, stride, self.descriptor)
            .map_err(|inner| {
                At::<zencodec::CodecError>::from(BitmapError::InvalidData(inner.to_string()))
            })?;

        Ok(Some((y, slice)))
    }

    fn info(&self) -> &ImageInfo {
        &self.info
    }
}

impl QoiEncoder {
    fn begin_inner(
        &mut self,
        width: u32,
        height: u32,
        descriptor: PixelDescriptor,
    ) -> Result<zencodec::encode::OutputTiming, At<zencodec::CodecError>> {
        if self.stream.is_some() {
            return Err(BitmapError::InvalidData("begin after rows were pushed".into()).into());
        }
        if let Some(limits) = self.effective_limits() {
            limits
                .check(width, height)
                .map_err(zencodec::CodecError::of)?;
        }
        self.start_stream(width, descriptor, Some(height))?;
        // Each pixel is coded as it arrives; only an open run waits.
        Ok(zencodec::encode::OutputTiming::streaming(0))
    }

    fn push_rows_inner(&mut self, rows: PixelSlice<'_>) -> Result<(), At<zencodec::CodecError>> {
        let desc = rows.descriptor();
        let channels: usize = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U8, ChannelLayout::Rgb) => 3,
            (ChannelType::U8, ChannelLayout::Rgba) => 4,
            (ChannelType::U8, ChannelLayout::Bgra) => 4,
            _ => {
                return Err(Self::reject(zencodec::UnsupportedOperation::PixelFormat));
            }
        };
        self.pulse
            .check()
            .map_err(|r| At::<zencodec::CodecError>::from(BitmapError::from(r)))?;
        if self.stream.is_none() {
            self.start_stream(rows.width(), desc, None)?;
        }
        let st = self.stream.as_mut().expect("started above");
        if st.width != rows.width()
            || st.channels != channels
            || st.needs_swizzle != (desc.layout() == ChannelLayout::Bgra)
        {
            return Err(BitmapError::InvalidData(
                "push_rows: width or pixel layout changed".into(),
            )
            .into());
        }
        if let Some(declared) = st.declared_height
            && total_rows_would_exceed(st.rows, rows.rows(), declared)
        {
            return Err(BitmapError::InvalidData(alloc::format!(
                "push_rows: more than the {declared} rows declared at begin"
            ))
            .into());
        }
        let total_rows = st.rows.checked_add(rows.rows()).ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::DimensionsTooLarge {
                width: st.width,
                height: u32::MAX,
            })
        })?;
        if let Some(limits) = &st.limits {
            limits
                .check(st.width, total_rows)
                .map_err(zencodec::CodecError::of)?;
        }

        let bytes = rows.contiguous_bytes();
        let pixels: &[u8] = if st.needs_swizzle {
            st.swizzle_buf.clear();
            st.swizzle_buf.extend_from_slice(&bytes);
            crate::swizzle::swap_rb_4(&mut st.swizzle_buf);
            &st.swizzle_buf
        } else {
            &bytes
        };
        let px_count = pixels.len() / channels;
        // Worst case per pixel is one RGBA opcode (5 bytes) plus the
        // kernel's 8-byte headroom. Encode straight into `out`'s zeroed
        // headroom; when it runs short, move to a fresh zeroed buffer of at
        // least double the size (calloc'd pages arrive zeroed for free, so
        // growth costs one copy of the data so far and no memset).
        let worst = px_count * (channels + 1) + 8;
        let needed = st.len + worst;
        if st.out.len() < needed {
            let new_len = needed.max(st.out.len().saturating_mul(2));
            let mut grown = alloc::vec![0u8; new_len];
            grown[..st.len].copy_from_slice(&st.out[..st.len]);
            st.out = grown;
        }
        let dst = &mut st.out[st.len..st.len + worst];
        let written = if channels == 4 {
            encode_strip::<4>(&mut st.index, &mut st.px_rgba, &mut st.run, pixels, dst)
        } else {
            encode_strip::<3>(&mut st.index, &mut st.px_rgb, &mut st.run, pixels, dst)
        }
        .map_err(At::<zencodec::CodecError>::from)?;
        st.len += written;
        st.rows = total_rows;
        Ok(())
    }

    fn finish_inner(self) -> Result<EncodeOutput, At<zencodec::CodecError>> {
        use crate::qoi::rapid_qoi::{QOI_OP_RUN, QOI_PADDING};

        let mut st = self.stream.ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::InvalidData(
                "finish() without push_rows()".into(),
            ))
        })?;
        st.out.truncate(st.len);
        if st.run > 0 {
            // The kernel counts repeats; the opcode stores length - 1.
            st.out.push(QOI_OP_RUN | (st.run as u8 - 1));
            st.run = 0;
        }
        st.out.extend_from_slice(&[0u8; QOI_PADDING - 1]);
        st.out.push(1);
        match st.declared_height {
            Some(declared) if declared != st.rows => {
                return Err(BitmapError::InvalidData(alloc::format!(
                    "finish: {} rows pushed, {declared} declared at begin",
                    st.rows
                ))
                .into());
            }
            Some(_) => {}
            None => st.out[8..12].copy_from_slice(&st.rows.to_be_bytes()),
        }
        if st.drained > 0 {
            st.out.drain(..st.drained);
        }
        Ok(EncodeOutput::new(st.out, ImageFormat::Qoi))
    }

    fn encode_inner(
        self,
        pixels: PixelSlice<'_>,
    ) -> Result<EncodeOutput, At<zencodec::CodecError>> {
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
        let stop: &dyn Stop = &self.pulse;
        let desc = pixels.descriptor();
        let w = pixels.width();
        let h = pixels.rows();

        if let Some(limits) = self.effective_limits() {
            limits.check(w, h).map_err(zencodec::CodecError::of)?;
        }

        let bytes = pixels.contiguous_bytes();
        let layout = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U8, ChannelLayout::Rgb) => crate::PixelLayout::Rgb8,
            (ChannelType::U8, ChannelLayout::Rgba) => crate::PixelLayout::Rgba8,
            (ChannelType::U8, ChannelLayout::Bgra) => crate::PixelLayout::Bgra8,
            // A well-formed request for a pixel format this encoder does not
            // support — caller-request-origin, routed via the existing
            // zencodec::UnsupportedOperation::PixelFormat axis.
            _ => {
                return Err(Self::reject(zencodec::UnsupportedOperation::PixelFormat));
            }
        };

        let encoded =
            crate::qoi::encode(&bytes, w, h, layout, stop).map_err(zencodec::CodecError::of)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Qoi))
    }
}
