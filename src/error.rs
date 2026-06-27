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

    /// The caller-supplied pixel buffer format cannot be encoded to the
    /// requested output format. Carries a human description of the offending
    /// layout and the formats the encoder accepts.
    ///
    /// Distinct from [`UnsupportedVariant`](Self::UnsupportedVariant) (a feature
    /// within a *decoded* image's format): this is a negotiation failure on the
    /// caller's pixel descriptor at encode time, and maps to the
    /// `UnsupportedPixelFormat` error category under the `zencodec` feature.
    #[error("unsupported pixel format: {0}")]
    UnsupportedPixelFormat(String),

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

// Codec-agnostic error taxonomy (zencodec PR #103). Maps every `BitmapError`
// variant to exactly one coarse `ErrorCategory` so consumers can route on the
// category (HTTP status, retry policy, logging) without naming this enum.
#[cfg(feature = "zencodec")]
impl zencodec::CategorizedError for BitmapError {
    fn codec_name(&self) -> Option<&'static str> {
        Some("zenbitmaps")
    }

    fn category(&self) -> zencodec::ErrorCategory {
        use zencodec::ErrorCategory as C;
        use zencodec::LimitKind as L;
        match self {
            // === Format / image type not handled ===
            // Magic bytes matched no format this codec recognizes.
            BitmapError::UnrecognizedFormat => C::UnsupportedImageType,

            // === Malformed / corrupt bitstream content ===
            BitmapError::InvalidHeader(_) | BitmapError::InvalidData(_) => C::MalformedImage,

            // === A handled format uses a sub-feature/variant we don't implement ===
            // (e.g. an unsupported BMP bit depth/compression, TGA image type,
            // PAM depth, HDR orientation, or a format whose cargo feature is
            // disabled). This is the residual stringly catch-all after the
            // encode-side pixel-format cases were split into
            // `UnsupportedPixelFormat`; the dominant remaining case is "a feature
            // within a recognized format", so `UnsupportedImageFeature` is the
            // best single fit.
            BitmapError::UnsupportedVariant(_) => C::UnsupportedImageFeature,

            // === Caller's pixel buffer can't be encoded to the target format ===
            BitmapError::UnsupportedPixelFormat(_) => C::UnsupportedPixelFormat,

            // === Truncated input ===
            BitmapError::UnexpectedEof => C::UnexpectedEof,

            // === Resource limits ===
            // The category is always `LimitsExceeded`; only the sub-kind varies.
            // `DimensionsTooLarge` guards usize overflow of `width*height*bpp`, so
            // the width*height product (`Pixels`) is the representative kind.
            BitmapError::DimensionsTooLarge { .. } => C::LimitsExceeded(L::Pixels),
            // `LimitExceeded` is a stringly catch-all spanning width/height/
            // pixel-count/output-memory caps; the always-on default pixel cap is
            // its dominant constructor, so `Pixels` is the representative kind
            // (the precise sub-kind lives in the message).
            BitmapError::LimitExceeded(_) => C::LimitsExceeded(L::Pixels),

            // === Caller-supplied pixel/output buffer geometry is wrong ===
            BitmapError::LayoutMismatch { .. } | BitmapError::BufferTooSmall { .. } => {
                C::InvalidBuffer
            }

            // === Delegate to the zencodec cause types ===
            // `StopReason` carries cancel-vs-timeout; `UnsupportedOperation`
            // carries the API-operation axis (incl. the pixel-format arm).
            BitmapError::Cancelled(r) => r.category(),
            BitmapError::UnsupportedOperation(op) => op.category(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "zencodec")]
    use super::*;

    #[cfg(feature = "zencodec")]
    #[test]
    fn error_category_mapping() {
        use zencodec::{CategorizedError, ErrorCategory as C, LimitKind as L};

        assert_eq!(
            BitmapError::UnrecognizedFormat.codec_name(),
            Some("zenbitmaps")
        );

        // Image type / format not handled at all.
        assert_eq!(
            BitmapError::UnrecognizedFormat.category(),
            C::UnsupportedImageType
        );

        // Malformed bitstream content.
        assert_eq!(
            BitmapError::InvalidHeader("x".into()).category(),
            C::MalformedImage
        );
        assert_eq!(
            BitmapError::InvalidData("x".into()).category(),
            C::MalformedImage
        );

        // A feature within a recognized format isn't supported.
        assert_eq!(
            BitmapError::UnsupportedVariant("x".into()).category(),
            C::UnsupportedImageFeature
        );

        // The caller's pixel buffer can't be encoded to the target format.
        assert_eq!(
            BitmapError::UnsupportedPixelFormat("x".into()).category(),
            C::UnsupportedPixelFormat
        );

        // Truncated input.
        assert_eq!(BitmapError::UnexpectedEof.category(), C::UnexpectedEof);

        // Resource limits map to the representative `LimitKind`.
        assert_eq!(
            BitmapError::DimensionsTooLarge {
                width: 9,
                height: 9
            }
            .category(),
            C::LimitsExceeded(L::Pixels)
        );
        assert_eq!(
            BitmapError::LimitExceeded("pixel count 9 exceeds limit 4".into()).category(),
            C::LimitsExceeded(L::Pixels)
        );

        // Caller-supplied pixel/output buffer geometry.
        assert_eq!(
            BitmapError::BufferTooSmall {
                needed: 9,
                actual: 1
            }
            .category(),
            C::InvalidBuffer
        );
        assert_eq!(
            BitmapError::LayoutMismatch {
                expected: crate::PixelLayout::Rgb8,
                actual: crate::PixelLayout::Rgba8,
            }
            .category(),
            C::InvalidBuffer
        );

        // Delegated zencodec cause types.
        assert_eq!(
            BitmapError::Cancelled(StopReason::Cancelled).category(),
            C::Cancelled
        );
        assert_eq!(
            BitmapError::Cancelled(StopReason::TimedOut).category(),
            C::TimedOut
        );
        assert_eq!(
            BitmapError::UnsupportedOperation(zencodec::UnsupportedOperation::PixelFormat)
                .category(),
            C::UnsupportedPixelFormat
        );
        assert_eq!(
            BitmapError::UnsupportedOperation(zencodec::UnsupportedOperation::AnimationEncode)
                .category(),
            C::UnsupportedOperation
        );

        // The `At<E>` blanket impl forwards both the category and the codec name.
        let traced = whereat::at!(BitmapError::UnrecognizedFormat);
        assert_eq!(traced.category(), C::UnsupportedImageType);
        assert_eq!(traced.codec_name(), Some("zenbitmaps"));
    }
}
