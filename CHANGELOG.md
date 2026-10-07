# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed
- Dependency floors raised to the tested versions: `enough` 0.4.4, `archmage` 0.9.29, `garb` 0.2.8, `imgref` 1.12.3, `rgb` 0.8.53, `thiserror` 2.0.21; dev: `zenbench` 0.1.10, `codec-corpus` 1.1.0. All compatible; no majors were available (5d8eef4).

- **`zencodec` / `zencodec-testkit` / `zenpixels` / `zenpixels-convert`
  requirements now span the published minor and the next one**: `zencodec
  >=0.1.26, <0.3.0`, `zencodec-testkit >=0.1.0, <0.3.0`, `zenpixels >=0.2.13,
  <0.4.0`, `zenpixels-convert >=0.2.13, <0.4.0`. For a `0.x` crate Cargo treats
  the minor as the major, so the previous plain requirements meant `^0.1.26` =
  `>=0.1.26, <0.2.0` and a `zencodec 0.2.0` release would have been invisible
  until this manifest was hand-edited. Floors are unchanged and nothing newer is
  published, so resolution is identical (`cargo metadata --all-features`: one
  copy of each). **Caveat now recorded next to the dev-dep:** the *published*
  `zencodec-testkit 0.1.0` still declares `zencodec ^0.1.26`, so it must
  republish with the widened range before `zencodec 0.2.0` ships or this graph
  would carry two `zencodec` copies. The standing current-plus-next rule is
  documented in the zencodec repo's `CLAUDE.md`.

- **Third-party lockfile refresh — 22 packages, no manifest change needed.**
  `Cargo.lock` only. Run as `cargo update -p …` naming each of the 84
  third-party packages individually, with every zen-family crate
  (`zencodec`, `zencodec-testkit`, `zenpixels`, `zenpixels-convert`, `zenbench`,
  `zenutils-fuzz`, `codec-corpus`, `enough`, `whereat`, `archmage`, `garb`)
  excluded, so nothing in the sibling graph moved. Notable: `thiserror`
  2.0.18 → 2.0.20, `imgref` 1.12.2 → 1.12.3, `bytemuck` 1.25.0 → 1.25.2,
  `bytemuck_derive` 1.10.2 → 1.12.0, `libc` 0.2.186 → 0.2.189, `memchr`
  2.8.2 → 2.8.3, `serde` 1.0.228 → 1.0.229, `bitflags` 2.13.0 → 2.13.1, plus
  `clap`/`proc-macro2`/`quote`/`syn`/`serde_json`/`zmij`/`libredox`/`rustversion`.
  Every direct third-party requirement (`imgref "1.12"`, `rgb "0.8"`,
  `thiserror "2.0"`) is a caret range that already admits the newest release,
  so no manifest edit was required — `cargo outdated --root-deps-only` reports
  only zen-family crates behind, and those are deliberately out of scope here.
  All 417 tests pass, `cargo clippy --all-targets -- -D warnings` (the CI
  invocation) is clean, `cargo fmt --all --check` is clean, the committed
  public-API snapshot is unchanged, and `cargo hack check --rust-version`
  still passes at the declared MSRV of 1.93.

  The two **nested** tracked lockfiles were refreshed under the same
  constraint: `fuzz/Cargo.lock` (14 packages — `getrandom` 0.3.4 → 0.4.3,
  `libc` 0.2.189, `cc` 1.4.4) and `apidoc/Cargo.lock` (14 packages —
  `rustdoc-types` 0.57.4, `serde` 1.0.229, `syn` 3.0.4). Both still
  `cargo check --all-targets` clean, and the committed public-API snapshot is
  still current under the newer `rustdoc-types`/`public-api`.

### Fixed

- PNM encode polls cancellation between bounded payload chunks instead of
  copying the whole image at once (edc6ed5). Copy chunks span 64 KiB–1 MiB;
  Gray8-to-Rgb8 chunks span 16–256 Ki samples. This avoids tiny copies for
  narrow images. Byte-equivalence and narrow-image polling tests pass.

- **Pushes to `main` now cancel their superseded CI runs.** `ci.yml` keyed its
  concurrency group on `${{ github.head_ref || github.run_id }}`.
  `github.head_ref` is populated only for `pull_request` events, so on a push it
  was empty and the group fell through to `github.run_id` — unique per run, so no
  two pushes ever shared a group and `cancel-in-progress` could never fire. Every
  push started a full matrix that ran to completion even when several commits
  landed seconds apart. Now keyed on `${{ github.ref }}`, which is set for both
  event types, so PR cancellation is unchanged and consecutive pushes supersede
  each other.

- **CI's `Clippy` job was red since at least 2026-08-27**: `swap_rb_3` /
  `swap_rb_4` are called only from the `bmp`, `tga` and `qoi` modules, all of
  which are off at the default feature set (`default = []`), so
  `cargo clippy --all-targets -- -D warnings` failed on `dead_code`. Both are
  now gated on `any(feature = "bmp", feature = "tga", feature = "qoi")`, as are
  the two tests that exercise them — CI's `cargo test --features bmp` leg keeps
  those running, so no coverage is lost. No behaviour change.

- **The `Fuzz regression` CI job could not fail.** It ran
  `cargo test --test fuzz_regression 2>/dev/null || echo "No regression test
  found…"` inside an `if [ -d fuzz/regression ]` guard, so a genuinely failing
  suite, a missing corpus, and a missing harness all reported green.
  `tests/fuzz_regression.rs` has existed the whole time, so the fallback was
  masking real failures rather than covering a missing target. The step is now
  a bare `cargo test --test fuzz_regression`, and the harness asserts at least
  `MIN_SEEDS` (5) replayable seeds are present — `zenutils_fuzz::RegressionSuite`
  treats a missing or empty seed dir as a clean no-op, so an emptied or renamed
  `fuzz/regression/` previously passed without replaying anything.
  Mutation-verified: removing the corpus and injecting a panic into the `decode`
  target each fail the test with exit code 101.

- **1/2/4-bpp uncompressed BMP rows were read without the mandatory 4-byte
  row alignment**, silently scrambling every scanline after the first for any
  width whose packed row size isn't a multiple of 4 (issue #20, 2026-08-26
  ultracode sweep, adversarially verified — the 8/16/24-bit branches already
  skipped the aligned stride; the standard pal1/pal4 fixtures are 127 px wide
  and accidentally aligned, which is why they never caught it). The sub-byte
  branch now skips to the aligned stride per row. Regression: hand-crafted
  9x3 1-bpp fixture with junk padding bytes; mutation-verified (fails with
  the skip removed). Also cleared the new clippy `-D warnings` wall
  (chunks_exact → as_chunks, expand_bits rewritten on `as_chunks_mut`).

### Added

- Implement `zencodec::CategorizedError` for `BitmapError` (the origin-first
  `ErrorCategory` taxonomy, now published as zencodec `0.1.26`; originally
  landed against the pre-release `caterr-reshape` branch via a since-removed
  temporary `[patch.crates-io]` pin) plus a
  `From<BitmapError> for At<zencodec::CodecError>` bridge, so every
  `zencodec` trait adapter (`type Error`) is now `At<zencodec::CodecError>`
  (Pattern B) instead of the bare `At<BitmapError>` (Pattern A) — the
  category and originating codec name (`"zenbitmaps"`) now survive
  dyn-dispatch type erasure. New `BitmapError::OutOfMemory(String)` (genuine
  `try_reserve`/allocator-exhaustion failures and `checked_mul` address-space
  overflows — the latter also covers the pre-existing `DimensionsTooLarge`
  variant, which now categorizes as `Resource(OutOfMemory)` too) and
  `BitmapError::DecompressionBomb(String)` (the BMP RLE-ratio and
  1024×-amplification guards, now routed to the new
  `LimitKind::DecompressionRatio` instead of a generic `InvalidData`) —
  distinct from a configured `ResourceLimits` cap
  (`BitmapError::LimitExceeded`, unchanged shape, still categorizes to
  `Resource(Limits(kind))` via a message-prefix classifier locked by a test
  against every real construction site). QOI header decode errors
  (`rapid_qoi::DecodeError`) are matched variant-by-variant instead of
  stringified via `{e:?}` — `NotEnoughData` now categorizes as
  `Image(UnexpectedEof)` and `InvalidMagic` as `Image(Unsupported(Type))`,
  rather than both collapsing into one `InvalidHeader(String)` bucket. Added
  `tests/truncation_series.rs` (zencodec-testkit
  `check_decode_truncation_series`, one test per format) verifying a
  truncated encoded file always categorizes as incomplete-input, never an
  internal/OOM/caller-fault category.
- Honor `zencodec::AllocPreference` (3-mode, per-site) at untrusted decode
  allocations, and implement `estimate_decode_resources` for all six bitmap
  `DecoderConfig`s. Each format's full-image output buffer (sized from the
  untrusted header dimensions) now defaults to the fallible `try_reserve` path —
  a malicious header gets a graceful `BitmapError::LimitExceeded` rather than an
  abort — while bounded per-row / palette scratch keeps the fast infallible
  `vec!`. `ResourceLimits::prefer_fallible_allocations` overrides every site at
  the zencodec decode boundary (`Fallible`/`Infallible` force one path,
  `CodecDefault` keeps each site's default); the bare `decode()` API is
  unchanged. New crate-local `AllocPref` (mirror of `AllocPreference`, so the
  always-compiled decode pipeline needn't name the optional dep) + `alloc_util`
  helpers. `estimate_decode_resources` delegates to a shared
  `codec::trivial_decode_resources`: working set ≈ output buffer only
  (`ThreadingInformation::SERIAL`, core-adjusted), structural not calibrated.
- vCPU-aware resource estimation via zencodec's unified `estimate` API. All six
  bitmap `EncoderConfig`s (PNM, BMP, farbfeld, HDR, TGA, QOI) now override
  `estimate_encode_resources(&ImageCharacteristics, &ComputeEnvironment)`,
  returning a core-adjusted `ResourceEstimate`. They share a
  `codec::trivial_encode_resources` helper: these formats encode in a single
  serial pass (`ThreadingInformation::SERIAL`) with peak ≈ input + output and a
  linear pixel-count time term. The per-format output ratios (~1.0× for the raw
  formats, ~0.6× for QOI) are structural placeholders, not a measured fit.

### Changed

- **BREAKING (0.2.0):** `BitmapError::UnsupportedVariant` now carries a
  leading `UnsupportedKind` tag (`Type` | `Feature`) ahead of the existing
  `String` message: `UnsupportedVariant(String)` →
  `UnsupportedVariant(UnsupportedKind, String)`. `Type` = the format,
  container, or named sub-format/profile isn't implemented at all (an
  unrecognized BMP compression scheme or bit depth, a PNM sub-format like
  PBM, a whole format disabled at compile time); `Feature` = the format (and
  specific sub-variant) *is* handled, but a narrower configuration within it
  (a bit depth, pixel-layout combination, header field value) isn't. Maps to
  `ErrorCategory::Image(ImageError::Unsupported(UnsupportedImageKind::Type |
  ::Feature))`. **Migration:** `BitmapError::UnsupportedVariant(msg)` →
  `BitmapError::UnsupportedVariant(_, msg)`, or `UnsupportedVariant(..)` if
  the kind and message are both irrelevant to the match. The zencodec
  adapters' own "caller supplied a pixel format this encoder can't produce"
  case no longer constructs `UnsupportedVariant` at all — it now rejects via
  the existing `zencodec::UnsupportedOperation::PixelFormat` axis (already
  used for the codec's other capability-negotiation failures), which is a
  caller-request-origin failure, not an image-format one.
- deps: zencodec is now the published crates.io `0.1.26` and the conformance
  harness dev-dependency is the published `zencodec-testkit 0.1.0`; the
  temporary `[patch.crates-io] zencodec = { git, .. }` pin for the
  pre-release taxonomy (and its interim v0.1.26 release-tag retarget,
  `c0e2e0a`) is removed — the whole dependency graph resolves from crates.io.
- deps: migrate to published zencodec 0.1.24 estimate API; drop the temporary
  `[patch.crates-io] zencodec = { git, rev = "0f71295" }` pin (the `estimate` API
  is now on crates.io). The shared `codec::trivial_encode_resources` helper follows
  the refined `ResourceEstimate` API: `ResourceEstimate::new(typ, time_ms as u64)`
  (`wall_ms` is now `u64`, was `f32`), `.with_peak_max(typ + input)` (replaces the
  dropped `.with_peak_range(min, max)`), and the `.with_output_bytes(..)` call is
  gone. All six codec configs delegate to it, so the change is one site.
- **BREAKING (0.2.0):** the public error type is now `At<BitmapError>`
  (`whereat::At` wrapping the `BitmapError` enum), re-exported as
  `zenbitmaps::At` alongside a `pub type Result<T> = Result<T, At<BitmapError>>`.
  Every `decode*`/`encode*`/`probe_bmp` entry point — and the `zencodec`
  adapters' associated `Error` types — now return the wrapped form, which
  carries a captured `file:line` location (and, via
  `whereat::define_at_crate_info!`, a GitHub source link) for server-side
  stack traces on malformed-input failures. **Migration:** match on the inner
  enum through `err.error()` (returns `&BitmapError`), e.g.
  `matches!(result.as_ref().map_err(|e| e.error()), Err(BitmapError::LimitExceeded(_)))`
  or `match err.error() { BitmapError::UnrecognizedFormat => .. }`. The
  `BitmapError` enum itself (variant names, `#[non_exhaustive]`) is unchanged;
  only the wrapper is new. Bumps to 0.2.0 (leftmost-non-zero 0.x break).
- Decode now applies a default 120 MP pixel cap (`DEFAULT_MAX_PIXELS`) even when
  no explicit `Limits` are supplied, matching the always-on 1 GiB
  `DEFAULT_MAX_MEMORY_BYTES` byte cap and the wider fleet's 120 MP house
  convention. **Behavior change:** a header declaring more than 120 MP is now
  rejected at the pre-flight dimension check before allocation, whereas
  previously the byte cap alone admitted far larger pixel counts for low-bpp
  formats (e.g. a ~1 G grayscale-px header fit under 1 GiB and decoded). The
  dimension check (`check_dimensions`) now reads the default via `unwrap_or`,
  so it fires on the no-limits path the same way the byte cap already did.
  Opt out by setting `Limits { max_pixels: Some(u64::MAX), .. }`; set a
  smaller `max_pixels` to lower the ceiling. Closes #13. Regression:
  `tests/default_pixel_cap.rs` + `limits::tests`.

### Fixed

- **PAM re-encode roundtrip is now lossless for 16-bit ASCII PPM (fuzz
  zenbitmaps#10).** A binary P6 16-bit PPM downscales to `Rgb8` (there is no
  16-bit RGB layout), but the ASCII P3 path keyed its output byte width on
  `maxval > 255` alone and emitted *two* bytes per sample while still tagging the
  buffer `Rgb8` — producing, e.g., a 6-byte 1×1 "Rgb8" image. `encode_pam` then
  copied `width·height·channels` (3) bytes, truncating the buffer, so
  `decode → encode_pam → decode` mismatched (left 6 bytes, right 3). The ASCII
  decoder (`decode_ascii_samples`) now sizes output by the *layout*: 2 raw bytes
  only for a genuinely 16-bit-per-channel layout (`Gray16`), and downscales
  16-bit samples to one `u8` for 8-bit layouts (16-bit P3 PPM → `Rgb8`), byte-for-byte
  matching the binary P6 path. The `Gray16` ASCII precision-preservation fix
  (zenpipe#51) is unchanged. Regression: `tests/roundtrip.rs`
  (`p3_ascii_ppm_16bit_downscales_to_rgb8`, `p3_ascii_and_p6_binary_16bit_agree`),
  plus the strengthened `tests/fuzz_regression.rs` `roundtrip` target (now asserts
  pixel-equality, not just no-panic) over seed
  `fuzz/regression/fuzz_roundtrip/pnm-p3-16bit-rgb8-roundtrip-zenbitmaps-10`.
- **`Gray16` decode byte order is now consistent across binary and ASCII PNM
  (#12).** `PixelLayout::Gray16` is documented native-endian, but the binary
  P5/P7 path returned the on-disk *big-endian* bytes verbatim
  (`decode_integer_transform`) while the ASCII P2 path emitted *native-endian*
  `u16` (`decode_ascii_samples`), so the same logical 16-bit image decoded to two
  different `pixels()` buffers on little-endian hosts — a consumer reinterpreting
  the bytes as `&[u16]` got byte-swapped values from binary inputs. The binary
  decode path now byte-swaps big-endian on-disk samples to host order, and
  `encode_pam` writes `Gray16` back out big-endian (converting from the
  native-endian buffer), mirroring the farbfeld `Rgba16` convention.
  **Behavior change (little-endian hosts only):** `decode()` of a binary 16-bit
  PGM/PAM now returns native-endian bytes where it previously returned
  big-endian; the on-disk PAM produced by `encode_pam` is unchanged
  (big-endian). `decode → encode_pam → decode` stays pixel-lossless, and is a
  no-op on big-endian hosts. Regressions: `tests/roundtrip.rs`
  (`p5_binary_gray16_decodes_native_endian`, `p5_binary_and_p2_ascii_gray16_agree`,
  `p7_pam_binary_gray16_agrees_with_ascii`, `pam_roundtrip_gray16_lossless`,
  `encode_pam_gray16_writes_big_endian_on_disk`).

### Docs

- README: overhauled to the zen* house conventions — standardized badge row
  (CI/crates.io/lib.rs/docs.rs/MSRV/license, flat-square, no `branch=`), a
  `## Quick start`, the rendered crosslink footer, and a generated, badge-free
  `README.crates.md` (`readme = "README.crates.md"`) so the GitHub and crates.io
  surfaces can't drift. Replaced the headline GiB/s table with a support matrix
  plus `benchmarks/README.md` methodology (no unverified numbers carried over),
  bumped the documented dependency version to 0.2, and corrected the QOI credit
  to a vendored rapid-qoi core.
- README: document the byte conventions that were previously undocumented and
  could cause silent pixel corruption in a downstream renderer — PFM row order
  (decoder normalizes the on-disk bottom-to-top scanlines to top-down) and
  endianness (scale sign selects file byte order; samples returned as
  native-endian `f32` with the scale magnitude applied), and `Gray16` byte order
  (decode returns native-endian `u16` from both binary P5/P7 and ASCII P2;
  `encode_pam` writes big-endian on disk — see #12, which reconciled the code
  paths these docs originally described as divergent). Also added the real
  `encode_*`/`DecodeOutput` signatures and
  field-vs-method shapes, the `encode_ppm` `maxval = 255` / RGB-only contract and
  its accepted/rejected layouts, the `Limits` units + the always-on 1 GiB
  `DEFAULT_MAX_MEMORY_BYTES` default cap (and that the zero-copy borrowed path is
  gated by dimension limits, not `max_memory_bytes`), and corrected the
  PPM/PAM decode-output layout table (PNM never yields `GrayAlpha*`/`Rgb16`/
  `Rgba16`; 16-bit RGB/RGBA downscale to 8-bit). Found via an insulated
  external-developer usability test of the published README.

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
