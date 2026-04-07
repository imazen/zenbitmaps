/// Default memory cap when no explicit `max_memory_bytes` is set (1 GiB).
///
/// Prevents OOM from crafted headers declaring enormous dimensions.
/// Override by setting `Limits { max_memory_bytes: Some(your_cap), .. }`.
pub const DEFAULT_MAX_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;

/// Resource limits for decode/encode operations.
///
/// When no limits are provided, decoders apply [`DEFAULT_MAX_MEMORY_BYTES`]
/// (1 GiB). Set `max_memory_bytes` explicitly to raise or lower this.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limits {
    pub max_width: Option<u64>,
    pub max_height: Option<u64>,
    /// Maximum pixel count (width * height).
    pub max_pixels: Option<u64>,
    /// Maximum memory bytes for output buffer allocation.
    /// Defaults to [`DEFAULT_MAX_MEMORY_BYTES`] (1 GiB) when `None`.
    pub max_memory_bytes: Option<u64>,
}

impl Limits {
    /// Check dimensions against limits. Returns Ok(()) or LimitExceeded error.
    pub(crate) fn check(&self, width: u32, height: u32) -> Result<(), crate::BitmapError> {
        if let Some(max_w) = self.max_width
            && u64::from(width) > max_w
        {
            return Err(crate::BitmapError::LimitExceeded(alloc::format!(
                "width {width} exceeds limit {max_w}"
            )));
        }
        if let Some(max_h) = self.max_height
            && u64::from(height) > max_h
        {
            return Err(crate::BitmapError::LimitExceeded(alloc::format!(
                "height {height} exceeds limit {max_h}"
            )));
        }
        if let Some(max_px) = self.max_pixels {
            let pixels = u64::from(width) * u64::from(height);
            if pixels > max_px {
                return Err(crate::BitmapError::LimitExceeded(alloc::format!(
                    "pixel count {pixels} exceeds limit {max_px}"
                )));
            }
        }
        Ok(())
    }
}

/// Check output buffer size against limits (user-provided or default 1 GiB cap).
///
/// Every decoder must call this before allocating the output buffer.
/// When `limits` is `None`, applies [`DEFAULT_MAX_MEMORY_BYTES`].
pub(crate) fn check_output_size(
    bytes: usize,
    limits: Option<&Limits>,
) -> Result<(), crate::BitmapError> {
    let max = limits
        .and_then(|l| l.max_memory_bytes)
        .unwrap_or(DEFAULT_MAX_MEMORY_BYTES);
    if bytes as u64 > max {
        return Err(crate::BitmapError::LimitExceeded(alloc::format!(
            "output size {bytes} bytes exceeds memory limit {max}"
        )));
    }
    Ok(())
}
