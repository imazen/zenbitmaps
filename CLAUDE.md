# zenbitmaps

PNM/PAM/PFM, BMP, and farbfeld image format decoder and encoder.

See `/home/lilith/work/codec-design/README.md` for API design guidelines.

## Purpose

Reference bitmap formats for codec testing and apples-to-apples comparisons.
These are lossless, simple formats used as ground truth for encode/decode pipelines.

## Supported Formats

### PNM family (always available)
- **P5** (PGM binary) — grayscale, 8-bit and 16-bit
- **P6** (PPM binary) — RGB, 8-bit and 16-bit
- **P7** (PAM) — arbitrary channels, 8-bit and 16-bit
- **PFM** — floating-point grayscale and RGB

### Farbfeld (always available)
- RGBA 16-bit (native endian output)
- Encode from Rgba16, Rgba8, Rgb8, Gray8

### BMP (`bmp` feature, opt-in)
- Headers: WinBMPv2–v5, OS/2 (12/16/40/52/56/64/108/124-byte info headers)
- Bit depths: 1, 2, 4, 8, 16, 24, 32
- Compression: uncompressed, RLE4, RLE8, BITFIELDS
- Color palettes, bottom-up/top-down, grayscale detection
- Encode: uncompressed 24-bit (RGB) and 32-bit (RGBA)
- `BmpPermissiveness` enum: `Strict` / `Standard` (default) / `Permissive`
- 1 GiB hard output cap prevents OOM from pathological headers

## Credits

PNM implementation draws from [zune-ppm](https://github.com/etemesi254/zune-image),
BMP from [zune-bmp](https://github.com/etemesi254/zune-image) 0.5.2,
and farbfeld from [zune-farbfeld](https://github.com/etemesi254/zune-image) 0.5.2,
all by Caleb Etemesi (MIT/Apache-2.0/Zlib licensed).

## Design Rules

Same as other zen* codecs — see codec-design/README.md. Key points:
- `with_` prefix for builder setters, bare-name for getters
- `#![forbid(unsafe_code)]`, no_std+alloc
- No backwards compatibility needed (0.x)

## Build Commands

- `just check` — cargo check --all-features
- `just fmt` — cargo fmt
- `just clippy` — clippy with -D warnings
- `just test` — cargo test --all-features
- `just check-no-std` — check wasm32 target

## Known Bugs

(none currently open)

### Fixed

- **BMP 8bpp decode roundtrip pixel corruption (fuzz).** `fuzz_roundtrip`
  (`fuzz/fuzz_targets/fuzz_roundtrip.rs:56`) asserted `decode_bmp` →
  `encode_bmp`/`encode_bmp_rgba` → `decode_bmp` is pixel-identical and failed
  on 8bpp BMPs. **Root cause:** the 8-bit-grayscale (Gray8) scanline reader in
  `src/bmp/decode.rs` shared the 24-bit RGB code path and applied the BGR↔RGB
  channel swap (`chunks_exact_mut(3).swap(0, 2)`) to single-channel Gray8 rows,
  scrambling pixels in 3-byte groups and dropping the trailing remainder on odd
  widths. Encoders (`encode_8bit_gray`, `encode_24bit`) were always correct;
  the defect was purely in decode. **Fix:** gate the channel swap on
  `num_components == 3` so Gray8 passes through untouched. Covered the
  no-palette Gray8 crash (`crash-760b7c45…`) and the paletted finding-#1 class
  (`crash-f38ce8cf…`, CI run 26546560011). Fixed 319cfe18, regression tests in
  `tests/roundtrip.rs` (`bmp_roundtrip_gray8_*`, `bmp_roundtrip_paletted8_odd_width`).

## User Feedback Log

See [FEEDBACK.md](FEEDBACK.md) if it exists.
