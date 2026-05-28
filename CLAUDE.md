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

- **BMP roundtrip pixel mismatch (fuzz, pre-existing).** `fuzz_roundtrip`
  (`fuzz/fuzz_targets/fuzz_roundtrip.rs:56`) asserts `decode_bmp` →
  `encode_bmp`/`encode_bmp_rgba` → `decode_bmp` is pixel-identical, and a
  fuzz input fails that assertion ("BMP roundtrip pixel mismatch"). Surfaced
  by CI run 26546560011 (2026-05-28). NOT related to QOI — `decode_bmp`/
  `encode_bmp` only. Crash artifact uploaded by that run as
  `crash-f38ce8cfe9f7f562ad08c4fcb629b5a1c9cd52e4`. Reproduce:
  `cargo fuzz run fuzz_roundtrip <artifact>`. Likely a BMP encode/decode
  path divergence (palette/bitfield/origin handling) — needs investigation.

## User Feedback Log

See [FEEDBACK.md](FEEDBACK.md) if it exists.
