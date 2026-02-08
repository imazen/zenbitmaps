# zenpnm

PNM/PAM/PFM and BMP image format decoder and encoder.

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

### BMP (`basic-bmp` feature, opt-in)
- Uncompressed 24-bit (RGB) and 32-bit (RGBA)
- No RLE, no indexed color

## Credits

PNM implementation draws from [zune-ppm](https://github.com/etemesi254/zune-image)
by Caleb Etemesi (MIT/Apache-2.0/Zlib licensed).

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

(none yet)

## User Feedback Log

See [FEEDBACK.md](FEEDBACK.md) if it exists.
