use crate::error::PnmError;
use crate::pixel::PixelLayout;

/// Image format detected from magic bytes.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapFormat {
    /// P5 — binary grayscale (PGM).
    Pgm,
    /// P6 — binary RGB (PPM).
    Ppm,
    /// P7 — PAM (arbitrary channels).
    Pam,
    /// PFM — floating-point.
    Pfm,
    /// BMP — Windows bitmap.
    Bmp,
}

/// Lightweight image metadata parsed from header bytes only.
///
/// Obtained via [`ImageInfo::from_bytes`] without decoding pixel data.
#[derive(Clone, Debug)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: BitmapFormat,
    pub native_layout: PixelLayout,
}

impl ImageInfo {
    /// Minimum bytes needed to parse any supported header.
    /// PNM headers are variable-length but typically small.
    /// BMP headers are exactly 54 bytes.
    pub const PROBE_BYTES: usize = 256;

    /// Parse image metadata from header bytes without decoding pixels.
    pub fn from_bytes(data: &[u8]) -> Result<Self, PnmError> {
        if data.len() < 3 {
            return Err(PnmError::UnexpectedEof);
        }

        match &data[..2] {
            #[cfg(feature = "pnm")]
            b"P5" | b"P6" | b"P7" | b"Pf" | b"PF" => crate::pnm::probe_header(data),
            #[cfg(feature = "bmp")]
            b"BM" => crate::bmp::probe_header(data),
            _ => Err(PnmError::UnrecognizedFormat),
        }
    }
}
