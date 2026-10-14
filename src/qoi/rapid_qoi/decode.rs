//! Vendored QOI decoder.
//!
//! Vendored from rapid-qoi v0.6.x (<https://github.com/zakarumych/rapid-qoi>),
//! © zakarumych, licensed MIT OR Apache-2.0.
//!
//! Local modifications relative to upstream:
//! * **Clamp fix** in the `QOI_OP_RUN` arm of [`Qoi::decode_range`]: the run
//!   length is clamped with `run.min(pixels.len())` before `split_at_mut`, so a
//!   run reaching the end of the output slice no longer panics with `mid > len`.
//!   This is the fix that lets a single run cross an output (row) boundary; the
//!   unfilled remainder is carried in `*prun` for the next call.
//! * The upstream `bytemuck::cast_slice_mut` call was replaced with the safe
//!   `<[u8]>::as_chunks_mut::<N>()` (stable since Rust 1.88) to avoid a new
//!   direct dependency. Behavior is identical: the assert guarantees the
//!   remainder is empty.
//!
//! No other algorithmic changes — pixel output is byte-identical to upstream.

use core::convert::TryInto;

use super::*;

use alloc::{vec, vec::Vec};

/// Errros that may occur during image decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DecodeError {
    /// Buffer does not contain enough encoded data.
    NotEnoughData,

    /// Encoded header contains invalid magic value.\
    /// First four bytes must contain `b"qoif"`.\
    /// This usually indicates that buffer does not contain QOI image.
    InvalidMagic,

    /// Encoded header contains invalud channels number.\
    /// QOI supports only images with `3` or `4` channels.\
    /// Any other value cannot be produced by valid encoder.
    InvalidChannelsValue,

    /// Encoded header contains invalud color space value.'
    /// QOI supports only images with SRGB color channels and linear alpha (if present) denoted by `0` and all linear channels denoted by `1`.\
    /// Any other value cannot be produced by valid encoder.
    InvalidColorSpaceValue,

    /// Output buffer is too small to fit decoded image.
    OutputIsTooSmall,
}

impl Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::NotEnoughData => {
                f.write_str("Buffer does not contain enough encoded data")
            }
            DecodeError::InvalidMagic => f.write_str("Encoded header contains invalid magic value"),
            DecodeError::InvalidChannelsValue => {
                f.write_str("Encoded header contains invalud channels number. Must be 3 or 4")
            }
            DecodeError::InvalidColorSpaceValue => {
                f.write_str("Encoded header contains invalud color space value. Must be 0 or 1")
            }
            DecodeError::OutputIsTooSmall => {
                f.write_str("Output buffer is too small to fit decoded image")
            }
        }
    }
}

impl core::error::Error for DecodeError {}

impl Qoi {
    /// Returns bytes size for the decoded image.
    #[inline]
    pub(crate) fn decoded_size(&self) -> usize {
        self.width as usize * self.height as usize * self.colors.channels()
    }

    /// Reads header from encoded QOI image.\
    /// Returned header can be analyzed before proceeding parsing with [`Qoi::decode_skip_header`].
    pub(crate) fn decode_header(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < QOI_HEADER_SIZE {
            return Err(DecodeError::NotEnoughData);
        }

        let magic = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        if magic != QOI_MAGIC {
            return Err(DecodeError::InvalidMagic);
        }

        let w = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let h = u32::from_be_bytes(bytes[8..12].try_into().unwrap());

        let channels = bytes[12];
        let colors = bytes[13];

        Ok(Qoi {
            width: w,
            height: h,
            colors: match (channels, colors) {
                (3, 0) => Colors::Srgb,
                (4, 0) => Colors::SrgbLinA,
                (3, 1) => Colors::Rgb,
                (4, 1) => Colors::Rgba,
                (_, 0 | 1) => return Err(DecodeError::InvalidChannelsValue),
                (_, _) => return Err(DecodeError::InvalidColorSpaceValue),
            },
        })
    }

    /// Decode a QOI image from bytes slice.\
    /// Decoded raw RGB or RGBA pixels are written into `output` slice.
    ///
    /// On success this function returns `Ok(qoi)` with `qoi` describing image dimensions and color space.\
    /// On failure this function returns `Err(err)` with `err` describing cause of the error.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn decode(bytes: &[u8], output: &mut [u8]) -> Result<Self, DecodeError> {
        let qoi = Self::decode_header(bytes)?;
        qoi.decode_skip_header(&bytes[QOI_HEADER_SIZE..], output)?;
        Ok(qoi)
    }

    /// Decode a QOI image from bytes slice.\
    /// `bytes` does not include QOI header. Uses provided `Qoi` value instead.\
    /// Decoded raw RGB or RGBA (depending on `self.colors` value) pixels are written into `output` slice.
    ///
    /// On success this function returns `Ok(())`.\
    /// On failure this function returns `Err(err)` with `err` describing cause of the error.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn decode_skip_header(
        &self,
        bytes: &[u8],
        output: &mut [u8],
    ) -> Result<(), DecodeError> {
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }

        let px_len = self.decoded_size();

        let output = match output.get_mut(..px_len) {
            None => return Err(DecodeError::OutputIsTooSmall),
            Some(output) => output,
        };

        match self.colors.has_alpha() {
            true => {
                Self::decode_range::<4>(
                    &mut [Pixel::new(); 64],
                    &mut Pixel::new_opaque(),
                    &mut 0,
                    bytes,
                    output,
                )?;
            }
            false => {
                Self::decode_range::<3>(
                    &mut [Pixel::new(); 64],
                    &mut Pixel::new_opaque(),
                    &mut 0,
                    bytes,
                    output,
                )?;
            }
        }
        Ok(())
    }

    /// Decode range of pixels into pixels slice.
    #[inline]
    pub(crate) fn decode_range<const N: usize>(
        index: &mut [[u8; N]; 64],
        ppx: &mut [u8; N],
        prun: &mut usize,
        bytes: &[u8],
        pixels: &mut [u8],
    ) -> Result<usize, DecodeError>
    where
        [u8; N]: Pixel,
    {
        assert_eq!(pixels.len() % N, 0);

        // Safe equivalent of upstream's `bytemuck::cast_slice_mut`: the assert
        // above guarantees the chunked remainder is empty.
        let (mut pixels, rem) = pixels.as_chunks_mut::<N>();
        debug_assert!(rem.is_empty());

        let mut px = *ppx;

        if *prun > 0 {
            let (head, tail) = pixels.split_at_mut((*prun).min(pixels.len()));

            pixels = tail;
            head.fill(px);

            if pixels.is_empty() {
                cold();
                *prun -= head.len();
                return Ok(0);
            } else {
                *prun = 0;
            }
        }

        let mut rest = bytes;

        loop {
            match pixels {
                [out, tail @ ..] => {
                    pixels = tail;
                    match rest {
                        [b1 @ 0b00000000..=0b00111111, tail @ ..] => {
                            px = index[*b1 as usize];
                            *out = px;

                            rest = tail;
                            continue;
                        }
                        [b1 @ 0b01000000..=0b01111111, tail @ ..] => {
                            let vr = ((b1 >> 4) & 0x03).wrapping_sub(2);
                            let vg = ((b1 >> 2) & 0x03).wrapping_sub(2);
                            let vb = (b1 & 0x03).wrapping_sub(2);
                            px.add_rgb(vr, vg, vb);

                            rest = tail;
                        }
                        [b1 @ 0b10000000..=0b10111111, b2, tail @ ..] => {
                            let vg = (b1 & 0x3f).wrapping_sub(32);
                            let vr = ((b2 >> 4) & 0x0f).wrapping_sub(8).wrapping_add(vg);
                            let vb = (b2 & 0x0f).wrapping_sub(8).wrapping_add(vg);
                            px.add_rgb(vr, vg, vb);

                            rest = tail;
                        }
                        [0b11111110, b2, b3, b4, tail @ ..] => {
                            px.set_rgb(*b2, *b3, *b4);

                            rest = tail;
                        }
                        [0b11111111, b2, b3, b4, _b5, tail @ ..] if N == 3 => {
                            cold();
                            px.set_rgb(*b2, *b3, *b4);

                            rest = tail;
                        }
                        [0b11111111, b2, b3, b4, b5, tail @ ..] => {
                            px.set_rgba(*b2, *b3, *b4, *b5);

                            rest = tail;
                        }
                        [b1 @ 0b11000000..=0b11111101, dtail @ ..] => {
                            *out = px;
                            let run = *b1 as usize & 0x3f;
                            // Clamp fix (local modification): clamp the run to the
                            // remaining output before splitting, so a run reaching
                            // the buffer edge does not panic. The unfilled
                            // remainder is carried in `*prun` below.
                            let (head, tail) = pixels.split_at_mut(run.min(pixels.len()));
                            head.fill(px);
                            pixels = tail;
                            rest = dtail;

                            if unlikely(pixels.is_empty()) {
                                *prun = run - head.len();
                                break;
                            }

                            continue;
                        }
                        _ => {
                            return Err(DecodeError::NotEnoughData);
                        }
                    }

                    index[px.hash() as usize] = px;

                    *out = px;
                }
                [] => {
                    cold();
                    break;
                }
            }
        }

        *ppx = px;

        Ok(bytes.len() - rest.len())
    }

    /// Decode a QOI image from bytes slice.\
    /// Decoded raw RGB or RGBA pixels are written into allocated `Vec`.
    ///
    /// On success this function returns `Ok((qoi, vec))` with `qoi` describing image dimensions and color space and `vec` containing raw pixels data.\
    /// On failure this function returns `Err(err)` with `err` describing cause of the error.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn decode_alloc(bytes: &[u8]) -> Result<(Self, Vec<u8>), DecodeError> {
        let qoi = Self::decode_header(bytes)?;

        let size = qoi.decoded_size();
        let mut output = vec![0; size];
        let qoi = Self::decode(bytes, &mut output)?;
        Ok((qoi, output))
    }
}
