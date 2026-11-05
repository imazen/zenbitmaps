//! Allocation helpers honoring an [`AllocPref`] policy per call site.
//!
//! A bitmap decode mixes two allocation regimes:
//!
//! * **Big, untrusted-sized buffers** (the full-image output pixel buffer,
//!   sized from the header dimensions) default to the *fallible* `try_reserve`
//!   path — a malicious header can demand far more than fits, so we want a
//!   graceful [`BitmapError::OutOfMemory`] rather than an abort. (The
//!   `Limits` byte/pixel caps already reject the worst cases up front; this is
//!   defence in depth for whatever slips under the cap.)
//! * **Small, bounded scratch** (one scanline, the ≤256-entry palette) defaults
//!   to the *infallible* `vec!` path — a single `calloc` is faster and the size
//!   is bounded by the image width / a fixed table, not unboundedly
//!   attacker-controlled.
//!
//! [`AllocPref`] is a **3-mode, per-site override** of that default:
//! [`Fallible`](AllocPref::Fallible) / [`Infallible`](AllocPref::Infallible)
//! force one path everywhere; [`CodecDefault`](AllocPref::CodecDefault) (and any
//! future variant) keeps each site's own default. The helper signatures
//! therefore take the caller's preference *and* the site default, and resolve
//! them together.
//!
//! `AllocPref` is a crate-local mirror of
//! [`zencodec::AllocPreference`](https://docs.rs/zencodec): the internal decode
//! pipeline is always compiled (the bare `decode()` API needs no `zencodec`
//! feature), so it cannot name the optional-dependency type directly. The
//! zencodec codec boundary converts `AllocPreference` → `AllocPref` via the
//! gated [`From`] impl below.

use alloc::vec::Vec;
use whereat::{At, at};

use crate::error::BitmapError;

/// Caller preference for allocation fallibility, applied per call site.
///
/// Crate-local mirror of `zencodec::AllocPreference` (see the module docs for
/// why it is duplicated). [`CodecDefault`](Self::CodecDefault) is the default:
/// each allocation site keeps its own default fallibility.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AllocPref {
    /// Let each site keep its own default (big untrusted buffers fallible,
    /// small bounded scratch infallible). Default — preserves existing
    /// behaviour.
    #[default]
    CodecDefault,
    /// Force the fallible `try_reserve` path everywhere (graceful OOM error).
    #[cfg_attr(not(feature = "zencodec"), allow(dead_code))]
    Fallible,
    /// Force the infallible `vec!` / `Vec::with_capacity` path everywhere
    /// (faster single `calloc`; aborts on OOM).
    #[cfg_attr(not(feature = "zencodec"), allow(dead_code))]
    Infallible,
}

#[cfg(feature = "zencodec")]
impl From<zencodec::AllocPreference> for AllocPref {
    fn from(p: zencodec::AllocPreference) -> Self {
        match p {
            zencodec::AllocPreference::Fallible => AllocPref::Fallible,
            zencodec::AllocPreference::Infallible => AllocPref::Infallible,
            // CodecDefault + any future `#[non_exhaustive]` variant.
            _ => AllocPref::CodecDefault,
        }
    }
}

/// Resolve the 3-mode [`AllocPref`] against THIS site's default fallibility.
///
/// * [`Fallible`](AllocPref::Fallible) → always `true`.
/// * [`Infallible`](AllocPref::Infallible) → always `false`.
/// * [`CodecDefault`](AllocPref::CodecDefault) (and any future variant) → the
///   site default, unchanged.
#[inline]
#[must_use]
pub(crate) fn resolve_fallible(pref: AllocPref, site_default_fallible: bool) -> bool {
    match pref {
        AllocPref::Fallible => true,
        AllocPref::Infallible => false,
        _ => site_default_fallible,
    }
}

/// Allocate `n` zeroed bytes, honoring the per-site fallibility.
///
/// `pref` is the caller's [`AllocPref`]; `site_default_fallible` is this site's
/// default when `pref` is [`CodecDefault`](AllocPref::CodecDefault).
///
/// * fallible → allocator-zeroed storage, returning
///   [`BitmapError::OutOfMemory`] on allocation failure.
/// * infallible → `vec![0u8; n]` (single `calloc`, aborts on OOM).
pub(crate) fn alloc_zeroed(
    pref: AllocPref,
    site_default_fallible: bool,
    n: usize,
) -> Result<Vec<u8>, At<BitmapError>> {
    bytemuck::allocation::try_zeroed_vec(n).map_err(|()| {
        if !resolve_fallible(pref, site_default_fallible) {
            let layout = core::alloc::Layout::array::<u8>(n).expect("capacity overflow");
            alloc::alloc::handle_alloc_error(layout);
        }
        at!(BitmapError::OutOfMemory(alloc::format!(
            "out of memory allocating {n} bytes"
        )))
    })
}

/// Allocate an empty `Vec<u8>` with reserved capacity for `cap` bytes, honoring
/// the per-site fallibility (for the `Vec::with_capacity` + extend sites).
///
/// `pref` is the caller's [`AllocPref`]; `site_default_fallible` is this site's
/// default when `pref` is [`CodecDefault`](AllocPref::CodecDefault).
///
/// * fallible → `try_reserve_exact`, returning [`BitmapError::OutOfMemory`]
///   on allocation failure.
/// * infallible → `Vec::with_capacity(cap)` (aborts on OOM).
///
/// The returned `Vec` is empty (length 0); the caller fills it.
pub(crate) fn vec_with_capacity(
    pref: AllocPref,
    site_default_fallible: bool,
    cap: usize,
) -> Result<Vec<u8>, At<BitmapError>> {
    if resolve_fallible(pref, site_default_fallible) {
        let mut v = Vec::new();
        v.try_reserve_exact(cap).map_err(|_| {
            at!(BitmapError::OutOfMemory(alloc::format!(
                "out of memory allocating {cap} bytes"
            )))
        })?;
        Ok(v)
    } else {
        Ok(Vec::with_capacity(cap))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `CodecDefault` keeps each site's own default fallibility.

    #[test]
    fn codec_default_keeps_site_default_true() {
        // Big-buffer site (default fallible): CodecDefault stays fallible.
        assert!(resolve_fallible(AllocPref::CodecDefault, true));
    }

    #[test]
    fn codec_default_keeps_site_default_false() {
        // Small-scratch site (default infallible): CodecDefault stays infallible.
        assert!(!resolve_fallible(AllocPref::CodecDefault, false));
    }

    #[test]
    fn explicit_fallible_overrides_any_site_default() {
        assert!(resolve_fallible(AllocPref::Fallible, false));
        assert!(resolve_fallible(AllocPref::Fallible, true));
    }

    #[test]
    fn explicit_infallible_overrides_any_site_default() {
        assert!(!resolve_fallible(AllocPref::Infallible, true));
        assert!(!resolve_fallible(AllocPref::Infallible, false));
    }

    #[test]
    fn zeroed_boundaries_preserve_policy_and_growth() {
        for pref in [
            AllocPref::CodecDefault,
            AllocPref::Fallible,
            AllocPref::Infallible,
        ] {
            for site in [false, true] {
                for n in [0, 1, 4095, 4096, 4097] {
                    let mut v = alloc_zeroed(pref, site, n).unwrap();
                    assert_eq!(v.len(), n);
                    assert!(v.iter().all(|&b| b == 0));
                    v.fill(7);
                    v.push(9);
                    assert!(v[..n].iter().all(|&b| b == 7));
                    assert_eq!(v[n], 9);
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "capacity overflow")]
    fn infallible_overflow_panics() {
        let _ = alloc_zeroed(AllocPref::Infallible, false, usize::MAX);
    }

    #[test]
    fn alloc_zeroed_all_modes_equal_bytes() {
        let a = alloc_zeroed(AllocPref::CodecDefault, true, 4096).unwrap();
        let b = alloc_zeroed(AllocPref::Infallible, true, 4096).unwrap();
        let c = alloc_zeroed(AllocPref::Fallible, false, 4096).unwrap();
        assert_eq!(a.len(), 4096);
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert!(a.iter().all(|&x| x == 0));
    }

    #[test]
    fn vec_with_capacity_reserves_and_is_empty() {
        let a = vec_with_capacity(AllocPref::Infallible, false, 1024).unwrap();
        let b = vec_with_capacity(AllocPref::Fallible, false, 1024).unwrap();
        assert_eq!(a.len(), 0);
        assert_eq!(b.len(), 0);
        assert!(a.capacity() >= 1024);
        assert!(b.capacity() >= 1024);
    }

    #[test]
    fn alloc_zeroed_fallible_oom_returns_err() {
        // Request an impossibly large allocation; the fallible path must
        // return Err (mapped to LimitExceeded) rather than abort.
        let r = alloc_zeroed(AllocPref::Fallible, true, usize::MAX);
        assert!(r.is_err());
        assert!(matches!(
            r.unwrap_err().error(),
            BitmapError::OutOfMemory(_)
        ));
    }

    #[test]
    fn vec_with_capacity_fallible_oom_returns_err() {
        let r = vec_with_capacity(AllocPref::Fallible, true, usize::MAX);
        assert!(r.is_err());
        assert!(matches!(
            r.unwrap_err().error(),
            BitmapError::OutOfMemory(_)
        ));
    }

    #[cfg(feature = "zencodec")]
    #[test]
    fn from_zencodec_alloc_preference_maps_each_mode() {
        use zencodec::AllocPreference;
        assert_eq!(
            AllocPref::from(AllocPreference::Fallible),
            AllocPref::Fallible
        );
        assert_eq!(
            AllocPref::from(AllocPreference::Infallible),
            AllocPref::Infallible
        );
        assert_eq!(
            AllocPref::from(AllocPreference::CodecDefault),
            AllocPref::CodecDefault
        );
    }
}
