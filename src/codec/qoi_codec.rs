use super::*;
use whereat::At;

use crate::alloc_util::AllocPref;

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
    type Error = At<zencodec::CodecError>;
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

    fn encoder(self) -> Result<QoiEncoder, Self::Error> {
        Ok(QoiEncoder {
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
    /// Strip-sized encode scratch at the worst-case size, zeroed once: the
    /// kernel writes before it reads, so only the written prefix is copied
    /// to the output and no per-strip headroom is zero-filled.
    scratch: Vec<u8>,
    limits: Option<Limits>,
}

/// Single-image QOI encoder.
pub struct QoiEncoder {
    config: QoiEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
    stream: Option<QoiStreamState>,
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

    /// Encode `rows` as they arrive. The first call fixes width and
    /// channel count and writes the header with a placeholder height,
    /// patched at `finish`. Output grows by at most `pixels * (channels + 1)`
    /// bytes per call; the kernel's run state carries across calls, so the
    /// result is byte-identical to `encode`.
    fn push_rows(&mut self, rows: PixelSlice<'_>) -> Result<(), Self::Error> {
        use crate::qoi::rapid_qoi::{QOI_HEADER_SIZE, QOI_MAGIC, Qoi};

        let desc = rows.descriptor();
        let channels: usize = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U8, ChannelLayout::Rgb) => 3,
            (ChannelType::U8, ChannelLayout::Rgba) => 4,
            (ChannelType::U8, ChannelLayout::Bgra) => 4,
            _ => {
                return Err(Self::reject(zencodec::UnsupportedOperation::PixelFormat));
            }
        };
        if let Some(s) = &self.stop {
            s.check()
                .map_err(|r| At::<zencodec::CodecError>::from(BitmapError::from(r)))?;
        }
        let limits_once = self.stream.is_none().then(|| self.effective_limits());
        let st = self.stream.get_or_insert_with(|| {
            let mut out = Vec::with_capacity(QOI_HEADER_SIZE + 64 * 1024);
            out.extend_from_slice(&QOI_MAGIC.to_be_bytes());
            out.extend_from_slice(&rows.width().to_be_bytes());
            out.extend_from_slice(&0u32.to_be_bytes());
            out.push(channels as u8);
            out.push(0); // sRGB colour space, as the one-shot path writes
            QoiStreamState {
                out,
                index: [[0; 4]; 64],
                px_rgb: [0, 0, 0],
                px_rgba: [0, 0, 0, 255],
                run: 0,
                width: rows.width(),
                rows: 0,
                channels,
                needs_swizzle: desc.layout() == ChannelLayout::Bgra,
                swizzle_buf: Vec::new(),
                scratch: Vec::new(),
                limits: limits_once.flatten(),
            }
        });
        if st.width != rows.width() || st.channels != channels {
            return Err(BitmapError::InvalidData(
                "push_rows: width or channel count changed".into(),
            )
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
        // kernel's 8-byte headroom; grown only when a larger strip arrives.
        let worst = px_count * (channels + 1) + 8;
        if st.scratch.len() < worst {
            st.scratch.resize(worst, 0);
        }
        let written = if channels == 4 {
            Qoi::encode_range::<4>(
                &mut st.index,
                &mut st.px_rgba,
                &mut st.run,
                pixels,
                &mut st.scratch[..worst],
                false,
            )
        } else {
            Qoi::encode_range::<3>(
                &mut st.index,
                &mut st.px_rgb,
                &mut st.run,
                pixels,
                &mut st.scratch[..worst],
                false,
            )
        }
        .map_err(|e| At::<zencodec::CodecError>::from(BitmapError::InvalidData(e.to_string())))?;
        st.out.extend_from_slice(&st.scratch[..written]);
        st.rows = total_rows;
        Ok(())
    }

    fn finish(self) -> Result<EncodeOutput, Self::Error> {
        use crate::qoi::rapid_qoi::{QOI_OP_RUN, QOI_PADDING};

        let mut st = self.stream.ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::InvalidData(
                "finish() without push_rows()".into(),
            ))
        })?;
        if st.run > 0 {
            // The kernel counts repeats; the opcode stores length - 1.
            st.out.push(QOI_OP_RUN | (st.run as u8 - 1));
            st.run = 0;
        }
        st.out.extend_from_slice(&[0u8; QOI_PADDING - 1]);
        st.out.push(1);
        st.out[8..12].copy_from_slice(&st.rows.to_be_bytes());
        Ok(EncodeOutput::new(st.out, ImageFormat::Qoi))
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
            stop: None,
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
    stop: Option<zencodec::StopToken>,
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
            self.stop.as_ref(),
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
            data, info, descriptor, width, height, has_alpha, row_bytes, self.stop,
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
        alloc::boxed::Box<dyn zencodec::decode::DynIncrementalDecoder>,
        zencodec::decode::BoxedError,
    > {
        let dec = self
            .into_incremental_decoder(preferred)
            .map_err(|e| alloc::boxed::Box::new(e) as zencodec::decode::BoxedError)?;
        Ok(alloc::boxed::Box::new(
            zencodec::decode::IncrementalDecoderShim(dec),
        ))
    }
}

impl QoiDecodeJob {
    /// Create an incremental (sans-I/O) decoder on the feed/step contract.
    pub fn into_incremental_decoder(
        self,
        _preferred: &[PixelDescriptor],
    ) -> Result<QoiIncrementalDecoder, At<zencodec::CodecError>> {
        let limits = self.limits.or(self.config.limits);
        Ok(QoiIncrementalDecoder {
            buf: Vec::new(),
            head: 0,
            buf_start: 0,
            total: None,
            info: None,
            descriptor: None,
            width: 0,
            height: 0,
            has_alpha: false,
            row_bytes: 0,
            state_rgb: crate::qoi::decode::QoiDecodeState::<3>::new(),
            state_rgba: crate::qoi::decode::QoiDecodeState::<4>::new(),
            partial: Vec::new(),
            partial_pos: 0,
            next_row: 0,
            done: None,
            failed: false,
            need: [zencodec::decode::ByteRange::from_offset(0)],
            stop: self.stop,
            limits,
            max_input_bytes: self.max_input_bytes,
            total_input_bytes: 0,
        })
    }
}

// ── QoiIncrementalDecoder ────────────────────────────────────────

/// Unread input retained between steps. Feeds beyond it are accepted
/// partially and re-offered by the driver.
const QOI_INPUT_CAP: usize = 1 << 20;
/// Rows decoded straight into one sink strip per step, at most.
const QOI_MAX_STRIP_ROWS: u32 = 64;
/// Largest QOI opcode: one `QOI_OP_RGBA`. A row of `width` pixels never
/// needs more than `width * 5` input bytes, so once that many are buffered
/// the row is decoded straight into the sink.
const QOI_MAX_OP_BYTES: usize = 5;

/// Incremental (sans-I/O) QOI decoder on the feed/step contract.
///
/// Rows are decoded directly into the sink's strip buffer whenever the
/// buffered input is enough to guarantee the rows complete (or the whole
/// input is present). Only a row that may run out of input mid-way is
/// decoded into a private row buffer and copied, so at most one row per
/// fed chunk takes the copying path. Consumed input is released by moving
/// a head index.
pub struct QoiIncrementalDecoder {
    buf: Vec<u8>,
    head: usize,
    /// Absolute offset of `buf[head]`.
    buf_start: u64,
    total: Option<u64>,
    info: Option<ImageInfo>,
    descriptor: Option<PixelDescriptor>,
    width: u32,
    height: u32,
    has_alpha: bool,
    row_bytes: usize,

    state_rgb: crate::qoi::decode::QoiDecodeState<3>,
    state_rgba: crate::qoi::decode::QoiDecodeState<4>,

    /// Fallback row in progress: `partial[..partial_pos]` is decoded.
    partial: Vec<u8>,
    partial_pos: usize,

    next_row: u32,
    done: Option<zencodec::decode::OutputInfo>,
    failed: bool,
    need: [zencodec::decode::ByteRange; 1],

    stop: Option<zencodec::StopToken>,
    limits: Option<Limits>,
    max_input_bytes: Option<u64>,
    total_input_bytes: u64,
}

impl QoiIncrementalDecoder {
    fn live_len(&self) -> usize {
        self.buf.len() - self.head
    }

    fn cursor(&self) -> u64 {
        self.buf_start + self.live_len() as u64
    }

    fn consume(&mut self, n: usize) {
        self.head += n;
        self.buf_start += n as u64;
        if self.head > self.buf.len() / 2 && self.head >= 64 * 1024 {
            self.buf.drain(..self.head);
            self.head = 0;
        }
    }

    fn err(&mut self, e: BitmapError) -> At<zencodec::CodecError> {
        self.failed = true;
        At::<zencodec::CodecError>::from(e)
    }

    fn sink_err(&mut self, e: zencodec::decode::SinkError) -> At<zencodec::CodecError> {
        self.err(BitmapError::InvalidData(alloc::format!("sink error: {e}")))
    }

    fn need_more(&mut self) -> zencodec::decode::Step<'_> {
        self.need = [zencodec::decode::ByteRange::from_offset(self.cursor())];
        zencodec::decode::Step::Need(&self.need)
    }

    fn channels(&self) -> usize {
        if self.has_alpha { 4 } else { 3 }
    }

    /// Decode from the buffered input starting `from` bytes into the live
    /// region, into `out`. Returns `(bytes_consumed, bytes_written)`.
    fn decode_live(&mut self, from: usize, out: &mut [u8]) -> (usize, usize) {
        let live = &self.buf[self.head + from..];
        let (c, px) = if self.has_alpha {
            self.state_rgba.decode_chunk(live, out)
        } else {
            self.state_rgb.decode_chunk(live, out)
        };
        (c, px * self.channels())
    }

    fn parse_header(
        &mut self,
        sink: &mut dyn zencodec::decode::DecodeRowSink,
    ) -> Result<(), At<zencodec::CodecError>> {
        let hdr = match crate::qoi::decode::parse_header(&self.buf[self.head..self.head + 14]) {
            Ok(h) => h,
            Err(e) => {
                self.failed = true;
                return Err(zencodec::CodecError::of(e));
            }
        };
        let (width, height, has_alpha) = (hdr.width, hdr.height, hdr.has_alpha);
        if let Some(lim) = &self.limits
            && let Err(e) = lim.check(width, height)
        {
            self.failed = true;
            return Err(zencodec::CodecError::of(e));
        }
        let channels: usize = if has_alpha { 4 } else { 3 };
        let Some(row_bytes) = (width as usize).checked_mul(channels) else {
            return Err(self.err(BitmapError::DimensionsTooLarge { width, height }));
        };
        if let Err(e) = crate::limits::check_output_size(
            row_bytes.saturating_mul(height as usize),
            self.limits.as_ref(),
        ) {
            self.failed = true;
            return Err(zencodec::CodecError::of(e));
        }

        // Same labelling as the one-shot path (`layout_to_pixel_buffer`
        // builds from typed `rgb` pixels, whose descriptor leaves the
        // transfer unknown), so every decode path agrees on the descriptor.
        let descriptor = if has_alpha {
            PixelDescriptor::RGBA8
        } else {
            PixelDescriptor::RGB8
        };
        let info = ImageInfo::new(width, height, ImageFormat::Qoi)
            .with_alpha(has_alpha)
            .with_bit_depth(8)
            .with_channel_count(channels as u8)
            .with_source_encoding_details(BitmapSourceEncoding);

        self.width = width;
        self.height = height;
        self.has_alpha = has_alpha;
        self.row_bytes = row_bytes;
        self.partial = alloc::vec![0u8; row_bytes];
        self.info = Some(info);
        self.descriptor = Some(descriptor);
        self.consume(14);
        if let Err(e) = sink.begin(width, height, descriptor) {
            return Err(self.sink_err(e));
        }
        Ok(())
    }

    /// Hand the completed fallback row to the sink.
    fn emit_partial(
        &mut self,
        sink: &mut dyn zencodec::decode::DecodeRowSink,
    ) -> Result<zencodec::decode::Step<'_>, At<zencodec::CodecError>> {
        let (y, width, row_bytes, desc) = (
            self.next_row,
            self.width,
            self.row_bytes,
            self.descriptor.expect("header parsed"),
        );
        let partial = core::mem::take(&mut self.partial);
        let copied = sink
            .provide_next_buffer(y, 1, width, desc)
            .map(|mut dst| dst.row_mut(0)[..row_bytes].copy_from_slice(&partial));
        self.partial = partial;
        if let Err(e) = copied {
            return Err(self.sink_err(e));
        }
        self.partial_pos = 0;
        self.next_row += 1;
        Ok(zencodec::decode::Step::Rows {
            meta: zencodec::decode::BatchMeta::final_rows(y),
            rows: 1,
        })
    }
}

impl zencodec::decode::IncrementalDecode for QoiIncrementalDecoder {
    type Error = At<zencodec::CodecError>;

    fn feed(&mut self, offset: u64, bytes: &[u8], eof: bool) -> Result<usize, Self::Error> {
        if eof {
            self.total = Some(offset.saturating_add(bytes.len() as u64));
        }
        if self.done.is_some() || self.failed {
            return Ok(bytes.len());
        }
        let cursor = self.cursor();
        // Bytes behind the cursor were consumed; bytes ahead of it cannot
        // go in a sequential buffer. Both count as accepted.
        if offset.saturating_add(bytes.len() as u64) <= cursor || offset > cursor {
            return Ok(bytes.len());
        }
        let skip = (cursor - offset) as usize;
        let room = QOI_INPUT_CAP.saturating_sub(self.live_len());
        let take = (bytes.len() - skip).min(room);
        if let Some(max) = self.max_input_bytes {
            self.total_input_bytes = self.total_input_bytes.saturating_add(take as u64);
            if self.total_input_bytes > max {
                let total = self.total_input_bytes;
                return Err(self.err(BitmapError::LimitExceeded(alloc::format!(
                    "input size {total} exceeds limit {max}"
                ))));
            }
        }
        self.buf.extend_from_slice(&bytes[skip..skip + take]);
        Ok(skip + take)
    }

    fn step(
        &mut self,
        sink: &mut dyn zencodec::decode::DecodeRowSink,
        stop: Option<&dyn Stop>,
    ) -> Result<zencodec::decode::Step<'_>, Self::Error> {
        use zencodec::decode::{BatchMeta, MetadataUpdate, Step};

        if self.failed {
            return Err(At::<zencodec::CodecError>::from(BitmapError::InvalidData(
                "decoder already failed".into(),
            )));
        }
        if let Some(done) = &self.done {
            return Ok(Step::Done(done.clone()));
        }
        if let Some(s) = stop {
            s.check()
                .map_err(|r| At::<zencodec::CodecError>::from(BitmapError::from(r)))?;
        }
        if let Some(s) = &self.stop {
            s.check()
                .map_err(|r| At::<zencodec::CodecError>::from(BitmapError::from(r)))?;
        }
        let at_end = self.total.is_some_and(|t| self.cursor() >= t);

        if self.info.is_none() {
            if self.live_len() < 14 {
                if at_end {
                    return Err(self.err(BitmapError::UnexpectedEof));
                }
                return Ok(self.need_more());
            }
            self.parse_header(sink)?;
            return Ok(Step::Metadata(MetadataUpdate::HeaderReady));
        }
        let desc = self.descriptor.expect("set with info");
        let (width, height, row_bytes) = (self.width, self.height, self.row_bytes);

        if self.next_row >= height {
            // The end marker must follow the pixel data: wait for it when
            // the length is still unknown, reject a stream cut inside it.
            if self.live_len() < crate::qoi::rapid_qoi::QOI_PADDING {
                if at_end {
                    return Err(self.err(BitmapError::UnexpectedEof));
                }
                return Ok(self.need_more());
            }
            if let Err(e) = sink.finish() {
                return Err(self.sink_err(e));
            }
            let info = zencodec::decode::OutputInfo::full_decode(width, height, desc);
            self.done = Some(info.clone());
            return Ok(Step::Done(info));
        }

        // A row that started in the fallback buffer finishes there.
        if self.partial_pos > 0 {
            let mut partial = core::mem::take(&mut self.partial);
            let (c, w) = self.decode_live(0, &mut partial[self.partial_pos..]);
            self.partial = partial;
            self.consume(c);
            self.partial_pos += w;
            if self.partial_pos == row_bytes {
                return self.emit_partial(sink);
            }
            if at_end {
                return Err(self.err(BitmapError::UnexpectedEof));
            }
            return Ok(self.need_more());
        }

        let remaining = height - self.next_row;
        let worst_per_row = width as usize * QOI_MAX_OP_BYTES;
        let guaranteed = if at_end {
            remaining
        } else {
            (self.live_len() / worst_per_row).min(remaining as usize) as u32
        };
        let rows = guaranteed.min(QOI_MAX_STRIP_ROWS);

        if rows >= 1 {
            // Enough input for every row of this strip: decode straight
            // into the sink.
            let y = self.next_row;
            let mut consumed = 0usize;
            let mut short = false;
            let mut dst = match sink.provide_next_buffer(y, rows, width, desc) {
                Ok(d) => d,
                Err(e) => return Err(self.sink_err(e)),
            };
            for r in 0..rows {
                let live = &self.buf[self.head + consumed..];
                let out = &mut dst.row_mut(r)[..row_bytes];
                let (c, px) = if self.has_alpha {
                    self.state_rgba.decode_chunk(live, out)
                } else {
                    self.state_rgb.decode_chunk(live, out)
                };
                consumed += c;
                if px * self.channels() != row_bytes {
                    short = true;
                    break;
                }
            }
            drop(dst);
            self.consume(consumed);
            if short {
                // Only reachable at the announced end with a truncated
                // stream: the input bound guarantees completion otherwise.
                return Err(self.err(BitmapError::UnexpectedEof));
            }
            self.next_row += rows;
            return Ok(Step::Rows {
                meta: BatchMeta::final_rows(y),
                rows,
            });
        }

        // Not enough buffered input to guarantee a row: decode what is
        // there into the fallback row.
        let mut partial = core::mem::take(&mut self.partial);
        let (c, w) = self.decode_live(0, &mut partial[..]);
        self.partial = partial;
        self.consume(c);
        self.partial_pos = w;
        if self.partial_pos == row_bytes {
            return self.emit_partial(sink);
        }
        if at_end {
            return Err(self.err(BitmapError::UnexpectedEof));
        }
        Ok(self.need_more())
    }

    fn wrap_sink_error(err: zencodec::decode::SinkError) -> Self::Error {
        BitmapError::InvalidData(alloc::format!("sink error: {err}")).into()
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
    stop: Option<zencodec::StopToken>,
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
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
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
            stop,
        })
    }
}

impl zencodec::decode::StreamingDecode for QoiStreamingDecoder<'_> {
    type Error = At<zencodec::CodecError>;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, Self::Error> {
        if self.current_row >= self.height {
            return Ok(None);
        }

        if let Some(ref stop) = self.stop {
            stop.check()
                .map_err(|r| At::<zencodec::CodecError>::from(BitmapError::from(r)))?;
        }

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
        if self.current_row == self.height {
            // Every path agrees on what a complete file is.
            crate::qoi::decode::end_marker_present(
                self.data.len().saturating_sub(self.byte_offset),
            )
            .map_err(zencodec::CodecError::of)?;
        }

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
