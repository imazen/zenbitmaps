/// Resource limits for decode/encode operations.
///
/// All fields default to `None` (no limit).
#[derive(Clone, Debug, Default)]
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
    pub(crate) fn check(&self, width: u32, height: u32) -> Result<(), crate::PnmError> {
        if let Some(max_w) = self.max_width {
            if u64::from(width) > max_w {
                return Err(crate::PnmError::LimitExceeded(alloc::format!(
                    "width {width} exceeds limit {max_w}"
                )));
            }
        }
        if let Some(max_h) = self.max_height {
            if u64::from(height) > max_h {
                return Err(crate::PnmError::LimitExceeded(alloc::format!(
                    "height {height} exceeds limit {max_h}"
                )));
            }
        }
        if let Some(max_px) = self.max_pixels {
            let pixels = u64::from(width) * u64::from(height);
            if pixels > max_px {
                return Err(crate::PnmError::LimitExceeded(alloc::format!(
                    "pixel count {pixels} exceeds limit {max_px}"
                )));
            }
        }
        Ok(())
    }

    /// Check that an allocation size is within memory limits.
    pub(crate) fn check_memory(&self, bytes: usize) -> Result<(), crate::PnmError> {
        if let Some(max_mem) = self.max_memory_bytes {
            if bytes as u64 > max_mem {
                return Err(crate::PnmError::LimitExceeded(alloc::format!(
                    "allocation {bytes} bytes exceeds memory limit {max_mem}"
                )));
            }
        }
        Ok(())
    }
}
