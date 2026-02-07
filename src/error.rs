use alloc::string::String;
use enough::StopReason;

/// Errors from PNM/BMP decoding and encoding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PnmError {
    #[error("unrecognized format magic bytes")]
    UnrecognizedFormat,

    #[error("invalid header: {0}")]
    InvalidHeader(String),

    #[error("unsupported format variant: {0}")]
    UnsupportedVariant(String),

    #[error("invalid pixel data: {0}")]
    InvalidData(String),

    #[error("dimensions too large: {width}x{height}")]
    DimensionsTooLarge { width: u32, height: u32 },

    #[error("limit exceeded: {0}")]
    LimitExceeded(String),

    #[error("unexpected end of input")]
    UnexpectedEof,

    #[error("pixel layout mismatch: expected {expected:?}, got {actual:?}")]
    LayoutMismatch {
        expected: crate::PixelLayout,
        actual: crate::PixelLayout,
    },

    #[error("buffer too small: need {needed} bytes, got {actual}")]
    BufferTooSmall { needed: usize, actual: usize },

    #[error("operation cancelled")]
    Cancelled(StopReason),
}

impl From<StopReason> for PnmError {
    fn from(r: StopReason) -> Self {
        PnmError::Cancelled(r)
    }
}
