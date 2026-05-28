# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed

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
  width Gray8 and odd-width paletted 8bpp (319cfe18).
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
