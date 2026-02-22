use alloc::borrow::Cow;
use alloc::vec::Vec;

#[cfg(feature = "rgb")]
use rgb::AsPixels as _;

use crate::pixel::PixelLayout;

/// Decoded image output. Pixels may be borrowed (zero-copy) or owned.
#[derive(Clone, Debug)]
pub struct DecodeOutput<'a> {
    pixels: Cow<'a, [u8]>,
    pub width: u32,
    pub height: u32,
    pub layout: PixelLayout,
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
        }
    }

    /// Whether the pixel data is borrowed (zero-copy from input).
    pub fn is_borrowed(&self) -> bool {
        matches!(self.pixels, Cow::Borrowed(_))
    }

    pub(crate) fn borrowed(data: &'a [u8], width: u32, height: u32, layout: PixelLayout) -> Self {
        Self {
            pixels: Cow::Borrowed(data),
            width,
            height,
            layout,
        }
    }

    pub(crate) fn owned(data: Vec<u8>, width: u32, height: u32, layout: PixelLayout) -> Self {
        Self {
            pixels: Cow::Owned(data),
            width,
            height,
            layout,
        }
    }

    /// Reinterpret pixel data as typed pixel slice.
    ///
    /// Returns [`crate::BitmapError::LayoutMismatch`] if the pixel layout doesn't match `P`.
    #[cfg(feature = "rgb")]
    pub fn as_pixels<P: crate::DecodePixel>(&self) -> Result<&[P], crate::BitmapError>
    where
        [u8]: rgb::AsPixels<P>,
    {
        if !self.layout.is_memory_compatible(P::layout()) {
            return Err(crate::BitmapError::LayoutMismatch {
                expected: P::layout(),
                actual: self.layout,
            });
        }
        Ok(self.pixels().as_pixels())
    }

    /// Zero-copy view as an [`imgref::ImgRef`] of typed pixels.
    ///
    /// No allocation or copy — the returned `ImgRef` borrows directly from
    /// this `DecodeOutput`'s pixel buffer. Works for both borrowed (PNM) and
    /// owned (BMP, farbfeld) data.
    ///
    /// Returns [`crate::BitmapError::LayoutMismatch`] if the pixel layout doesn't match `P`.
    #[cfg(feature = "imgref")]
    pub fn as_imgref<P: crate::DecodePixel>(
        &self,
    ) -> Result<imgref::ImgRef<'_, P>, crate::BitmapError>
    where
        [u8]: rgb::AsPixels<P>,
    {
        let pixels: &[P] = self.as_pixels()?;
        Ok(imgref::ImgRef::new(
            pixels,
            self.width as usize,
            self.height as usize,
        ))
    }

    /// Convert to an [`imgref::ImgVec`] of typed pixels.
    ///
    /// Returns [`crate::BitmapError::LayoutMismatch`] if the pixel layout doesn't match `P`.
    #[cfg(feature = "imgref")]
    pub fn to_imgvec<P: crate::DecodePixel>(&self) -> Result<imgref::ImgVec<P>, crate::BitmapError>
    where
        [u8]: rgb::AsPixels<P>,
    {
        let pixels: &[P] = self.as_pixels()?;
        Ok(imgref::ImgVec::new(
            pixels.to_vec(),
            self.width as usize,
            self.height as usize,
        ))
    }
}
