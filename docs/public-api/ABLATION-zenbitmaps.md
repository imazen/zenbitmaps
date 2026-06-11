# ABLATION-zenbitmaps — Conservative Public-API Review

**Date:** 2026-06-10
**Snapshot commit:** 024c23db9a46 (main@origin)
**Snapshot file:** docs/public-api/zenbitmaps.txt (142 default items / 920 all-features items)
**Grep template:** `grep -rn "<SYMBOL>" /home/lilith/work/ --include="*.rs" 2>/dev/null | grep -v "/zen/zenbitmaps/" | grep -v "target/" | grep -v ".jj/"`

## Summary

**0 items flagged for action.** The surface is intentional: a no_std/alloc core (always-available PNM + farbfeld free functions, `DecodeOutput`, `Limits`, `BitmapError`, `ImageFormat`, `PixelLayout`) plus opt-in feature layers (`bmp`, `hdr`, `qoi`, `tga`, `simd`, `rgb`, `imgref`, `zencodec`). Feature spread by design; no internals are leaked in a reachable form.

Known consumers as of this scan: zencodecs (zenpipe), imageflow_core, codec-eval, zenpixels-convert.

## Items Investigated

### Streaming decoder type aliases via private module paths

`HdrDecodeJob::StreamDec`, `QoiDecodeJob::StreamDec`, and `TgaDecodeJob::StreamDec` appear in the snapshot as paths like `zenbitmaps::codec::hdr_codec::HdrStreamingDecoder`. These structs are `pub` inside the private `codec` sub-module, re-exported via `pub use codec::*` chains, but NOT named explicitly in the top-level `pub use codec::{...}` lists in lib.rs.

In practice: zencodecs (the primary consumer) wraps the result of `.streaming_decoder()` in `OwnedStreamingDecoderShim<S>` and returns `Box<dyn DynStreamingDecoder>`. No external consumer names these types directly (confirmed by grep: 0 hits for `HdrStreamingDecoder`, `QoiStreamingDecoder`, `TgaStreamingDecoder` outside zenbitmaps). The path leak is cosmetic — consumers can't depend on the internal path compiling.

**Verdict: no action now.** If explicit re-exports are desired for documentation clarity, add `pub use codec::{HdrStreamingDecoder, QoiStreamingDecoder, TgaStreamingDecoder};` at crate root (additive, non-breaking).

### Items with zero external hits (KEEP — conservative default)

All confirmed at 0 external hits via grep scan. Kept because each is part of an intentional feature-gated API surface:

| Item | Feature gate | Rationale for KEEP |
|------|-------------|-------------------|
| `BmpMetadata` / `probe_bmp` | `bmp` | BMP-specific metadata introspection; plausible BMP-aware consumer use |
| `BmpPermissiveness` | `bmp` | Configuration enum; part of BMP decode API |
| `DecodePixel` / `EncodePixel` traits | `rgb` | Typed-pixel API surface; bounds for `as_pixels`/`to_imgvec`/`decode_pixels` |
| `decode_bmp_native`, `decode_bmp_permissive` | `bmp` | Convenience decode entry points with specific permissiveness |
| `decode_pixels`, `decode_pixels_with_limits` | `rgb` | Generic typed-pixel convenience functions |
| `decode_bmp_pixels`, `decode_bmp_pixels_with_limits` | `bmp` + `rgb` | BMP-specific typed decode |

### `BitmapSourceEncoding` (pub struct in codec/mod.rs)

Not accessible externally: `mod codec` is private in lib.rs and lib.rs never re-exports `BitmapSourceEncoding` by name. It does not appear in the snapshot. No action.

## Flagged Items

None.

## Digest

- Snapshot: 142 (default) / 920 (all-features) items
- Items investigated: ~12
- Flagged A: 0
- Flagged B: 0
- 0% of surface flagged
- Top finding: streaming decoder canonical paths appear in snapshot as internal module paths; purely cosmetic, no external consumer names these types
