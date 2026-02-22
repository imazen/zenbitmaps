/// Image format detected from magic bytes.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    /// PNM family: PGM (P5), PPM (P6), PAM (P7), PFM (Pf/PF).
    Pnm,
    /// BMP (Windows bitmap).
    Bmp,
    /// Farbfeld (RGBA 16-bit).
    Farbfeld,
}

/// Pixel memory layout.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelLayout {
    /// Single channel, 8-bit grayscale.
    Gray8,
    /// Single channel, 16-bit grayscale (native endian).
    Gray16,
    /// 3 channels, 8-bit RGB.
    Rgb8,
    /// 4 channels, 8-bit RGBA.
    Rgba8,
    /// 3 channels, 8-bit BGR.
    Bgr8,
    /// 4 channels, 8-bit BGRA.
    Bgra8,
    /// 4 channels, 8-bit BGRX (opaque; 4th byte is padding, not alpha).
    Bgrx8,
    /// Single channel, 32-bit float grayscale.
    GrayF32,
    /// 3 channels, 32-bit float RGB.
    RgbF32,
    /// 4 channels, 16-bit RGBA (native endian).
    Rgba16,
}

impl PixelLayout {
    /// Bytes per pixel for this layout.
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::Gray16 => 2,
            Self::Rgb8 | Self::Bgr8 => 3,
            Self::Rgba8 | Self::Bgra8 | Self::Bgrx8 => 4,
            Self::GrayF32 => 4,
            Self::RgbF32 => 12,
            Self::Rgba16 => 8,
        }
    }

    /// Number of channels.
    pub fn channels(&self) -> usize {
        match self {
            Self::Gray8 | Self::Gray16 | Self::GrayF32 => 1,
            Self::Rgb8 | Self::Bgr8 | Self::RgbF32 => 3,
            Self::Rgba8 | Self::Bgra8 | Self::Bgrx8 | Self::Rgba16 => 4,
        }
    }

    /// Whether this layout has the same memory representation as `other`.
    ///
    /// For example, `Bgra8` and `Bgrx8` are compatible (same 4-byte B,G,R,X/A layout).
    pub fn is_memory_compatible(&self, other: PixelLayout) -> bool {
        if *self == other {
            return true;
        }
        matches!(
            (*self, other),
            (Self::Bgra8, Self::Bgrx8) | (Self::Bgrx8, Self::Bgra8)
        )
    }
}
