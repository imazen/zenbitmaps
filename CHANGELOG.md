# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Fixed

- QOI/BMP encode of chroma-free RGB no longer fails with
  `UnsupportedVariant`. The load-bearing narrowing (see "Added" below)
  collapses an all-gray RGB(A) view to Gray8, but QOI and BMP have no
  grayscale encode path (QOI: RGB/RGBA/BGRA; BMP: 24-bit RGB / 32-bit
  RGBA), so the reduced buffer hit the encoders' error arm — an all-gray
  RGB image (e.g. solid black) panicked on encode. `reduce_for_raw_encode`
  is now codec-aware: each encoder passes a predicate for the layouts it
  can emit, and the reduction is used only if encodable, else the original
  (broader) view is encoded. Narrowing only ever loses encodability via
  the →Gray path, so PNM and TGA (which encode grayscale) are unchanged.
  Regression: `codec::tests::{qoi,bmp}_all_gray_rgb_not_narrowed_to_unencodable_gray`.
- RLE BMP decompression-bomb DoS: a 158-byte RLE4 BMP declaring width 1 / height ≈ 2.7e8 decoded in ~18 s (found by the fuzz farm). The ~134 MB declared output passed the 1 GiB cap, so the decoder allocated it and ran per-output-pixel post-processing over 268M pixels. Fixes: (1) `decode_rle4` now stops at input exhaustion (`!self.bytes.eof()`), matching `decode_rle8plus` — without it a truncated stream spun `*line` down from the declared height; (2) `decode_rle` rejects an output larger than the compressed stream could plausibly encode (256×-input ratio guard, 64 KiB floor). The repro now rejects in ~3 ms. Regression: `tests/bmp_rle_dos_regression.rs`.
- ASCII PNM (P2/P3) with `maxval > 255` now decodes as true 16-bit. `decode_ascii_samples` emitted a single downscaled `u8` per sample even though `maxval > 255` selects a 16-bit layout (e.g. Gray16) — producing half the bytes the layout declares. That overran `PixelBuffer::as_slice` (OOB panic, reached via `zencodecs::push_decode`, fuzz zenpipe#51) and silently dropped 16-bit precision. 16-bit samples are now emitted as 2 raw native-endian bytes (matching the binary Gray16 path) and out-of-range samples are clamped to `maxval`. Regression: `tests/pnm_ascii_16bit_regression.rs`.

### Added

- Load-bearing descriptor narrowing on the zencodec encode path
  (zenpixels-convert 0.2.13 `load_bearing` #30): PNM/QOI/BMP/TGA
  encoders reduce the input view to its bit-exact load-bearing form
  before format mapping — dead alpha drops (RGBA→RGB: PAM→PPM, QOI
  4→3 channels, BMP 32→24, TGA 32→24), chroma-free RGB collapses to
  gray, bit-replicated U16 narrows to U8. Raw formats pay full price
  for dead lanes (no entropy coder downstream), so the reduction wins
  more here than for any compressed codec. Live alpha / real chroma /
  genuine high bits suppress the reduction (scan-proven, never lossy);
  wiring test pins PPM-vs-PAM both ways. Farbfeld (fixed RGBA16) and
  HDR (RGBE) are structurally exempt.

### Changed

- zencodec 0.1.19 → 0.1.22; zenpixels 0.2.10 → 0.2.13;
  zenpixels-convert 0.2.13 added (optional, rides the `zencodec`
  feature).

### Changed

- Removed `tests/` and `benches/` from the published package `include` list; downstream consumers no longer receive ~444 KB of test fixtures and bench sources (local build/test/bench unaffected).
- Vendored the QOI codec core (descriptor, pixel traits, encode/decode kernels)
  from `rapid-qoi` v0.6.x into `src/qoi/rapid_qoi/`, and dropped the external
  `rapid-qoi` Cargo dependency (the `qoi` feature is now self-contained). The
  vendored kernel carries the `QOI_OP_RUN` clamp fix at the source, so both the
  whole-image and the streaming/row-by-row decode paths share one unified,
  clamped implementation; the previous native workaround
  (`src/qoi/run_decode.rs`) was retired and replaced by a thin
  `QoiDecodeState` wrapper over the vendored `decode_range`. Upstream's
  `bytemuck` slice casts were replaced with safe `<[u8]>::as_chunks` so the
  crate stays `#![forbid(unsafe_code)]`-clean with no new direct dependency.
  Attribution preserved in each vendored file (© zakarumych, MIT OR
  Apache-2.0). Decoded pixels verified byte-identical to the reference `qoi`
  crate (0.4.1) across the corpus + synthetic run-heavy/run-at-edge images,
  both whole-image and row-by-row (e580b5e).

### Fixed

- BMP 8-bit grayscale (Gray8) decode is now byte-for-byte lossless on
  decode→encode→decode roundtrips. The 8bpp scanline reader shared the 24-bit
  RGB code path and wrongly applied the BGR↔RGB channel swap
  (`chunks_exact_mut(3).swap(0, 2)`) to single-channel Gray8 rows, scrambling
  pixels in 3-byte groups and dropping the trailing remainder on odd widths.
  The swap is now gated on `num_components == 3`, so Gray8 passes through
  untouched while 24-bit RGB behaviour is unchanged. This fixes the long-open
  `Fuzz (fuzz_roundtrip)` failure on 8bpp BMPs (the no-palette Gray8 crash and
  the paletted finding-#1 class). Encoders were already correct; the bug was
  purely in decode. Regression tests in `tests/roundtrip.rs` cover odd/even
  width Gray8 and odd-width paletted 8bpp (bc497d28).
- QOI decode no longer panics (`mid > len`) on spec-valid files where a
  `QOI_OP_RUN` chunk's run-length reaches the output-buffer edge / crosses a
  row boundary. The vendored `decode_range` clamps the run to the remaining
  output and carries the leftover across rows. Regression tests in
  `tests/roundtrip.rs` (a15fe87, #6, fixes #5).

### Changed

- `tests/fuzz_regression.rs` now uses the shared `zen-fuzz-regress`
  test-helper crate (DEDUP-J2). Behaviour is unchanged — same
  `fuzz/regression/` seeds, same two targets (`decode`, `roundtrip`),
  same panic-propagation failure semantics. The in-file `collect_seeds`
  scaffolding is now provided by `RegressionSuite`.

### Added

- Versioned public-API surface snapshot at `docs/public-api/zenbitmaps.txt`,
  regenerated by `tests/public_api_doc.rs` on every `cargo test` run
  (`ZEN_API_DOC=check` verifies in CI's clippy job, `=off` skips elsewhere);
  `just api-doc` / `just api-doc-check` recipes added.
- `tests/fuzz_regression.rs` regression-harness template ported from
  zenwebp (DEDUP-J). Walks `fuzz/regression/` (incl. per-target subdirs)
  and runs every seed through `decode`, `decode_bmp` (gated on `bmp`),
  `decode_farbfeld`, and the `encode_pam`/`encode_bmp[_rgba]` roundtrip
  on the stable toolchain — no nightly required.

## [0.1.5] - 2026-04-17

### Changed

- Bump zencodec to 0.1.19 (release prep)

## [0.1.4] - 2026-04-10

### Added

- Nightly fuzz workflow: 60s smoke on push, 5 min nightly (f4de1bf)
- Fuzz dictionary for BMP/PNM targets (ccea1c4)
- BMP roundtrip fuzz crash regression seed (cf3c0ad)
- Declare `hdr` and `tga` features, add zenbench dev-dep (022a599)

### Changed

- Bump zencodec to 0.1.13 (2085784)

### Fixed

- Restore `qoi` and `simd` features and deps for semver compatibility (a08c0bc)
- Reject BMP decompression bombs with insufficient pixel data (5455574)
- Correct BMP paletted row padding and palette handling (#3) (2c8f5d2, 2f587b3)
- Make memory cap configurable and remove dead code (8f64f60)
- Replace nonexistent `Limits::check_memory` with `check_output_size` (eeca252)
- Reject oversized dimensions to prevent OOM from crafted input (79014dc)

## 0.1.3 — 2026-04-01

### Changed

- Bump zencodec to 0.1.12
- Bump zenpixels/zenpixels-convert to 0.2.2
- Bump archmage, magetypes, enough, whereat, linear-srgb dependencies

## 0.1.0 — 2026-03-28

Initial release.

### Formats

- **PNM family** (always available): P5 (PGM), P6 (PPM), P7 (PAM), PFM — 8-bit, 16-bit, and float
- **Farbfeld** (always available): RGBA 16-bit
- **BMP** (`bmp` feature): all standard bit depths (1/2/4/8/16/24/32), RLE4/RLE8, BITFIELDS, palette expansion, bottom-up/top-down, grayscale detection, configurable permissiveness levels

### Decode

- Auto-detect format from magic bytes via `decode()` / `detect_format()`
- Zero-copy PNM decoding for maxval=255 (the common case) — no allocation, no copy
- Native byte order BMP decoding via `decode_bmp_native()` (skips BGR-to-RGB swizzle)
- Resource limits via `Limits` (max width, height, pixels, memory)
- Cooperative cancellation via `enough::Stop` on all decode paths

### Encode

- PGM, PPM, PAM, PFM, farbfeld, and BMP encoders
- All encoders accept BGR/BGRA/BGRX input (automatic swizzle)
- Checked arithmetic throughout — no silent overflow on output size calculation

### Type safety

- `rgb` feature: typed pixel API (`RGB8`, `RGBA8`, `as_pixels()`, `encode_*_pixels()`)
- `imgref` feature: 2D buffer API (`ImgVec`/`ImgRef`, `as_imgref()`, `decode_into()`)
- `zencodec` feature: zencodec trait integration for pipeline use

### Design

- `no_std` + `alloc`, `#![forbid(unsafe_code)]`, panic-free
- `BitmapError` is `#[non_exhaustive]`
- Fuzz targets included for PNM, BMP, and farbfeld decode paths
