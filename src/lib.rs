//! # zenbitmaps
//!
//! PNM/PAM/PFM, BMP, and farbfeld image format decoder and encoder.
//!
//! Reference bitmap formats for codec testing and apples-to-apples comparisons.
//!
//! ## Zero-Copy Decoding
//!
//! For PNM files with maxval=255 (the common case), decoding returns a borrowed
//! slice into the input buffer — no allocation or copy needed. Formats that
//! require transformation (BMP row flip, farbfeld endian swap, etc.) allocate.
//!
//! ## Supported Formats
//!
//! ### PNM family (always available)
//! - **P5** (PGM binary) — grayscale, 8-bit and 16-bit
//! - **P6** (PPM binary) — RGB, 8-bit and 16-bit
//! - **P7** (PAM) — arbitrary channels (grayscale, RGB, RGBA), 8-bit and 16-bit
//! - **PFM** — floating-point grayscale and RGB (32-bit float per channel)
//!
//! ### Farbfeld (always available)
//! - RGBA 16-bit per channel
//! - Auto-detected by `decode()` via `"farbfeld"` magic
//!
//! ### BMP (`bmp` feature, opt-in)
//! - All standard bit depths: 1, 2, 4, 8, 16, 24, 32
//! - Compression: uncompressed, RLE4, RLE8, BITFIELDS
//! - Palette expansion, bottom-up/top-down, grayscale detection
//! - Auto-detected by `decode()` via `"BM"` magic
//!
//! ## Usage
//!
//! ```no_run
//! use zenbitmaps::*;
//! use enough::Unstoppable;
//!
//! // Encode pixels to PPM
//! let pixels = vec![255u8, 0, 0, 0, 255, 0]; // 2 RGB pixels
//! let encoded = encode_ppm(&pixels, 2, 1, PixelLayout::Rgb8, Unstoppable)?;
//!
//! // Decode (auto-detects PNM/BMP/farbfeld, zero-copy when possible)
//! let decoded = decode(&encoded, Unstoppable)?;
//! assert!(decoded.is_borrowed()); // zero-copy for PPM with maxval=255
//! assert_eq!(decoded.pixels(), &pixels[..]);
//! # Ok::<(), zenbitmaps::BitmapError>(())
//! ```
//!
//! ## Credits
//!
//! - PNM: draws from [zune-ppm](https://github.com/etemesi254/zune-image)
//!   by Caleb Etemesi (MIT/Apache-2.0/Zlib)
//! - BMP: forked from [zune-bmp](https://github.com/etemesi254/zune-image) 0.5.2
//!   by Caleb Etemesi (MIT/Apache-2.0/Zlib)
//! - Farbfeld: forked from [zune-farbfeld](https://github.com/etemesi254/zune-image) 0.5.2
//!   by Caleb Etemesi (MIT/Apache-2.0/Zlib)

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(feature = "rgb")]
use rgb::{AsPixels as _, ComponentBytes as _};

mod decode;
mod error;
mod limits;
mod pixel;

mod pnm;

mod farbfeld;

#[cfg(feature = "bmp")]
mod bmp;

#[cfg(feature = "rgb")]
mod pixel_traits;

#[cfg(feature = "zencodec")]
mod zencodec;

pub use decode::DecodeOutput;
pub use enough::{Stop, Unstoppable};
pub use error::BitmapError;
pub use limits::Limits;
pub use pixel::{ImageFormat, PixelLayout};

#[cfg(feature = "bmp")]
pub use bmp::BmpPermissiveness;

#[cfg(feature = "rgb")]
pub use pixel_traits::{DecodePixel, EncodePixel};

#[cfg(feature = "zencodec")]
pub use zencodec::{
    PnmDecodeJob, PnmDecoder, PnmDecoderConfig, PnmEncodeJob, PnmEncoder, PnmEncoderConfig,
    PnmFrameDecoder, PnmFrameEncoder,
};

#[cfg(all(feature = "zencodec", feature = "bmp"))]
pub use zencodec::{
    BmpDecodeJob, BmpDecoder, BmpDecoderConfig, BmpEncodeJob, BmpEncoder, BmpEncoderConfig,
    BmpFrameDecoder, BmpFrameEncoder,
};

#[cfg(feature = "zencodec")]
pub use zencodec::{
    FarbfeldDecodeJob, FarbfeldDecoder, FarbfeldDecoderConfig, FarbfeldEncodeJob, FarbfeldEncoder,
    FarbfeldEncoderConfig, FarbfeldFrameDecoder, FarbfeldFrameEncoder,
};

// Re-export rgb pixel types for convenience
#[cfg(feature = "rgb")]
pub use rgb::RGB as Rgb;
#[cfg(feature = "rgb")]
pub use rgb::RGBA as Rgba;
#[cfg(feature = "rgb")]
pub use rgb::alt::BGR as Bgr;
#[cfg(feature = "rgb")]
pub use rgb::alt::BGRA as Bgra;

/// 8-bit RGB pixel.
#[cfg(feature = "rgb")]
pub type RGB8 = rgb::RGB<u8>;
/// 8-bit RGBA pixel.
#[cfg(feature = "rgb")]
pub type RGBA8 = rgb::RGBA<u8>;
/// 8-bit BGR pixel.
#[cfg(feature = "rgb")]
pub type BGR8 = rgb::alt::BGR<u8>;
/// 8-bit BGRA pixel.
#[cfg(feature = "rgb")]
pub type BGRA8 = rgb::alt::BGRA<u8>;

// ── Format detection ──────────────────────────────────────────────────

/// Detect image format from magic bytes.
///
/// Returns `None` if the data doesn't match any supported format's magic bytes.
/// Recognized formats: BMP (`BM`), farbfeld (`farbfeld`), PNM (`P5`/`P6`/`P7`/`Pf`/`PF`).
pub fn detect_format(data: &[u8]) -> Option<ImageFormat> {
    if data.len() >= 2 && data[0] == b'B' && data[1] == b'M' {
        return Some(ImageFormat::Bmp);
    }
    if data.len() >= 8 && &data[0..8] == b"farbfeld" {
        return Some(ImageFormat::Farbfeld);
    }
    // PNM magic: P followed by 5, 6, 7, f, or F
    if data.len() >= 2 && data[0] == b'P' {
        match data[1] {
            b'5' | b'6' | b'7' | b'f' | b'F' => return Some(ImageFormat::Pnm),
            _ => {}
        }
    }
    None
}

// ── Auto-detect decode (PNM, BMP, farbfeld from magic bytes) ─────────

/// Decode any supported format (auto-detected from magic bytes).
///
/// Detects PNM (P5/P6/P7/PFM), farbfeld, and BMP (if the `bmp` feature is enabled).
/// Zero-copy when possible — PNM with maxval=255 returns a borrowed slice.
pub fn decode(data: &[u8], stop: impl Stop) -> Result<DecodeOutput<'_>, BitmapError> {
    decode_dispatch(data, None, &stop)
}

/// Decode any supported format with resource limits.
pub fn decode_with_limits<'a>(
    data: &'a [u8],
    limits: &'a Limits,
    stop: impl Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    decode_dispatch(data, Some(limits), &stop)
}

fn decode_dispatch<'a>(
    data: &'a [u8],
    limits: Option<&Limits>,
    stop: &dyn enough::Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    match detect_format(data) {
        Some(ImageFormat::Bmp) => {
            #[cfg(feature = "bmp")]
            return bmp::decode(data, limits, stop);
            #[cfg(not(feature = "bmp"))]
            return Err(BitmapError::UnsupportedVariant(
                "BMP support requires the 'bmp' feature".into(),
            ));
        }
        Some(ImageFormat::Farbfeld) => farbfeld::decode(data, limits, stop),
        Some(ImageFormat::Pnm) => pnm::decode(data, limits, stop),
        None => Err(BitmapError::UnrecognizedFormat),
    }
}

// ── PNM encode ───────────────────────────────────────────────────────

/// Encode pixels as PPM (P6, binary RGB).
pub fn encode_ppm(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
    pnm::encode(pixels, width, height, layout, pnm::PnmFormat::Ppm, &stop)
}

/// Encode pixels as PGM (P5, binary grayscale).
pub fn encode_pgm(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
    pnm::encode(pixels, width, height, layout, pnm::PnmFormat::Pgm, &stop)
}

/// Encode pixels as PAM (P7, arbitrary channels).
pub fn encode_pam(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
    pnm::encode(pixels, width, height, layout, pnm::PnmFormat::Pam, &stop)
}

/// Encode pixels as PFM (floating-point).
pub fn encode_pfm(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
    pnm::encode(pixels, width, height, layout, pnm::PnmFormat::Pfm, &stop)
}

// ── Farbfeld encode/decode ────────────────────────────────────────────

/// Decode farbfeld data to pixels.
///
/// Also auto-detected by [`decode()`] via the `"farbfeld"` magic bytes.
/// Output layout is always [`PixelLayout::Rgba16`].
pub fn decode_farbfeld(data: &[u8], stop: impl Stop) -> Result<DecodeOutput<'_>, BitmapError> {
    farbfeld::decode(data, None, &stop)
}

/// Decode farbfeld with resource limits.
pub fn decode_farbfeld_with_limits<'a>(
    data: &'a [u8],
    limits: &'a Limits,
    stop: impl Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    farbfeld::decode(data, Some(limits), &stop)
}

/// Encode pixels as farbfeld.
///
/// Accepts `Rgba16` (direct), `Rgba8` (expand via val*257),
/// `Rgb8` (expand + alpha=65535), or `Gray8` (expand to RGBA).
pub fn encode_farbfeld(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
    farbfeld::encode(pixels, width, height, layout, &stop)
}

// ── BMP (auto-detected, or explicit) ─────────────────────────────────

/// Decode BMP data to pixels.
///
/// Also auto-detected by [`decode()`] via the `"BM"` magic bytes.
/// BMP always allocates (BGR→RGB conversion + row flip).
#[cfg(feature = "bmp")]
pub fn decode_bmp(data: &[u8], stop: impl Stop) -> Result<DecodeOutput<'_>, BitmapError> {
    bmp::decode(data, None, &stop)
}

/// Decode BMP with resource limits.
#[cfg(feature = "bmp")]
pub fn decode_bmp_with_limits<'a>(
    data: &'a [u8],
    limits: &'a Limits,
    stop: impl Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    bmp::decode(data, Some(limits), &stop)
}

/// Decode BMP data in native byte order (BGR for 24-bit, BGRA for 32-bit).
///
/// Unlike [`decode_bmp`], this skips the BGR→RGB channel swizzle,
/// returning pixels in the BMP-native byte order. The output layout will be
/// [`PixelLayout::Bgr8`], [`PixelLayout::Bgra8`], or [`PixelLayout::Gray8`].
#[cfg(feature = "bmp")]
pub fn decode_bmp_native(data: &[u8], stop: impl Stop) -> Result<DecodeOutput<'_>, BitmapError> {
    bmp::decode_native(data, None, &stop)
}

/// Decode BMP in native byte order with resource limits.
#[cfg(feature = "bmp")]
pub fn decode_bmp_native_with_limits<'a>(
    data: &'a [u8],
    limits: &'a Limits,
    stop: impl Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    bmp::decode_native(data, Some(limits), &stop)
}

/// Decode BMP with a specific permissiveness level.
///
/// - [`BmpPermissiveness::Strict`]: reject any spec violation
/// - [`BmpPermissiveness::Standard`]: default, reject corrupted files but
///   accept benign metadata errors (bad DPI, wrong file size field)
/// - [`BmpPermissiveness::Permissive`]: best-effort recovery (zero-pad
///   truncated files, clamp RLE overflows, accept unknown compression)
#[cfg(feature = "bmp")]
pub fn decode_bmp_permissive(
    data: &[u8],
    permissiveness: BmpPermissiveness,
    stop: impl Stop,
) -> Result<DecodeOutput<'_>, BitmapError> {
    bmp::decode_with_permissiveness(data, None, permissiveness, &stop)
}

/// Decode BMP with a specific permissiveness level and resource limits.
#[cfg(feature = "bmp")]
pub fn decode_bmp_permissive_with_limits<'a>(
    data: &'a [u8],
    permissiveness: BmpPermissiveness,
    limits: &'a Limits,
    stop: impl Stop,
) -> Result<DecodeOutput<'a>, BitmapError> {
    bmp::decode_with_permissiveness(data, Some(limits), permissiveness, &stop)
}

/// Encode pixels as 24-bit BMP (RGB, no alpha).
#[cfg(feature = "bmp")]
pub fn encode_bmp(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
    bmp::encode(pixels, width, height, layout, false, &stop)
}

/// Encode pixels as 32-bit BMP (RGBA with alpha).
#[cfg(feature = "bmp")]
pub fn encode_bmp_rgba(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError> {
    bmp::encode(pixels, width, height, layout, true, &stop)
}

// ── Typed pixel API (rgb feature) ────────────────────────────────────

/// Decode any PNM format to typed pixels.
#[cfg(feature = "rgb")]
pub fn decode_pixels<P: DecodePixel>(
    data: &[u8],
    stop: impl Stop,
) -> Result<(alloc::vec::Vec<P>, u32, u32), BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let decoded = decode(data, stop)?;
    decoded_to_pixels(decoded)
}

/// Decode any PNM format to typed pixels with resource limits.
#[cfg(feature = "rgb")]
pub fn decode_pixels_with_limits<P: DecodePixel>(
    data: &[u8],
    limits: &Limits,
    stop: impl Stop,
) -> Result<(alloc::vec::Vec<P>, u32, u32), BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let decoded = decode_with_limits(data, limits, stop)?;
    decoded_to_pixels(decoded)
}

/// Decode BMP to typed pixels.
#[cfg(all(feature = "bmp", feature = "rgb"))]
pub fn decode_bmp_pixels<P: DecodePixel>(
    data: &[u8],
    stop: impl Stop,
) -> Result<(alloc::vec::Vec<P>, u32, u32), BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let decoded = decode_bmp(data, stop)?;
    decoded_to_pixels(decoded)
}

/// Decode BMP to typed pixels with resource limits.
#[cfg(all(feature = "bmp", feature = "rgb"))]
pub fn decode_bmp_pixels_with_limits<P: DecodePixel>(
    data: &[u8],
    limits: &Limits,
    stop: impl Stop,
) -> Result<(alloc::vec::Vec<P>, u32, u32), BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let decoded = decode_bmp_with_limits(data, limits, stop)?;
    decoded_to_pixels(decoded)
}

#[cfg(feature = "rgb")]
fn decoded_to_pixels<P: DecodePixel>(
    decoded: DecodeOutput<'_>,
) -> Result<(alloc::vec::Vec<P>, u32, u32), BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    if !decoded.layout.is_memory_compatible(P::layout()) {
        return Err(BitmapError::LayoutMismatch {
            expected: P::layout(),
            actual: decoded.layout,
        });
    }
    let pixels: &[P] = decoded.pixels().as_pixels();
    Ok((pixels.to_vec(), decoded.width, decoded.height))
}

// ── Typed pixel encode (rgb feature) ─────────────────────────────────

/// Encode typed pixels as PPM (P6).
#[cfg(feature = "rgb")]
pub fn encode_ppm_pixels<P: EncodePixel>(
    pixels: &[P],
    width: u32,
    height: u32,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    encode_ppm(pixels.as_bytes(), width, height, P::layout(), stop)
}

/// Encode typed pixels as PGM (P5).
#[cfg(feature = "rgb")]
pub fn encode_pgm_pixels<P: EncodePixel>(
    pixels: &[P],
    width: u32,
    height: u32,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    encode_pgm(pixels.as_bytes(), width, height, P::layout(), stop)
}

/// Encode typed pixels as PAM (P7).
#[cfg(feature = "rgb")]
pub fn encode_pam_pixels<P: EncodePixel>(
    pixels: &[P],
    width: u32,
    height: u32,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    encode_pam(pixels.as_bytes(), width, height, P::layout(), stop)
}

/// Encode typed pixels as PFM (floating-point).
#[cfg(feature = "rgb")]
pub fn encode_pfm_pixels<P: EncodePixel>(
    pixels: &[P],
    width: u32,
    height: u32,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    encode_pfm(pixels.as_bytes(), width, height, P::layout(), stop)
}

/// Encode typed pixels as 24-bit BMP.
#[cfg(all(feature = "bmp", feature = "rgb"))]
pub fn encode_bmp_pixels<P: EncodePixel>(
    pixels: &[P],
    width: u32,
    height: u32,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    encode_bmp(pixels.as_bytes(), width, height, P::layout(), stop)
}

/// Encode typed pixels as 32-bit BMP (RGBA).
#[cfg(all(feature = "bmp", feature = "rgb"))]
pub fn encode_bmp_rgba_pixels<P: EncodePixel>(
    pixels: &[P],
    width: u32,
    height: u32,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    encode_bmp_rgba(pixels.as_bytes(), width, height, P::layout(), stop)
}

// ── ImgVec/ImgRef API (imgref feature) ───────────────────────────────

/// Decode any PNM format to an [`imgref::ImgVec`].
#[cfg(feature = "imgref")]
pub fn decode_img<P: DecodePixel>(
    data: &[u8],
    stop: impl Stop,
) -> Result<imgref::ImgVec<P>, BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let (pixels, w, h) = decode_pixels::<P>(data, stop)?;
    Ok(imgref::ImgVec::new(pixels, w as usize, h as usize))
}

/// Decode any PNM format to an [`imgref::ImgVec`] with resource limits.
#[cfg(feature = "imgref")]
pub fn decode_img_with_limits<P: DecodePixel>(
    data: &[u8],
    limits: &Limits,
    stop: impl Stop,
) -> Result<imgref::ImgVec<P>, BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let (pixels, w, h) = decode_pixels_with_limits::<P>(data, limits, stop)?;
    Ok(imgref::ImgVec::new(pixels, w as usize, h as usize))
}

/// Decode BMP to an [`imgref::ImgVec`].
#[cfg(all(feature = "bmp", feature = "imgref"))]
pub fn decode_bmp_img<P: DecodePixel>(
    data: &[u8],
    stop: impl Stop,
) -> Result<imgref::ImgVec<P>, BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let (pixels, w, h) = decode_bmp_pixels::<P>(data, stop)?;
    Ok(imgref::ImgVec::new(pixels, w as usize, h as usize))
}

/// Decode BMP to an [`imgref::ImgVec`] with resource limits.
#[cfg(all(feature = "bmp", feature = "imgref"))]
pub fn decode_bmp_img_with_limits<P: DecodePixel>(
    data: &[u8],
    limits: &Limits,
    stop: impl Stop,
) -> Result<imgref::ImgVec<P>, BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let (pixels, w, h) = decode_bmp_pixels_with_limits::<P>(data, limits, stop)?;
    Ok(imgref::ImgVec::new(pixels, w as usize, h as usize))
}

/// Decode PNM into an existing [`imgref::ImgRefMut`] buffer.
///
/// The output buffer dimensions must match the decoded image exactly.
/// Handles arbitrary stride (row-by-row copy).
#[cfg(feature = "imgref")]
pub fn decode_into<P: DecodePixel>(
    data: &[u8],
    output: imgref::ImgRefMut<'_, P>,
    stop: impl Stop,
) -> Result<(), BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let decoded = decode(data, stop)?;
    copy_decoded_into(decoded, output)
}

/// Decode BMP into an existing [`imgref::ImgRefMut`] buffer.
#[cfg(all(feature = "bmp", feature = "imgref"))]
pub fn decode_bmp_into<P: DecodePixel>(
    data: &[u8],
    output: imgref::ImgRefMut<'_, P>,
    stop: impl Stop,
) -> Result<(), BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    let decoded = decode_bmp(data, stop)?;
    copy_decoded_into(decoded, output)
}

#[cfg(feature = "imgref")]
fn copy_decoded_into<P: DecodePixel>(
    decoded: DecodeOutput<'_>,
    mut output: imgref::ImgRefMut<'_, P>,
) -> Result<(), BitmapError>
where
    [u8]: rgb::AsPixels<P>,
{
    if !decoded.layout.is_memory_compatible(P::layout()) {
        return Err(BitmapError::LayoutMismatch {
            expected: P::layout(),
            actual: decoded.layout,
        });
    }
    let out_w = output.width();
    let out_h = output.height();
    if decoded.width as usize != out_w || decoded.height as usize != out_h {
        return Err(BitmapError::InvalidData(alloc::format!(
            "dimension mismatch: decoded {}x{}, output buffer {}x{}",
            decoded.width,
            decoded.height,
            out_w,
            out_h
        )));
    }
    let src_pixels: &[P] = decoded.pixels().as_pixels();
    for (src_row, dst_row) in src_pixels.chunks_exact(out_w).zip(output.rows_mut()) {
        <[P]>::copy_from_slice(dst_row, src_row);
    }
    Ok(())
}

/// Encode an [`imgref::ImgRef`] as PPM (P6).
///
/// Handles arbitrary stride by copying row-by-row when needed.
#[cfg(feature = "imgref")]
pub fn encode_ppm_img<P: EncodePixel>(
    img: imgref::ImgRef<'_, P>,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    let (bytes, w, h) = collect_img_bytes(img);
    encode_ppm(&bytes, w, h, P::layout(), stop)
}

/// Encode an [`imgref::ImgRef`] as PGM (P5).
#[cfg(feature = "imgref")]
pub fn encode_pgm_img<P: EncodePixel>(
    img: imgref::ImgRef<'_, P>,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    let (bytes, w, h) = collect_img_bytes(img);
    encode_pgm(&bytes, w, h, P::layout(), stop)
}

/// Encode an [`imgref::ImgRef`] as PAM (P7).
#[cfg(feature = "imgref")]
pub fn encode_pam_img<P: EncodePixel>(
    img: imgref::ImgRef<'_, P>,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    let (bytes, w, h) = collect_img_bytes(img);
    encode_pam(&bytes, w, h, P::layout(), stop)
}

/// Encode an [`imgref::ImgRef`] as PFM.
#[cfg(feature = "imgref")]
pub fn encode_pfm_img<P: EncodePixel>(
    img: imgref::ImgRef<'_, P>,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    let (bytes, w, h) = collect_img_bytes(img);
    encode_pfm(&bytes, w, h, P::layout(), stop)
}

/// Encode an [`imgref::ImgRef`] as 24-bit BMP.
#[cfg(all(feature = "bmp", feature = "imgref"))]
pub fn encode_bmp_img<P: EncodePixel>(
    img: imgref::ImgRef<'_, P>,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    let (bytes, w, h) = collect_img_bytes(img);
    encode_bmp(&bytes, w, h, P::layout(), stop)
}

/// Encode an [`imgref::ImgRef`] as 32-bit BMP (RGBA).
#[cfg(all(feature = "bmp", feature = "imgref"))]
pub fn encode_bmp_rgba_img<P: EncodePixel>(
    img: imgref::ImgRef<'_, P>,
    stop: impl Stop,
) -> Result<alloc::vec::Vec<u8>, BitmapError>
where
    [P]: rgb::ComponentBytes<u8>,
{
    let (bytes, w, h) = collect_img_bytes(img);
    encode_bmp_rgba(&bytes, w, h, P::layout(), stop)
}

/// Collect image rows into contiguous bytes, handling arbitrary stride.
#[cfg(feature = "imgref")]
fn collect_img_bytes<P: EncodePixel>(img: imgref::ImgRef<'_, P>) -> (alloc::vec::Vec<u8>, u32, u32)
where
    [P]: rgb::ComponentBytes<u8>,
{
    let w = img.width();
    let h = img.height();
    if img.stride() == w {
        // Contiguous — single memcpy, no intermediate Vec<P>
        let pixels = &img.buf()[..w * h];
        (pixels.as_bytes().to_vec(), w as u32, h as u32)
    } else {
        // Strided — collect row-by-row directly into bytes
        let bpp = core::mem::size_of::<P>();
        let mut bytes = alloc::vec::Vec::with_capacity(w * h * bpp);
        for row in img.rows() {
            bytes.extend_from_slice(row.as_bytes());
        }
        (bytes, w as u32, h as u32)
    }
}
