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
    .with_native_gray(true)
    .with_native_alpha(true)
    .with_stop(true)
    .with_enforces_max_pixels(true)
    .with_enforces_max_memory(true)
    .with_enforces_max_input_bytes(true);

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
            stop: None,
        }
    }
}

// ── BmpEncodeJob ─────────────────────────────────────────────────

/// Per-operation BMP encode job.
pub struct BmpEncodeJob {
    config: BmpEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
}

impl zencodec::encode::EncodeJob for BmpEncodeJob {
    type Error = At<zencodec::CodecError>;
    type Enc = BmpEncoder;
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

    fn encoder(self) -> CodecResult<BmpEncoder> {
        Ok(BmpEncoder {
            config: self.config,
            limits: self.limits,
            stop: self.stop,
        })
    }

    fn animation_frame_encoder(self) -> CodecResult<()> {
        Err(cerr!(BitmapError::from(
            zencodec::UnsupportedOperation::AnimationEncode,
        )))
    }
}

// ── BmpEncoder ───────────────────────────────────────────────────

/// Single-image BMP encoder.
pub struct BmpEncoder {
    config: BmpEncoderConfig,
    limits: Option<ResourceLimits>,
    stop: Option<zencodec::StopToken>,
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
        cerr!(BitmapError::from(op))
    }

    fn encode(self, pixels: PixelSlice<'_>) -> CodecResult<EncodeOutput> {
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
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let desc = pixels.descriptor();
        let w = pixels.width();
        let h = pixels.rows();

        if let Some(limits) = self.effective_limits() {
            limits.check(w, h).envelope()?;
        }

        let bytes = pixels.contiguous_bytes();
        let (layout, alpha) = match (desc.channel_type(), desc.layout()) {
            (ChannelType::U8, ChannelLayout::Rgb) => (crate::PixelLayout::Rgb8, false),
            (ChannelType::U8, ChannelLayout::Rgba) => (crate::PixelLayout::Rgba8, true),
            (ChannelType::U8, ChannelLayout::Bgra) => (crate::PixelLayout::Bgra8, true),
            _ => {
                return Err(cerr!(BitmapError::UnsupportedPixelFormat(alloc::format!(
                    "BMP encode: unsupported pixel format: {:?}",
                    desc
                ))));
            }
        };

        let encoded = crate::bmp::encode(&bytes, w, h, layout, alpha, stop).envelope()?;
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
            stop: None,
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
    stop: Option<zencodec::StopToken>,
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

    fn probe(&self, data: &[u8]) -> CodecResult<ImageInfo> {
        // Metadata only — do not reject on the pixel-count cap.
        let header = crate::bmp::decode::parse_bmp_header(data, u64::MAX).envelope()?;
        let has_alpha = matches!(
            header.layout,
            crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
        );
        let channel_count: u8 = match header.layout {
            crate::PixelLayout::Gray8 => 1,
            crate::PixelLayout::Rgb8 => 3,
            crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8 => 4,
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

    fn output_info(&self, data: &[u8]) -> CodecResult<OutputInfo> {
        // Metadata only — do not reject on the pixel-count cap.
        let header = crate::bmp::decode::parse_bmp_header(data, u64::MAX).envelope()?;
        let has_alpha = matches!(
            header.layout,
            crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8
        );
        let native_format = layout_to_descriptor(header.layout);
        Ok(
            OutputInfo::full_decode(header.width, header.height, native_format)
                .with_alpha(has_alpha),
        )
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> CodecResult<BmpDecoder<'a>> {
        if let Some(max) = self.max_input_bytes
            && data.len() as u64 > max
        {
            return Err(cerr!(BitmapError::LimitExceeded(alloc::format!(
                "input size {} exceeds limit {max}",
                data.len()
            ))));
        }
        let permissiveness = policy_to_bmp_permissiveness(self.policy.as_ref());
        Ok(BmpDecoder {
            config: self.config,
            limits: self.limits,
            data,
            stop: self.stop,
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
            cerr!(BitmapError::InvalidData(e.to_string()))
        })
    }

    fn streaming_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> CodecResult<zencodec::Unsupported<At<zencodec::CodecError>>> {
        Err(cerr!(BitmapError::from(
            zencodec::UnsupportedOperation::RowLevelDecode,
        )))
    }

    fn animation_frame_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> CodecResult<zencodec::Unsupported<At<zencodec::CodecError>>> {
        Err(cerr!(BitmapError::from(
            zencodec::UnsupportedOperation::AnimationDecode,
        )))
    }
}

// ── BmpDecoder ───────────────────────────────────────────────────

/// Single-image BMP decoder.
pub struct BmpDecoder<'a> {
    config: BmpDecoderConfig,
    limits: Option<Limits>,
    data: Cow<'a, [u8]>,
    stop: Option<zencodec::StopToken>,
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

    fn decode(self) -> CodecResult<DecodeOutput> {
        let limits = self.effective_limits();
        let stop: &dyn Stop = match &self.stop {
            Some(s) => s,
            None => &enough::Unstoppable,
        };
        let decoded = crate::bmp::decode_with_permissiveness_and_alloc_pref(
            &self.data,
            limits,
            self.permissiveness,
            self.alloc_pref,
            stop,
        )
        .envelope()?;
        decode_output_from_internal(&decoded, ImageFormat::Bmp)
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
