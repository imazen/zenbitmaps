/// Default memory cap when no explicit `max_memory_bytes` is set (1 GiB).
///
/// Prevents OOM from crafted headers declaring enormous dimensions.
/// Override by setting `Limits { max_memory_bytes: Some(your_cap), .. }`.
pub const DEFAULT_MAX_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;

/// Default pixel-count cap when no explicit `max_pixels` is set (120 MP).
///
/// Decoders enforce this against the header-declared `width * height` before
/// allocating, even when no [`Limits`] are supplied — matching the always-on
/// [`DEFAULT_MAX_MEMORY_BYTES`] byte cap and the wider fleet's 120 MP house
/// convention. The 1 GiB byte cap alone admits a far larger pixel count for
/// low-bpp formats (e.g. ~1 G grayscale px), so this brings the pixel ceiling
/// in line. Opt out by setting `Limits { max_pixels: Some(u64::MAX), .. }`.
pub const DEFAULT_MAX_PIXELS: u64 = 120_000_000;

/// Resource limits for decode/encode operations.
///
/// When no limits are provided, decoders still apply two always-on default
/// ceilings: [`DEFAULT_MAX_MEMORY_BYTES`] (1 GiB output bytes) and
/// [`DEFAULT_MAX_PIXELS`] (120 MP). Set `max_memory_bytes` / `max_pixels`
/// explicitly to raise or lower either (use `Some(u64::MAX)` to opt out).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limits {
    pub max_width: Option<u64>,
    pub max_height: Option<u64>,
    /// Maximum pixel count (width * height).
    /// Defaults to [`DEFAULT_MAX_PIXELS`] (120 MP) when `None`.
    pub max_pixels: Option<u64>,
    /// Maximum memory bytes for output buffer allocation.
    /// Defaults to [`DEFAULT_MAX_MEMORY_BYTES`] (1 GiB) when `None`.
    pub max_memory_bytes: Option<u64>,
}

impl Limits {
    /// Check dimensions against limits. Returns Ok(()) or LimitExceeded error.
    ///
    /// Only the `zencodec` codec adapters call this directly; gate it so the
    /// default-feature lib build (whose decoders go through `check_dimensions`)
    /// does not flag it as dead code.
    #[cfg(any(feature = "zencodec", test))]
    pub(crate) fn check(&self, width: u32, height: u32) -> crate::Result<()> {
        check_dimensions(width, height, Some(self))
    }
}

/// Check header-declared dimensions against limits (user-provided or defaults).
///
/// Every decoder must call this before allocating the output buffer, passing
/// its `Option<&Limits>` directly so the default ceilings fire even when no
/// limits are supplied. `max_width`/`max_height` are enforced only when set;
/// the pixel-count cap always applies, defaulting to [`DEFAULT_MAX_PIXELS`]
/// (120 MP) when `max_pixels` is `None`. Mirrors [`check_output_size`].
pub(crate) fn check_dimensions(
    width: u32,
    height: u32,
    limits: Option<&Limits>,
) -> crate::Result<()> {
    if let Some(max_w) = limits.and_then(|l| l.max_width)
        && u64::from(width) > max_w
    {
        return Err(whereat::at!(crate::BitmapError::LimitExceeded(
            alloc::format!("width {width} exceeds limit {max_w}")
        )));
    }
    if let Some(max_h) = limits.and_then(|l| l.max_height)
        && u64::from(height) > max_h
    {
        return Err(whereat::at!(crate::BitmapError::LimitExceeded(
            alloc::format!("height {height} exceeds limit {max_h}")
        )));
    }
    let max_px = limits
        .and_then(|l| l.max_pixels)
        .unwrap_or(DEFAULT_MAX_PIXELS);
    let pixels = u64::from(width) * u64::from(height);
    if pixels > max_px {
        return Err(whereat::at!(crate::BitmapError::LimitExceeded(
            alloc::format!("pixel count {pixels} exceeds limit {max_px}")
        )));
    }
    Ok(())
}

/// Check output buffer size against limits (user-provided or default 1 GiB cap).
///
/// Every decoder must call this before allocating the output buffer.
/// When `limits` is `None`, applies [`DEFAULT_MAX_MEMORY_BYTES`].
pub(crate) fn check_output_size(bytes: usize, limits: Option<&Limits>) -> crate::Result<()> {
    let max = limits
        .and_then(|l| l.max_memory_bytes)
        .unwrap_or(DEFAULT_MAX_MEMORY_BYTES);
    if bytes as u64 > max {
        return Err(whereat::at!(crate::BitmapError::LimitExceeded(
            alloc::format!("output size {bytes} bytes exceeds memory limit {max}")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A dimension whose pixel count exceeds the 120 MP default but whose
    // declared *bytes* would still pass the 1 GiB byte cap for a low-bpp
    // format — exactly the gap the default pixel cap closes. 13000 * 13000 =
    // 169,000,000 px > 120 MP; at 1 byte/px that is ~169 MB << 1 GiB.
    const OVER_W: u32 = 13_000;
    const OVER_H: u32 = 13_000;
    // 10000 * 10000 = 100 MP, under the 120 MP default.
    const UNDER_W: u32 = 10_000;
    const UNDER_H: u32 = 10_000;

    fn is_pixel_limit_err(r: crate::Result<()>) -> bool {
        match r.as_ref().map_err(|e| e.error()) {
            Err(crate::BitmapError::LimitExceeded(msg)) => msg.contains("pixel count"),
            _ => false,
        }
    }

    #[test]
    fn default_pixel_cap_rejects_over_120mp_with_no_limits() {
        // No explicit Limits (the untrusted-decode path) must still reject a
        // >120 MP header via the default pixel cap, NOT silently admit it.
        let r = check_dimensions(OVER_W, OVER_H, None);
        assert!(
            is_pixel_limit_err(r),
            "expected pixel-count LimitExceeded for {OVER_W}x{OVER_H} with default limits"
        );
        // The capability claim must match reality: the default really enforces.
        const { assert!(DEFAULT_MAX_PIXELS <= 120_000_000) };
    }

    #[test]
    fn default_pixel_cap_allows_under_120mp_with_no_limits() {
        assert!(
            check_dimensions(UNDER_W, UNDER_H, None).is_ok(),
            "{UNDER_W}x{UNDER_H} (100 MP) is under the 120 MP default and must pass"
        );
    }

    #[test]
    fn explicit_unlimited_pixels_opts_out_of_default_cap() {
        // The documented opt-out: max_pixels: Some(u64::MAX) restores the
        // pre-default "no pixel cap" behavior for callers that want it.
        let unlimited = Limits {
            max_pixels: Some(u64::MAX),
            ..Limits::default()
        };
        assert!(
            check_dimensions(OVER_W, OVER_H, Some(&unlimited)).is_ok(),
            "Some(u64::MAX) must opt out of the default 120 MP pixel cap"
        );
        // And via the method form used by the codec encode paths.
        assert!(unlimited.check(OVER_W, OVER_H).is_ok());
    }

    #[test]
    fn explicit_lower_pixel_cap_still_honored() {
        let tight = Limits {
            max_pixels: Some(4),
            ..Limits::default()
        };
        assert!(is_pixel_limit_err(check_dimensions(3, 3, Some(&tight))));
        assert!(check_dimensions(2, 2, Some(&tight)).is_ok());
    }

    #[test]
    fn width_height_limits_unchanged_under_default() {
        // Explicit width/height caps still fire; pixel default coexists.
        let l = Limits {
            max_width: Some(100),
            ..Limits::default()
        };
        match l.check(200, 1).as_ref().map_err(|e| e.error()) {
            Err(crate::BitmapError::LimitExceeded(msg)) => {
                assert!(msg.contains("width"), "got: {msg}")
            }
            other => panic!("expected width LimitExceeded, got {other:?}"),
        }
    }
}
