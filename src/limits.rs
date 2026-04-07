/// Hard cap on output buffer allocation (1 GiB).
///
/// Applied unconditionally by all decoders, even when no user [`Limits`] are set.
/// Prevents OOM from crafted headers declaring enormous dimensions.
const HARD_MAX_OUTPUT_BYTES: usize = 1024 * 1024 * 1024;

/// Resource limits for decode/encode operations.
///
/// All fields default to `None` (no limit).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limits {
    pub max_width: Option<u64>,
    pub max_height: Option<u64>,
    /// Maximum pixel count (width * height).
    pub max_pixels: Option<u64>,
    /// Maximum memory bytes for output buffer allocation.
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

    /// Check that an allocation size is within memory limits.
    pub(crate) fn check_memory(&self, bytes: usize) -> Result<(), crate::BitmapError> {
        if let Some(max_mem) = self.max_memory_bytes
            && bytes as u64 > max_mem
        {
            return Err(crate::BitmapError::LimitExceeded(alloc::format!(
                "allocation {bytes} bytes exceeds memory limit {max_mem}"
            )));
        }
        Ok(())
    }
}

/// Check output buffer size against the 1 GiB hard cap and optional user limits.
///
/// Every decoder must call this before allocating the output buffer.
/// The hard cap prevents OOM from crafted input regardless of whether
/// user-provided [`Limits`] are set.
pub(crate) fn check_output_size(
    bytes: usize,
    limits: Option<&Limits>,
) -> Result<(), crate::BitmapError> {
    if bytes > HARD_MAX_OUTPUT_BYTES {
        return Err(crate::BitmapError::LimitExceeded(alloc::format!(
            "output size {bytes} bytes exceeds hard limit of {HARD_MAX_OUTPUT_BYTES} bytes"
        )));
    }
    if let Some(limits) = limits {
        limits.check_memory(bytes)?;
    }
    Ok(())
}
