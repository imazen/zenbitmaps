use alloc::vec::Vec;
use enough::Stop;

use crate::error::PnmError;
use crate::pixel::PixelLayout;

/// Unified encode request for all supported bitmap formats.
pub enum EncodeRequest {
    #[cfg(feature = "pnm")]
    Pnm(crate::pnm::PnmFormat),
    #[cfg(feature = "bmp")]
    Bmp {
        /// Include alpha channel (32-bit BMP). Otherwise 24-bit.
        alpha: bool,
    },
}

impl EncodeRequest {
    /// Encode to a PNM format (P5/P6/P7/PFM).
    #[cfg(feature = "pnm")]
    pub fn pnm(format: crate::pnm::PnmFormat) -> Self {
        Self::Pnm(format)
    }

    /// Encode to 24-bit BMP (RGB, no alpha).
    #[cfg(feature = "bmp")]
    pub fn bmp() -> Self {
        Self::Bmp { alpha: false }
    }

    /// Encode to 32-bit BMP (RGBA with alpha).
    #[cfg(feature = "bmp")]
    pub fn bmp_with_alpha() -> Self {
        Self::Bmp { alpha: true }
    }

    /// Encode pixels to bytes.
    pub fn encode(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        layout: PixelLayout,
        stop: impl Stop,
    ) -> Result<Vec<u8>, PnmError> {
        match self {
            #[cfg(feature = "pnm")]
            Self::Pnm(format) => crate::pnm::encode(pixels, width, height, layout, *format, &stop),
            #[cfg(feature = "bmp")]
            Self::Bmp { alpha } => crate::bmp::encode(pixels, width, height, layout, *alpha, &stop),
        }
    }
}
