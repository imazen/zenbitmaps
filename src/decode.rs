use alloc::borrow::Cow;
use alloc::vec::Vec;
use enough::Stop;

use crate::error::PnmError;
use crate::info::BitmapFormat;
use crate::limits::Limits;
use crate::pixel::PixelLayout;

/// Decoded image output. Pixels may be borrowed (zero-copy) or owned.
#[derive(Clone, Debug)]
pub struct DecodeOutput<'a> {
    pixels: Cow<'a, [u8]>,
    pub width: u32,
    pub height: u32,
    pub layout: PixelLayout,
    pub format: BitmapFormat,
}

impl<'a> DecodeOutput<'a> {
    /// Access the pixel data.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Take ownership of the pixel data (copies if borrowed).
    pub fn into_owned(self) -> DecodeOutput<'static> {
        DecodeOutput {
            pixels: Cow::Owned(self.pixels.into_owned()),
            width: self.width,
            height: self.height,
            layout: self.layout,
            format: self.format,
        }
    }

    /// Whether the pixel data is borrowed (zero-copy from input).
    pub fn is_borrowed(&self) -> bool {
        matches!(self.pixels, Cow::Borrowed(_))
    }

    pub(crate) fn borrowed(
        data: &'a [u8],
        width: u32,
        height: u32,
        layout: PixelLayout,
        format: BitmapFormat,
    ) -> Self {
        Self {
            pixels: Cow::Borrowed(data),
            width,
            height,
            layout,
            format,
        }
    }

    pub(crate) fn owned(
        data: Vec<u8>,
        width: u32,
        height: u32,
        layout: PixelLayout,
        format: BitmapFormat,
    ) -> Self {
        Self {
            pixels: Cow::Owned(data),
            width,
            height,
            layout,
            format,
        }
    }
}

/// Unified decode request for all supported bitmap formats.
///
/// Auto-detects format from magic bytes. Use `with_` methods to set
/// limits and desired output layout.
pub struct DecodeRequest<'a> {
    data: &'a [u8],
    limits: Option<&'a Limits>,
}

impl<'a> DecodeRequest<'a> {
    /// Create a decode request for the given data.
    /// Format is auto-detected from magic bytes.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, limits: None }
    }

    /// Set resource limits.
    pub fn with_limits(mut self, limits: &'a Limits) -> Self {
        self.limits = Some(limits);
        self
    }

    /// Decode the image. Returns zero-copy output when possible.
    pub fn decode(self, stop: impl Stop) -> Result<DecodeOutput<'a>, PnmError> {
        if self.data.len() < 3 {
            return Err(PnmError::UnexpectedEof);
        }

        match &self.data[..2] {
            #[cfg(feature = "pnm")]
            b"P5" | b"P6" | b"P7" | b"Pf" | b"PF" => {
                crate::pnm::decode(self.data, self.limits, &stop)
            }
            #[cfg(feature = "bmp")]
            b"BM" => crate::bmp::decode(self.data, self.limits, &stop),
            _ => Err(PnmError::UnrecognizedFormat),
        }
    }
}
