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

- **Gray16 decode byte-order divergence between binary and ASCII PNM #12.**
  `PixelLayout::Gray16` is documented native-endian, but binary P5/P7 decode
  returned the on-disk *big-endian* bytes verbatim (`decode_integer_transform`,
  `src/pnm/decode.rs`) while ASCII P2 emitted *native-endian* `u16`
  (`decode_ascii_samples`), so the same logical 16-bit image decoded to two
  different `pixels()` buffers on little-endian hosts — reinterpreting as
  `&[u16]` gave byte-swapped values from binary inputs. **Fix:** the binary
  Gray16 arm now byte-swaps big-endian on-disk samples to host order
  (`u16::from_be_bytes` → `to_ne_bytes`), and `encode_pam`'s new Gray16 arm
  writes big-endian back out (`from_ne_bytes` → `to_be_bytes`), mirroring
  farbfeld's `Rgba16` convention — so both decode paths agree, the
  `decode → encode_pam → decode` roundtrip stays pixel-lossless, and the on-disk
  PAM is spec-compliant. No-op on big-endian hosts. Also corrected the
  `encode_pam` capacity hint (was `pixel_count·depth`, which under-allocated
  Gray16 by half since DEPTH 1 ≠ 2 bytes/px). Regressions in `tests/roundtrip.rs`
  (`p5_binary_gray16_decodes_native_endian`, `p5_binary_and_p2_ascii_gray16_agree`,
  `p7_pam_binary_gray16_agrees_with_ascii`, `pam_roundtrip_gray16_lossless`,
  `encode_pam_gray16_writes_big_endian_on_disk`). Verified the regressions catch
  it: reverting the decode arm makes all five fail with the byte-swap signature
  (`[52,18,…]` vs `[18,52,…]`).

- **fuzz_roundtrip libFuzzer OOM #7 (rss_limit 2048MB) — verified fixed on main.**
  The decode-bomb (>2 GiB RSS from ~8 KB inputs) is closed by the layered
  guards now present on every decode path: (a) the always-on 120 MP pixel cap
  (`DEFAULT_MAX_PIXELS`, landed b52a9d5 / #14 — *after* the 06-11/06-12 CI
  failures, which is why those OOMed) plus the 1 GiB byte cap
  (`DEFAULT_MAX_MEMORY_BYTES`); and (b) input-proportional guards —
  uncompressed BMP output ≤ 1024× input (`src/bmp/decode.rs:784`), RLE ≤ 256×
  input (`:1212`), and binary PNM / farbfeld require the declared pixel bytes to
  be present before allocating. The encoders are bounded by the (already-capped)
  decoded buffer, so the worst small-input roundtrip holds ≤3 capped buffers.
  **Measured:** the full `fuzz_roundtrip` logic over all 294 farm crashes + the
  local `oom-*` artifacts peaks at 0.14 GiB RSS (was the OOM corpus); the old
  `oom-012c6491…` artifact now returns `Err` at ~3 MB RSS. No code change was
  needed — only regression coverage was added. Regressions:
  `tests/default_pixel_cap.rs` (`pnm_binary_over_cap_*`, `pnm_ascii_over_cap_*`,
  `farbfeld_over_cap_*`, `pnm_binary_truncated_under_cap_*`), the existing
  `amplification-bomb-{16,32}bpp` BMP seeds, and `tests/bmp_rle_dos_regression.rs`.

- **fuzz_roundtrip PNM PAM roundtrip pixel mismatch #10.** `fuzz_roundtrip`
  asserted `decode → encode_pam → decode` is pixel-identical and failed on
  16-bit ASCII PPM (P3, maxval > 255). **Root cause:** `decode_ascii_samples`
  (`src/pnm/decode.rs`) keyed its output byte width on `maxval > 255` and emitted
  2 bytes/sample for a P3 PPM whose layout is `Rgb8` (no Rgb16 layout exists) —
  a 6-byte 1×1 "Rgb8" buffer. `encode_pam` then copied `w·h·channels` (3) bytes,
  truncating, so the roundtrip mismatched (left 6 bytes, right 3). The binary P6
  16-bit path was always correct (downscales to `Rgb8`); only ASCII disagreed.
  **Fix:** the ASCII decoder now sizes output by the *layout* — 2 raw bytes only
  for a 16-bit-per-channel layout (`Gray16`), else downscale 16-bit samples to one
  `u8` (`val·255/maxval`), byte-for-byte matching the binary path. The Gray16
  precision fix (zenpipe#51) is unchanged. Regressions: `tests/roundtrip.rs`
  (`p3_ascii_ppm_16bit_downscales_to_rgb8`, `p3_ascii_and_p6_binary_16bit_agree`)
  + strengthened `tests/fuzz_regression.rs` roundtrip target (asserts
  pixel-equality) over seed `pnm-p3-16bit-rgb8-roundtrip-zenbitmaps-10`.
  Verified the regression catches it: re-introducing the maxval-keyed logic makes
  `fuzz_regression` fail with "PNM PAM roundtrip pixel mismatch".

### Previously Fixed

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
  (`crash-f38ce8cf…`, CI run 26546560011). Fixed bc497d28, regression tests in
  `tests/roundtrip.rs` (`bmp_roundtrip_gray8_*`, `bmp_roundtrip_paletted8_odd_width`).

## User Feedback Log

See [FEEDBACK.md](FEEDBACK.md) if it exists.
