use alloc::string::String;
use enough::StopReason;
use whereat::At;

/// Result alias with `At<BitmapError>` for automatic file:line location tracking.
///
/// Every public decode/encode entry point returns this. The error carries a
/// captured call-site trace (file, line, and — with [`whereat::define_at_crate_info!`]
/// in scope — a GitHub source link), which is invaluable for diagnosing
/// malformed-input failures in server logs. Match on the underlying enum via
/// [`At::error`]:
///
/// ```
/// use zenbitmaps::{decode, BitmapError};
/// use enough::Unstoppable;
///
/// match decode(b"not an image", Unstoppable) {
///     Ok(_) => {}
///     Err(e) => {
///         // `e` is `At<BitmapError>`; the inner enum is `e.error()`.
///         assert!(matches!(e.error(), BitmapError::UnrecognizedFormat));
///     }
/// }
/// ```
pub type Result<T> = core::result::Result<T, At<BitmapError>>;

/// Errors from PNM/BMP decoding and encoding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BitmapError {
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

    /// Unsupported codec operation.
    #[cfg(feature = "zencodec")]
    #[error(transparent)]
    UnsupportedOperation(#[from] zencodec::UnsupportedOperation),
}

impl From<StopReason> for BitmapError {
    fn from(r: StopReason) -> Self {
        BitmapError::Cancelled(r)
    }
}
