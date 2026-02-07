//! Sealed traits mapping `rgb` crate pixel types to [`PixelLayout`].

use crate::PixelLayout;

mod private {
    pub trait Sealed {}
}

/// Pixel type that can be decoded from PNM/BMP data.
pub trait DecodePixel: Copy + 'static + private::Sealed {
    /// The [`PixelLayout`] this pixel type corresponds to.
    fn layout() -> PixelLayout;
}

/// Pixel type that can be encoded to PNM/BMP data.
pub trait EncodePixel: Copy + 'static + private::Sealed {
    /// The [`PixelLayout`] this pixel type corresponds to.
    fn layout() -> PixelLayout;
}

macro_rules! impl_pixel {
    ($ty:ty, $layout:expr) => {
        impl private::Sealed for $ty {}
        impl DecodePixel for $ty {
            fn layout() -> PixelLayout { $layout }
        }
        impl EncodePixel for $ty {
            fn layout() -> PixelLayout { $layout }
        }
    };
}

impl_pixel!(rgb::RGB<u8>, PixelLayout::Rgb8);
impl_pixel!(rgb::RGBA<u8>, PixelLayout::Rgba8);
impl_pixel!(rgb::alt::BGR<u8>, PixelLayout::Bgr8);
impl_pixel!(rgb::alt::BGRA<u8>, PixelLayout::Bgra8);
