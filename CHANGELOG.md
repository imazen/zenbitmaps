# Changelog

All notable changes to this project will be documented in this file.

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
