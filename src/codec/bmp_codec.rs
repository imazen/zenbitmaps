use super::*;
use whereat::At;

use crate::alloc_util::AllocPref;

// ══════════════════════════════════════════════════════════════════════
// BMP capabilities and descriptors
// ══════════════════════════════════════════════════════════════════════

static BMP_ENCODE_CAPS: EncodeCapabilities = EncodeCapabilities::new()
    .with_lossless(true)
    .with_native_alpha(true)
    .with_stop(true)
    .with_enforces_max_pixels(true);

static BMP_DECODE_CAPS: DecodeCapabilities = DecodeCapabilities::new()
    .with_cheap_probe(true)
    .with_incremental(true)
    .with_native_gray(true)
    .with_native_alpha(true)
    .with_stop(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true)
    .with_inventory(cfg!(feature = "inventory"));

static BMP_ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
];

static BMP_DECODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGB8_SRGB,
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::GRAY8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
];

// ══════════════════════════════════════════════════════════════════════
// BMP codec
// ══════════════════════════════════════════════════════════════════════

// ── BmpEncoderConfig ─────────────────────────────────────────────

/// Encoding configuration for BMP format.
///
/// Supports 24-bit RGB and 32-bit RGBA BMP output.
#[derive(Clone, Debug)]
pub struct BmpEncoderConfig {
    limits: ResourceLimits,
}

impl Default for BmpEncoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl BmpEncoderConfig {
    /// Create a new BMP encoder config with default settings.
    pub fn new() -> Self {
        Self {
            limits: ResourceLimits::none(),
        }
    }
}

impl zencodec::encode::EncoderConfig for BmpEncoderConfig {
    type Error = At<zencodec::CodecError>;
    type Job = BmpEncodeJob;

    fn format() -> ImageFormat {
        ImageFormat::Bmp
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        BMP_ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static EncodeCapabilities {
        &BMP_ENCODE_CAPS
    }

    fn is_lossless(&self) -> Option<bool> {
        Some(true)
    }

    fn estimate_encode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // Uncompressed 24/32-bit BMP: output ≈ input (plus a small fixed header).
        trivial_encode_resources(image, compute, 1.0)
    }

    fn job(self) -> BmpEncodeJob {
        BmpEncodeJob {
            config: self,
            limits: None,
            pulse: zencodec::SharedPulse::default(),
        }
    }
}

// ── BmpEncodeJob ─────────────────────────────────────────────────

/// Per-operation BMP encode job.
pub struct BmpEncodeJob {
    config: BmpEncoderConfig,
    limits: Option<ResourceLimits>,
    pulse: zencodec::SharedPulse,
}

impl zencodec::encode::EncodeJob for BmpEncodeJob {
    type Error = At<zencodec::CodecError>;
    type Enc = BmpEncoder;
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

    fn encoder(self) -> Result<BmpEncoder, Self::Error> {
        Ok(BmpEncoder {
            config: self.config,
            limits: self.limits,
            pulse: self.pulse,
            declared: None,
            rows: None,
        })
    }

    fn animation_frame_encoder(self) -> Result<(), Self::Error> {
        Err(BitmapError::from(zencodec::UnsupportedOperation::AnimationEncode).into())
    }
}

// ── BmpEncoder ───────────────────────────────────────────────────

/// Single-image BMP encoder.
pub struct BmpEncoder {
    config: BmpEncoderConfig,
    limits: Option<ResourceLimits>,
    pulse: zencodec::SharedPulse,
    /// The image `begin` declared: width, height, descriptor.
    declared: Option<(u32, u32, PixelDescriptor)>,
    /// Rows pushed so far. BMP stores rows bottom-up, so `finish` encodes
    /// them with the one-shot path: the same narrowing and the same bytes.
    rows: Option<BmpRows>,
}

struct BmpRows {
    data: Vec<u8>,
    width: u32,
    height: u32,
    descriptor: PixelDescriptor,
}

impl BmpEncoder {
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

impl zencodec::encode::Encoder for BmpEncoder {
    type Error = At<zencodec::CodecError>;

    fn reject(op: zencodec::UnsupportedOperation) -> At<zencodec::CodecError> {
        BitmapError::from(op).into()
    }

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, Self::Error> {
        self.encode_slice(pixels)
    }

    /// Rows accumulate and the file is written at `finish`. Only descriptors
    /// BMP stores directly (8-bit RGB, RGBA, BGRA) are declared; others may
    /// still narrow into them, but only once the whole image is known.
    fn begin(
        &mut self,
        width: u32,
        height: u32,
        descriptor: PixelDescriptor,
    ) -> Result<zencodec::encode::OutputTiming, Self::Error> {
        if self.declared.is_some() {
            return Err(BitmapError::InvalidData("begin called twice".into()).into());
        }
        if self.rows.is_some() {
            return Err(BitmapError::InvalidData("begin after rows were pushed".into()).into());
        }
        if bmp_layout(descriptor).is_none() {
            return Err(Self::reject(zencodec::UnsupportedOperation::PixelFormat));
        }
        if let Some(limits) = self.effective_limits() {
            limits
                .check(width, height)
                .map_err(zencodec::CodecError::of)?;
        }
        self.declared = Some((width, height, descriptor));
        Ok(zencodec::encode::OutputTiming::AT_FINISH)
    }

    fn push_rows(&mut self, rows: PixelSlice<'_>) -> Result<(), Self::Error> {
        self.pulse
            .check()
            .map_err(|r| At::<zencodec::CodecError>::from(BitmapError::from(r)))?;
        let (width, descriptor) = (rows.width(), rows.descriptor());
        let declared = self.declared;
        if declared.is_some_and(|(w, _, d)| (w, d) != (width, descriptor)) {
            return Err(
                BitmapError::InvalidData("push_rows: rows differ from begin".into()).into(),
            );
        }
        let acc = self.rows.get_or_insert_with(|| BmpRows {
            data: Vec::new(),
            width,
            height: 0,
            descriptor,
        });
        if (acc.width, acc.descriptor) != (width, descriptor) {
            return Err(BitmapError::InvalidData(
                "push_rows: width or pixel format changed".into(),
            )
            .into());
        }
        let height = acc.height.checked_add(rows.rows()).ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::InvalidData("too many rows".into()))
        })?;
        if declared.is_some_and(|(_, h, _)| height > h) {
            return Err(BitmapError::InvalidData(
                "push_rows: more rows than begin declared".into(),
            )
            .into());
        }
        let limits = self.effective_limits();
        if let Some(limits) = &limits {
            limits
                .check(width, height)
                .map_err(zencodec::CodecError::of)?;
        }
        let row_bytes = width as usize * descriptor.bytes_per_pixel();
        let acc = self.rows.as_mut().expect("rows were just initialized");
        acc.data
            .try_reserve(row_bytes * rows.rows() as usize)
            .map_err(|_| {
                At::<zencodec::CodecError>::from(BitmapError::OutOfMemory("BMP rows".into()))
            })?;
        for y in 0..rows.rows() {
            acc.data.extend_from_slice(&rows.row(y)[..row_bytes]);
        }
        acc.height = height;
        Ok(())
    }

    fn finish(self) -> Result<EncodeOutput, Self::Error> {
        let rows = self.rows.as_ref().ok_or_else(|| {
            At::<zencodec::CodecError>::from(BitmapError::InvalidData(
                "finish() without push_rows()".into(),
            ))
        })?;
        if self.declared.is_some_and(|(_, h, _)| h != rows.height) {
            return Err(
                BitmapError::InvalidData("finish: fewer rows than begin declared".into()).into(),
            );
        }
        let stride = rows.width as usize * rows.descriptor.bytes_per_pixel();
        let pixels = PixelSlice::new(&rows.data, rows.width, rows.height, stride, rows.descriptor)
            .map_err(|e| {
                At::<zencodec::CodecError>::from(BitmapError::InvalidData(e.to_string()))
            })?;
        self.encode_slice(pixels)
    }
}

/// The BMP layout that stores `desc` directly, and whether it has alpha.
fn bmp_layout(desc: PixelDescriptor) -> Option<(crate::PixelLayout, bool)> {
    match (desc.channel_type(), desc.layout()) {
        (ChannelType::U8, ChannelLayout::Rgb) => Some((crate::PixelLayout::Rgb8, false)),
        (ChannelType::U8, ChannelLayout::Rgba) => Some((crate::PixelLayout::Rgba8, true)),
        (ChannelType::U8, ChannelLayout::Bgra) => Some((crate::PixelLayout::Bgra8, true)),
        _ => None,
    }
}

impl BmpEncoder {
    /// The one-shot encode, shared with `finish`.
    fn encode_slice(
        &self,
        pixels: PixelSlice<'_>,
    ) -> Result<EncodeOutput, At<zencodec::CodecError>> {
        // Bit-exact load-bearing narrowing (dead alpha / chroma-free /
        // replicated-low-bits) before format mapping — see
        // `super::reduce_for_raw_encode`. BMP encodes only 24-bit RGB and
        // 32-bit RGBA/BGRA, so the predicate forbids the →Gray narrowing
        // while still allowing dead-alpha and bit-depth reductions.
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
        // A well-formed request for a pixel format this encoder does not
        // support — caller-request-origin, routed via the existing
        // zencodec::UnsupportedOperation::PixelFormat axis.
        let Some((layout, alpha)) = bmp_layout(desc) else {
            return Err(BitmapError::from(zencodec::UnsupportedOperation::PixelFormat).into());
        };

        let encoded = crate::bmp::encode(&bytes, w, h, layout, alpha, stop)
            .map_err(zencodec::CodecError::of)?;
        Ok(EncodeOutput::new(encoded, ImageFormat::Bmp))
    }
}

// ── BmpDecoderConfig ─────────────────────────────────────────────

/// Decoding configuration for BMP format.
#[derive(Clone, Debug)]
pub struct BmpDecoderConfig {
    limits: Option<Limits>,
}

impl Default for BmpDecoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl BmpDecoderConfig {
    /// Create a new BMP decoder config with default settings.
    pub fn new() -> Self {
        Self { limits: None }
    }
}

impl zencodec::decode::DecoderConfig for BmpDecoderConfig {
    type Error = At<zencodec::CodecError>;
    type Job<'a> = BmpDecodeJob;

    fn formats() -> &'static [ImageFormat] {
        &[ImageFormat::Bmp]
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        BMP_DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static DecodeCapabilities {
        &BMP_DECODE_CAPS
    }

    fn estimate_decode_resources(
        &self,
        image: &zencodec::estimate::ImageCharacteristics,
        compute: &zencodec::estimate::ComputeEnvironment,
    ) -> zencodec::estimate::ResourceEstimate {
        // BMP working set ≈ output buffer + one scanline + ≤256-entry palette
        // (serial; RLE expands into a second output-sized buffer but the
        // bomb-ratio guard keeps it bounded).
        super::trivial_decode_resources(image, compute)
    }

    fn job<'a>(self) -> Self::Job<'a> {
        BmpDecodeJob {
            config: self,
            limits: None,
            pulse: zencodec::SharedPulse::default(),
            max_input_bytes: None,
            alloc_pref: AllocPref::CodecDefault,
            policy: None,
        }
    }
}

// ── BmpDecodeJob ─────────────────────────────────────────────────

/// Per-operation BMP decode job.
pub struct BmpDecodeJob {
    config: BmpDecoderConfig,
    limits: Option<Limits>,
    pulse: zencodec::SharedPulse,
    max_input_bytes: Option<u64>,
    /// Allocation-fallibility preference from
    /// [`ResourceLimits::prefer_fallible_allocations`].
    alloc_pref: AllocPref,
    policy: Option<DecodePolicy>,
}

impl<'a> zencodec::decode::DecodeJob<'a> for BmpDecodeJob {
    type Error = At<zencodec::CodecError>;
    type Dec = BmpDecoder<'a>;
    type StreamDec = zencodec::Unsupported<At<zencodec::CodecError>>;
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
        // Metadata only — do not reject on the pixel-count cap.
        let header = crate::bmp::decode::parse_bmp_header(data, u64::MAX)
            .map_err(zencodec::CodecError::of)?;
        let has_alpha = header.has_alpha;
        let channel_count: u8 = match header.layout {
            crate::PixelLayout::Gray8 => 1,
            crate::PixelLayout::Rgb8 => 3,
            crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8 => 4,
            crate::PixelLayout::Rgba16 if has_alpha => 4,
            _ => 3, // BMP decoded output is at least RGB
        };
        let mut info = ImageInfo::new(header.width, header.height, ImageFormat::Bmp)
            .with_alpha(has_alpha)
            .with_bit_depth(header.bpp as u8)
            .with_channel_count(channel_count)
            .with_cicp(zencodec::Cicp::SRGB)
            .with_source_encoding_details(BitmapSourceEncoding);
        // BMP stores resolution as pixels-per-meter
        if header.x_pels_per_meter > 0 || header.y_pels_per_meter > 0 {
            info = info.with_resolution(zencodec::Resolution {
                x: header.x_pels_per_meter as f64,
                y: header.y_pels_per_meter as f64,
                unit: zencodec::ResolutionUnit::Meter,
            });
        }
        Ok(info)
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, Self::Error> {
        // Metadata only — do not reject on the pixel-count cap.
        let header = crate::bmp::decode::parse_bmp_header(data, u64::MAX)
            .map_err(zencodec::CodecError::of)?;
        let has_alpha = header.has_alpha;
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
            crate::bmp::decode::parse_bmp_header(data, u64::MAX)
                .ok()
                .map(|h| (h.width, h.height, Some(h.layout.bytes_per_pixel())))
        };
        let perm = policy_to_bmp_permissiveness(self.policy.as_ref());
        super::job_inventory(
            data,
            &self.pulse,
            self.max_input_bytes,
            limits,
            output,
            |d, control| crate::inventory::bmp::walk(d, perm, control),
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<BmpDecoder<'a>, Self::Error> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))
            .into());
        }
        let permissiveness = policy_to_bmp_permissiveness(self.policy.as_ref());
        Ok(BmpDecoder {
            config: self.config,
            limits: self.limits,
            data,
            pulse: self.pulse,
            permissiveness,
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
        // Rows are not decodable before the whole file is here (bottom-up
        // and RLE bodies): report the header early, decode at the end with
        // this job's own one-shot path.
        let (pulse, max_input_bytes) = (self.pulse.clone(), self.max_input_bytes);
        Ok(zencodec::decode::erase_incremental(
            super::strips::StripDecoder::new(
                super::strips::WholeFile::new(self, ImageFormat::Bmp),
                pulse,
                max_input_bytes,
            ),
        ))
    }
}

// ── BmpDecoder ───────────────────────────────────────────────────

/// Single-image BMP decoder.
pub struct BmpDecoder<'a> {
    config: BmpDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    pulse: zencodec::SharedPulse,
    permissiveness: crate::bmp::BmpPermissiveness,
    alloc_pref: AllocPref,
}

impl BmpDecoder<'_> {
    fn effective_limits(&self) -> Option<&Limits> {
        self.limits.as_ref().or(self.config.limits.as_ref())
    }
}

impl zencodec::decode::Decode for BmpDecoder<'_> {
    type Error = At<zencodec::CodecError>;

    fn decode(self) -> Result<DecodeOutput, Self::Error> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = &self.pulse;
        let decoded = crate::bmp::decode_with_permissiveness_and_alloc_pref(
            &self.data,
            limits,
            self.permissiveness,
            self.alloc_pref,
            stop,
        )
        .map_err(zencodec::CodecError::of)?;
        decode_output_from_internal(&decoded, ImageFormat::Bmp).map_err(zencodec::CodecError::of)
    }
}

/// Map [`DecodePolicy`] to [`BmpPermissiveness`].
///
/// - `strict == Some(true)` → `Strict`
/// - `allow_truncated == Some(true)` → `Permissive`
/// - otherwise (or no policy) → `Standard`
fn policy_to_bmp_permissiveness(policy: Option<&DecodePolicy>) -> crate::bmp::BmpPermissiveness {
    use crate::bmp::BmpPermissiveness;
    let Some(p) = policy else {
        return BmpPermissiveness::Standard;
    };
    if p.resolve_strict(false) {
        BmpPermissiveness::Strict
    } else if p.resolve_truncated(false) {
        BmpPermissiveness::Permissive
    } else {
        BmpPermissiveness::Standard
    }
}
