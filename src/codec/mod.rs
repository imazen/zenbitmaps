//! zencodec trait implementations for zenbitmaps.
//!
//! Provides per-format codec pairs:
//! - PNM: PnmEncoderConfig / PnmDecoderConfig (always available)
//! - BMP: BmpEncoderConfig / BmpDecoderConfig (requires `bmp` feature)
//! - Farbfeld: FarbfeldEncoderConfig / FarbfeldDecoderConfig (always available)

use alloc::borrow::Cow;
use alloc::string::ToString as _;
use alloc::vec::Vec;
use enough::Stop;
use zencodec::decode::{DecodeCapabilities, DecodeOutput, DecodePolicy, OutputInfo};
use zencodec::encode::{EncodeCapabilities, EncodeOutput};
use zencodec::{ImageFormat, ImageInfo, Metadata, ResourceLimits};
use zenpixels::{ChannelLayout, ChannelType, PixelBuffer, PixelDescriptor, PixelSlice};

use crate::error::BitmapError;
use crate::limits::Limits;

/// Source encoding details shared by all bitmap formats (PNM, BMP, Farbfeld).
///
/// All three are lossless and have no meaningful quality metric.
#[derive(Debug, Clone, Copy)]
pub struct BitmapSourceEncoding;

impl zencodec::SourceEncodingDetails for BitmapSourceEncoding {
    fn source_generic_quality(&self) -> Option<f32> {
        None
    }

    fn is_lossless(&self) -> bool {
        true
    }
}

// ══════════════════════════════════════════════════════════════════════
// Per-format codec modules
// ══════════════════════════════════════════════════════════════════════

mod pnm_codec;
pub use pnm_codec::*;

#[cfg(feature = "bmp")]
mod bmp_codec;
#[cfg(feature = "bmp")]
pub use bmp_codec::*;

mod farbfeld_codec;
pub use farbfeld_codec::*;

#[cfg(feature = "qoi")]
mod qoi_codec;
#[cfg(feature = "qoi")]
pub use qoi_codec::*;

// ══════════════════════════════════════════════════════════════════════
// Shared helpers
// ══════════════════════════════════════════════════════════════════════

pub(crate) fn convert_limits(limits: &ResourceLimits) -> Limits {
    Limits {
        max_width: limits.max_width.map(u64::from),
        max_height: limits.max_height.map(u64::from),
        max_pixels: limits.max_pixels,
        max_memory_bytes: limits.max_memory_bytes,
    }
}

pub(crate) fn header_to_image_info(header: &crate::pnm::PnmHeader) -> ImageInfo {
    use crate::PixelLayout;
    let has_alpha = matches!(
        header.layout,
        PixelLayout::Rgba8 | PixelLayout::Bgra8 | PixelLayout::Rgba16
    );
    let bit_depth: u8 = match header.layout {
        PixelLayout::GrayF32 | PixelLayout::RgbF32 => 32,
        _ if header.maxval > 255 => 16,
        _ => 8,
    };
    // PFM is linear float; all other PNM variants are sRGB
    let cicp = match header.layout {
        PixelLayout::GrayF32 | PixelLayout::RgbF32 => {
            zencodec::Cicp::new(1, 8, 0, true) // BT.709 primaries, Linear transfer
        }
        _ => zencodec::Cicp::SRGB,
    };
    ImageInfo::new(header.width, header.height, ImageFormat::Pnm)
        .with_alpha(has_alpha)
        .with_bit_depth(bit_depth)
        .with_channel_count(header.depth as u8)
        .with_cicp(cicp)
        .with_source_encoding_details(BitmapSourceEncoding)
}

pub(crate) fn layout_to_descriptor(layout: crate::PixelLayout) -> PixelDescriptor {
    use crate::PixelLayout;
    match layout {
        PixelLayout::Gray8 => PixelDescriptor::GRAY8_SRGB,
        PixelLayout::Gray16 => PixelDescriptor::GRAY16_SRGB,
        PixelLayout::Rgb8 => PixelDescriptor::RGB8_SRGB,
        PixelLayout::Rgba8 => PixelDescriptor::RGBA8_SRGB,
        PixelLayout::GrayF32 => PixelDescriptor::GRAYF32_LINEAR,
        PixelLayout::RgbF32 => PixelDescriptor::RGBAF32_LINEAR,
        PixelLayout::Bgr8 | PixelLayout::Bgrx8 => PixelDescriptor::RGB8_SRGB,
        PixelLayout::Bgra8 => PixelDescriptor::BGRA8_SRGB,
        PixelLayout::Rgba16 => PixelDescriptor::RGBA16_SRGB,
    }
}

pub(crate) fn layout_to_pixel_buffer(
    decoded: &crate::decode::DecodeOutput<'_>,
) -> Result<PixelBuffer, BitmapError> {
    use crate::PixelLayout;
    use rgb::AsPixels as _;

    let w = decoded.width as usize;
    let h = decoded.height as usize;
    let bytes = decoded.pixels();

    match decoded.layout {
        PixelLayout::Gray8 => {
            let pixels: &[rgb::Gray<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::Gray16 => {
            let pixels: Vec<rgb::Gray<u16>> = bytes
                .chunks_exact(2)
                .map(|c| rgb::Gray::new(u16::from_ne_bytes([c[0], c[1]])))
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::Rgb8 => {
            let pixels: &[rgb::Rgb<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::Rgba8 => {
            let pixels: &[rgb::Rgba<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::GrayF32 => {
            let pixels: Vec<rgb::Gray<f32>> = bytes
                .chunks_exact(4)
                .map(|c| rgb::Gray::new(f32::from_ne_bytes([c[0], c[1], c[2], c[3]])))
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::RgbF32 => {
            // RgbF32 → promote to RgbaF32 (PFM has no alpha concept)
            let pixels: Vec<rgb::Rgba<f32>> = bytes
                .chunks_exact(12)
                .map(|c| {
                    let r = f32::from_ne_bytes([c[0], c[1], c[2], c[3]]);
                    let g = f32::from_ne_bytes([c[4], c[5], c[6], c[7]]);
                    let b = f32::from_ne_bytes([c[8], c[9], c[10], c[11]]);
                    rgb::Rgba { r, g, b, a: 1.0 }
                })
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::Bgr8 => {
            // BGR → convert to RGB
            let pixels: Vec<rgb::Rgb<u8>> = bytes
                .chunks_exact(3)
                .map(|c| rgb::Rgb {
                    r: c[2],
                    g: c[1],
                    b: c[0],
                })
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::Bgra8 => {
            let pixels: &[rgb::alt::BGRA<u8>] = bytes.as_pixels();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels.to_vec(), w, h)).into())
        }
        PixelLayout::Bgrx8 => {
            // BGRX → convert to RGB (strip padding byte, swizzle BGR→RGB)
            let pixels: Vec<rgb::Rgb<u8>> = bytes
                .chunks_exact(4)
                .map(|c| rgb::Rgb {
                    r: c[2],
                    g: c[1],
                    b: c[0],
                })
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
        PixelLayout::Rgba16 => {
            let pixels: Vec<rgb::Rgba<u16>> = bytes
                .chunks_exact(8)
                .map(|c| rgb::Rgba {
                    r: u16::from_ne_bytes([c[0], c[1]]),
                    g: u16::from_ne_bytes([c[2], c[3]]),
                    b: u16::from_ne_bytes([c[4], c[5]]),
                    a: u16::from_ne_bytes([c[6], c[7]]),
                })
                .collect();
            Ok(PixelBuffer::from_imgvec(imgref::ImgVec::new(pixels, w, h)).into())
        }
    }
}

/// Build a zencodec DecodeOutput from an internal DecodeOutput.
pub(crate) fn decode_output_from_internal(
    decoded: &crate::decode::DecodeOutput<'_>,
    format: ImageFormat,
) -> Result<DecodeOutput, BitmapError> {
    let has_alpha = matches!(
        decoded.layout,
        crate::PixelLayout::Rgba8 | crate::PixelLayout::Bgra8 | crate::PixelLayout::Rgba16
    );
    let info = ImageInfo::new(decoded.width, decoded.height, format)
        .with_alpha(has_alpha)
        .with_source_encoding_details(BitmapSourceEncoding);
    let pixels = layout_to_pixel_buffer(decoded)?;
    Ok(DecodeOutput::new(pixels, info).with_source_encoding_details(BitmapSourceEncoding))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
    use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

    /// Helper: encode via the four-layer flow (type-erased).
    fn encode_pixels(slice: PixelSlice<'_>) -> EncodeOutput {
        let config = PnmEncoderConfig::new();
        config.job().encoder().unwrap().encode(slice).unwrap()
    }

    /// Helper: decode via the four-layer flow.
    fn decode_bytes(data: &[u8]) -> DecodeOutput {
        let config = PnmDecoderConfig::new();
        config
            .job()
            .decoder(Cow::Borrowed(data), &[])
            .unwrap()
            .decode()
            .unwrap()
    }

    #[test]
    fn encode_decode_rgb8_roundtrip() {
        let pixels = vec![
            rgb::Rgb { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());
        assert_eq!(output.format(), ImageFormat::Pnm);

        let decoded = decode_bytes(output.data());
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
        let buf = decoded.into_buffer();
        let rgb_img = buf.try_as_imgref::<rgb::Rgb<u8>>().unwrap();
        assert_eq!(rgb_img.buf(), &pixels);
    }

    #[test]
    fn encode_decode_gray8_roundtrip() {
        let pixels = vec![
            rgb::Gray::new(0u8),
            rgb::Gray::new(128),
            rgb::Gray::new(255),
            rgb::Gray::new(64),
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        let buf = decoded.into_buffer();
        let gray_img = buf.try_as_imgref::<rgb::Gray<u8>>().unwrap();
        assert_eq!(gray_img.buf(), &pixels);
    }

    #[test]
    fn encode_decode_rgba8_roundtrip() {
        let pixels = vec![
            rgb::Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            rgb::Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 128,
            },
            rgb::Rgba {
                r: 0,
                g: 0,
                b: 255,
                a: 0,
            },
            rgb::Rgba {
                r: 128,
                g: 128,
                b: 128,
                a: 255,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        assert!(decoded.has_alpha());
        let buf = decoded.into_buffer();
        let rgba_img = buf.try_as_imgref::<rgb::Rgba<u8>>().unwrap();
        assert_eq!(rgba_img.buf(), &pixels);
    }

    #[test]
    fn encode_bgra8_no_double_swizzle() {
        // BGRA encode should go directly to PPM via zenbitmaps's native BGRA→RGB
        // path, not through the default trait BGRA→RGBA→PAM path.
        let pixels = vec![
            rgb::alt::BGRA {
                b: 0,
                g: 0,
                r: 255,
                a: 255,
            },
            rgb::alt::BGRA {
                b: 0,
                g: 255,
                r: 0,
                a: 255,
            },
            rgb::alt::BGRA {
                b: 255,
                g: 0,
                r: 0,
                a: 255,
            },
            rgb::alt::BGRA {
                b: 128,
                g: 128,
                r: 128,
                a: 255,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        let buf = decoded.into_buffer();
        let rgb_img = buf.try_as_imgref::<rgb::Rgb<u8>>().unwrap();
        let rgb_buf = rgb_img.buf();
        assert_eq!(rgb_buf[0], rgb::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(rgb_buf[1], rgb::Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(rgb_buf[2], rgb::Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(
            rgb_buf[3],
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128
            }
        );
    }

    #[test]
    fn probe_extracts_info() {
        let pixels = vec![rgb::Rgb::<u8> { r: 1, g: 2, b: 3 }; 6];
        let img = imgref::ImgVec::new(pixels, 3, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let dec = PnmDecoderConfig::new();
        let info = dec.job().probe(output.data()).unwrap();
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 2);
        assert_eq!(info.format, ImageFormat::Pnm);
        assert!(!info.has_alpha);
    }

    #[test]
    fn capabilities_are_correct() {
        let enc_caps = PnmEncoderConfig::capabilities();
        assert!(enc_caps.native_gray());
        assert!(enc_caps.native_alpha());
        assert!(enc_caps.native_f32());
        assert!(enc_caps.hdr());
        assert!(enc_caps.lossless());
        assert!(enc_caps.stop());
        assert!(enc_caps.enforces_max_pixels());
        assert!(!enc_caps.icc());
        assert!(!enc_caps.lossy());

        let dec_caps = PnmDecoderConfig::capabilities();
        assert!(dec_caps.native_gray());
        assert!(dec_caps.native_alpha());
        assert!(dec_caps.native_16bit());
        assert!(dec_caps.native_f32());
        assert!(dec_caps.hdr());
        assert!(dec_caps.cheap_probe());
        assert!(dec_caps.stop());
        assert!(dec_caps.enforces_max_pixels());
        assert!(!dec_caps.icc());
    }

    #[test]
    fn with_limits_propagates() {
        let limits = ResourceLimits::none()
            .with_max_width(10)
            .with_max_height(10);

        let big_pixels = vec![rgb::Rgb::<u8> { r: 0, g: 0, b: 0 }; 100 * 100];
        let img = imgref::ImgVec::new(big_pixels, 100, 100);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let dec = PnmDecoderConfig::new();
        let result = dec
            .job()
            .with_limits(limits)
            .decoder(Cow::Borrowed(output.data()), &[])
            .unwrap()
            .decode();
        assert!(result.is_err());
    }

    #[test]
    fn decode_rgb_pixel_data() {
        let pixels = vec![
            rgb::Rgb::<u8> { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        // Verify pixel data through PixelSlice
        let ps = decoded.pixels();
        assert_eq!(ps.width(), 2);
        assert_eq!(ps.rows(), 2);
        // First pixel should be red (255, 0, 0)
        let row0 = ps.row(0);
        assert_eq!(row0[0], 255); // R
        assert_eq!(row0[1], 0); // G
        assert_eq!(row0[2], 0); // B
    }

    #[test]
    fn encode_decode_rgb_f32_roundtrip() {
        let pixels = vec![
            rgb::Rgb {
                r: 0.0f32,
                g: 0.5,
                b: 1.0,
            },
            rgb::Rgb {
                r: 0.25,
                g: 0.75,
                b: 0.125,
            },
            rgb::Rgb {
                r: 1.0,
                g: 0.0,
                b: 0.0,
            },
            rgb::Rgb {
                r: 0.5,
                g: 0.5,
                b: 0.5,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());
        assert_eq!(output.format(), ImageFormat::Pnm);

        let decoded = decode_bytes(output.data());
        // RgbF32 gets promoted to RgbaF32 in the decode path
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
    }

    #[test]
    fn encode_decode_gray_f32_roundtrip() {
        let pixels = vec![
            rgb::Gray::new(0.0f32),
            rgb::Gray::new(0.25),
            rgb::Gray::new(0.5),
            rgb::Gray::new(1.0),
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let decoded = decode_bytes(output.data());
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
    }

    #[test]
    fn encoding_clone_send_sync() {
        fn assert_traits<T: Clone + Send + Sync>() {}
        assert_traits::<PnmEncoderConfig>();
    }

    #[test]
    fn decoding_clone_send_sync() {
        fn assert_traits<T: Clone + Send + Sync>() {}
        assert_traits::<PnmDecoderConfig>();
    }

    #[test]
    fn output_info_matches_decode() {
        let pixels = vec![rgb::Rgb::<u8> { r: 1, g: 2, b: 3 }; 6];
        let img = imgref::ImgVec::new(pixels, 3, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let dec = PnmDecoderConfig::new();
        let info = dec.job().output_info(output.data()).unwrap();
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 2);

        let decoded = decode_bytes(output.data());
        assert_eq!(decoded.width(), info.width);
        assert_eq!(decoded.height(), info.height);
    }

    #[test]
    fn four_layer_encode_flow() {
        let pixels = vec![
            rgb::Rgb::<u8> { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let config = PnmEncoderConfig::new();

        let slice = PixelSlice::from(img.as_ref()).erase();
        let output = config.job().encoder().unwrap().encode(slice).unwrap();
        assert_eq!(output.format(), ImageFormat::Pnm);
        assert!(!output.data().is_empty());
    }

    #[test]
    fn four_layer_decode_flow() {
        let pixels = vec![
            rgb::Rgb::<u8> {
                r: 100,
                g: 200,
                b: 50
            };
            4
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let output = encode_pixels(PixelSlice::from(img.as_ref()).erase());

        let config = PnmDecoderConfig::new();
        let decoded = config
            .job()
            .decoder(Cow::Borrowed(output.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
    }

    #[test]
    fn farbfeld_decode_has_alpha() {
        // Farbfeld is always RGBA16 — decoded output must report has_alpha: true.
        let pixels = vec![
            rgb::Rgba::<u8> {
                r: 255,
                g: 0,
                b: 0,
                a: 128,
            },
            rgb::Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            },
            rgb::Rgba {
                r: 0,
                g: 0,
                b: 255,
                a: 0,
            },
            rgb::Rgba {
                r: 128,
                g: 128,
                b: 128,
                a: 255,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let ff_config = FarbfeldEncoderConfig::new();
        let encoded = ff_config
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();
        assert_eq!(encoded.format(), ImageFormat::Farbfeld);

        let dec_config = FarbfeldDecoderConfig::new();
        let decoded = dec_config
            .job()
            .decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        assert!(
            decoded.has_alpha(),
            "farbfeld RGBA16 decode must report has_alpha"
        );
    }

    #[test]
    fn farbfeld_probe_has_alpha() {
        // Farbfeld probe should also report has_alpha: true.
        let pixels = vec![
            rgb::Rgba::<u8> {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            };
            4
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let encoded = FarbfeldEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let info = FarbfeldDecoderConfig::new()
            .job()
            .probe(encoded.data())
            .unwrap();
        assert!(info.has_alpha, "farbfeld probe must report has_alpha");
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_encode_decode_rgb8_roundtrip() {
        let pixels = vec![
            rgb::Rgb { r: 255, g: 0, b: 0 },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb {
                r: 42,
                g: 42,
                b: 42,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();
        assert_eq!(encoded.format(), ImageFormat::Qoi);

        let decoded = QoiDecoderConfig::new()
            .job()
            .decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        let buf = decoded.into_buffer();
        let rgb_img = buf.try_as_imgref::<rgb::Rgb<u8>>().unwrap();
        assert_eq!(rgb_img.buf(), &pixels);
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_encode_decode_rgba8_roundtrip() {
        let pixels = vec![
            rgb::Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            rgb::Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 128,
            },
            rgb::Rgba {
                r: 0,
                g: 0,
                b: 255,
                a: 0,
            },
            rgb::Rgba {
                r: 42,
                g: 42,
                b: 42,
                a: 200,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let decoded = QoiDecoderConfig::new()
            .job()
            .decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        assert!(decoded.has_alpha());
        let buf = decoded.into_buffer();
        let rgba_img = buf.try_as_imgref::<rgb::Rgba<u8>>().unwrap();
        assert_eq!(rgba_img.buf(), &pixels);
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_probe_extracts_info() {
        let pixels = vec![
            rgb::Rgba::<u8> {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            };
            4
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let info = QoiDecoderConfig::new().job().probe(encoded.data()).unwrap();
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 2);
        assert!(info.has_alpha);
        assert_eq!(info.source_color.bit_depth, Some(8));
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_streaming_decode_rgb() {
        use zencodec::decode::{DecodeJob, DecoderConfig, StreamingDecode};

        let pixels = vec![
            rgb::Rgb {
                r: 255u8,
                g: 0,
                b: 0,
            },
            rgb::Rgb { r: 0, g: 255, b: 0 },
            rgb::Rgb { r: 0, g: 0, b: 255 },
            rgb::Rgb {
                r: 42,
                g: 42,
                b: 42,
            },
        ];
        let img = imgref::ImgVec::new(pixels.clone(), 2, 2);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let mut stream = QoiDecoderConfig::new()
            .job()
            .streaming_decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap();

        assert_eq!(stream.info().width, 2);
        assert_eq!(stream.info().height, 2);

        // Row 0
        let (y, batch) = stream.next_batch().unwrap().unwrap();
        assert_eq!(y, 0);
        assert_eq!(batch.rows(), 1);
        assert_eq!(batch.width(), 2);
        let row0 = batch.contiguous_bytes();
        assert_eq!(&row0[..], &[255, 0, 0, 0, 255, 0]);

        // Row 1
        let (y, batch) = stream.next_batch().unwrap().unwrap();
        assert_eq!(y, 1);
        let row1 = batch.contiguous_bytes();
        assert_eq!(&row1[..], &[0, 0, 255, 42, 42, 42]);

        // Done
        assert!(stream.next_batch().unwrap().is_none());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_streaming_encode_roundtrip() {
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let row0: Vec<rgb::Rgb<u8>> = vec![
            rgb::Rgb {
                r: 10u8,
                g: 20,
                b: 30,
            },
            rgb::Rgb {
                r: 40,
                g: 50,
                b: 60,
            },
        ];
        let row1: Vec<rgb::Rgb<u8>> = vec![
            rgb::Rgb {
                r: 70u8,
                g: 80,
                b: 90,
            },
            rgb::Rgb {
                r: 100,
                g: 110,
                b: 120,
            },
        ];

        let mut encoder = QoiEncoderConfig::new().job().encoder().unwrap();

        let img0 = imgref::ImgVec::new(row0.clone(), 2, 1);
        encoder
            .push_rows(PixelSlice::from(img0.as_ref()).erase())
            .unwrap();

        let img1 = imgref::ImgVec::new(row1.clone(), 2, 1);
        encoder
            .push_rows(PixelSlice::from(img1.as_ref()).erase())
            .unwrap();

        let output = encoder.finish().unwrap();
        assert_eq!(output.format(), ImageFormat::Qoi);

        // Decode and verify
        let decoded = QoiDecoderConfig::new()
            .job()
            .decoder(Cow::Borrowed(output.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        let buf = decoded.into_buffer();
        let result = buf.try_as_imgref::<rgb::Rgb<u8>>().unwrap();
        assert_eq!(result.width(), 2);
        assert_eq!(result.height(), 2);
        let all_pixels: Vec<rgb::Rgb<u8>> = row0.into_iter().chain(row1).collect();
        assert_eq!(result.buf(), &all_pixels);
    }

    #[test]
    fn pnm_decode_descriptors_exclude_rgba16() {
        // PNM decoder downscales non-gray 16-bit to 8-bit, so RGBA16 must
        // not appear in the supported decode descriptors.
        let descs = PnmDecoderConfig::supported_descriptors();
        assert!(
            !descs.contains(&PixelDescriptor::RGBA16_SRGB),
            "PNM_DECODE_DESCRIPTORS should not contain RGBA16_SRGB"
        );
        // Gray16 IS preserved, so it should be present.
        assert!(
            descs.contains(&PixelDescriptor::GRAY16_SRGB),
            "PNM_DECODE_DESCRIPTORS should contain GRAY16_SRGB"
        );
    }

    // ── QOI zencodec trait tests ────────────────────────────────────────

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_capabilities_correct() {
        use zencodec::decode::DecoderConfig;
        use zencodec::encode::EncoderConfig;

        let enc_caps = QoiEncoderConfig::capabilities();
        assert!(enc_caps.lossless());
        assert!(enc_caps.native_alpha());
        assert!(enc_caps.stop());
        assert!(!enc_caps.native_gray());
        assert!(!enc_caps.native_f32());

        let dec_caps = QoiDecoderConfig::capabilities();
        assert!(dec_caps.cheap_probe());
        assert!(dec_caps.native_alpha());
        assert!(dec_caps.streaming());
        assert!(dec_caps.stop());
        assert!(!dec_caps.native_gray());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_encode_descriptors() {
        use zencodec::encode::EncoderConfig;
        let descs = QoiEncoderConfig::supported_descriptors();
        assert!(descs.contains(&PixelDescriptor::RGB8_SRGB));
        assert!(descs.contains(&PixelDescriptor::RGBA8_SRGB));
        assert!(descs.contains(&PixelDescriptor::BGRA8_SRGB));
        assert!(!descs.contains(&PixelDescriptor::GRAY8_SRGB));
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_decode_descriptors() {
        use zencodec::decode::DecoderConfig;
        let descs = QoiDecoderConfig::supported_descriptors();
        assert!(descs.contains(&PixelDescriptor::RGB8_SRGB));
        assert!(descs.contains(&PixelDescriptor::RGBA8_SRGB));
        assert!(!descs.contains(&PixelDescriptor::BGRA8_SRGB));
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_output_info() {
        use zencodec::decode::{DecodeJob, DecoderConfig};
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let pixels = vec![
            rgb::Rgba::<u8> {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            };
            4
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let info = QoiDecoderConfig::new()
            .job()
            .output_info(encoded.data())
            .unwrap();
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 2);
        assert!(info.has_alpha);
        assert_eq!(info.native_format, PixelDescriptor::RGBA8_SRGB);
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_animation_rejected() {
        use zencodec::decode::{DecodeJob, DecoderConfig};
        use zencodec::encode::{EncodeJob, EncoderConfig};

        // Animation encode
        let result = QoiEncoderConfig::new().job().animation_frame_encoder();
        assert!(result.is_err());

        // Animation decode
        let pixels = vec![rgb::Rgb::<u8> { r: 0, g: 0, b: 0 }; 1];
        let img = imgref::ImgVec::new(pixels, 1, 1);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();
        let result = QoiDecoderConfig::new()
            .job()
            .animation_frame_decoder(Cow::Borrowed(encoded.data()), &[]);
        assert!(result.is_err());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_with_limits_decode() {
        use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let pixels = vec![rgb::Rgb::<u8> { r: 0, g: 0, b: 0 }; 100];
        let img = imgref::ImgVec::new(pixels, 10, 10);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        // max_width too small
        let limits = ResourceLimits::none().with_max_width(5);
        let result = QoiDecoderConfig::new()
            .job()
            .with_limits(limits)
            .decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap()
            .decode();
        assert!(result.is_err());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_with_limits_streaming() {
        use zencodec::decode::{DecodeJob, DecoderConfig};
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let pixels = vec![rgb::Rgb::<u8> { r: 0, g: 0, b: 0 }; 100];
        let img = imgref::ImgVec::new(pixels, 10, 10);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        // max_height too small — should fail at streaming_decoder creation
        let limits = ResourceLimits::none().with_max_height(5);
        let result = QoiDecoderConfig::new()
            .job()
            .with_limits(limits)
            .streaming_decoder(Cow::Borrowed(encoded.data()), &[]);
        assert!(result.is_err());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_max_input_bytes_limit() {
        use zencodec::decode::{DecodeJob, DecoderConfig};
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let pixels = vec![rgb::Rgb::<u8> { r: 0, g: 0, b: 0 }; 4];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let limits = ResourceLimits::none().with_max_input_bytes(5); // too small
        let result = QoiDecoderConfig::new()
            .job()
            .with_limits(limits)
            .decoder(Cow::Borrowed(encoded.data()), &[]);
        assert!(result.is_err());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_streaming_decode_rgba() {
        use zencodec::decode::{DecodeJob, DecoderConfig, StreamingDecode};
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let pixels = vec![
            rgb::Rgba {
                r: 255u8,
                g: 0,
                b: 0,
                a: 255,
            },
            rgb::Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 128,
            },
            rgb::Rgba {
                r: 0,
                g: 0,
                b: 255,
                a: 64,
            },
            rgb::Rgba {
                r: 42,
                g: 42,
                b: 42,
                a: 0,
            },
        ];
        let img = imgref::ImgVec::new(pixels, 2, 2);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let mut stream = QoiDecoderConfig::new()
            .job()
            .streaming_decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap();

        assert!(stream.info().has_alpha);

        // Row 0
        let (y, batch) = stream.next_batch().unwrap().unwrap();
        assert_eq!(y, 0);
        let row0 = batch.contiguous_bytes();
        assert_eq!(&row0[..], &[255, 0, 0, 255, 0, 255, 0, 128]);

        // Row 1
        let (y, batch) = stream.next_batch().unwrap().unwrap();
        assert_eq!(y, 1);
        let row1 = batch.contiguous_bytes();
        assert_eq!(&row1[..], &[0, 0, 255, 64, 42, 42, 42, 0]);

        // Done
        assert!(stream.next_batch().unwrap().is_none());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_streaming_decode_1x1() {
        use zencodec::decode::{DecodeJob, DecoderConfig, StreamingDecode};
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let pixels = vec![rgb::Rgb {
            r: 99u8,
            g: 88,
            b: 77,
        }];
        let img = imgref::ImgVec::new(pixels, 1, 1);
        let encoded = QoiEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(PixelSlice::from(img.as_ref()).erase())
            .unwrap();

        let mut stream = QoiDecoderConfig::new()
            .job()
            .streaming_decoder(Cow::Borrowed(encoded.data()), &[])
            .unwrap();

        let (y, batch) = stream.next_batch().unwrap().unwrap();
        assert_eq!(y, 0);
        assert_eq!(batch.width(), 1);
        assert_eq!(batch.rows(), 1);
        assert_eq!(&batch.contiguous_bytes()[..], &[99, 88, 77]);

        assert!(stream.next_batch().unwrap().is_none());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_streaming_encode_rgba_roundtrip() {
        use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let row0 = vec![
            rgb::Rgba {
                r: 10u8,
                g: 20,
                b: 30,
                a: 255
            };
            3
        ];
        let row1 = vec![
            rgb::Rgba {
                r: 40u8,
                g: 50,
                b: 60,
                a: 128
            };
            3
        ];

        let mut encoder = QoiEncoderConfig::new().job().encoder().unwrap();

        let img0 = imgref::ImgVec::new(row0.clone(), 3, 1);
        encoder
            .push_rows(PixelSlice::from(img0.as_ref()).erase())
            .unwrap();

        let img1 = imgref::ImgVec::new(row1.clone(), 3, 1);
        encoder
            .push_rows(PixelSlice::from(img1.as_ref()).erase())
            .unwrap();

        let output = encoder.finish().unwrap();

        let decoded = QoiDecoderConfig::new()
            .job()
            .decoder(Cow::Borrowed(output.data()), &[])
            .unwrap()
            .decode()
            .unwrap();
        assert!(decoded.has_alpha());
        let buf = decoded.into_buffer();
        let result = buf.try_as_imgref::<rgb::Rgba<u8>>().unwrap();
        assert_eq!(result.width(), 3);
        assert_eq!(result.height(), 2);
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_streaming_encode_width_mismatch_error() {
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let mut encoder = QoiEncoderConfig::new().job().encoder().unwrap();

        let row0 = vec![rgb::Rgb { r: 0u8, g: 0, b: 0 }; 3];
        let img0 = imgref::ImgVec::new(row0, 3, 1);
        encoder
            .push_rows(PixelSlice::from(img0.as_ref()).erase())
            .unwrap();

        // Different width — should error
        let row1 = vec![rgb::Rgb { r: 0u8, g: 0, b: 0 }; 5];
        let img1 = imgref::ImgVec::new(row1, 5, 1);
        let result = encoder.push_rows(PixelSlice::from(img1.as_ref()).erase());
        assert!(result.is_err());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_streaming_encode_finish_without_push_errors() {
        use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};

        let encoder = QoiEncoderConfig::new().job().encoder().unwrap();
        let result = encoder.finish();
        assert!(result.is_err());
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_is_lossless() {
        use zencodec::encode::EncoderConfig;
        let config = QoiEncoderConfig::new();
        assert_eq!(config.is_lossless(), Some(true));
    }

    #[cfg(feature = "qoi")]
    #[test]
    fn qoi_format_is_qoi() {
        use zencodec::decode::DecoderConfig;
        use zencodec::encode::EncoderConfig;
        assert_eq!(QoiEncoderConfig::format(), ImageFormat::Qoi);
        assert_eq!(QoiDecoderConfig::formats(), &[ImageFormat::Qoi]);
    }

    #[cfg(feature = "bmp")]
    #[test]
    fn bmp_capabilities_include_native_gray() {
        let caps = BmpDecoderConfig::capabilities();
        assert!(
            caps.native_gray(),
            "BMP decode capabilities should include native_gray"
        );
        assert!(caps.native_alpha());
        assert!(caps.cheap_probe());
    }

    #[cfg(feature = "bmp")]
    #[test]
    fn bmp_decode_descriptors_include_gray8() {
        let descs = BmpDecoderConfig::supported_descriptors();
        assert!(
            descs.contains(&PixelDescriptor::GRAY8_SRGB),
            "BMP_DECODE_DESCRIPTORS should contain GRAY8_SRGB"
        );
    }
}
