# Farbfeld fixed-pixel append: Neoverse-N1 confirmation

**Accepted for one measured host only.** The RGBA8 farbfeld row writer now appends
each expanded pixel (`[r, r, g, g, b, b, a, a]`) in one call instead of four
sample appends. Output bytes are unchanged.

## Scope

- Measured on one native Neoverse-N1 (aarch64) cloud VM with eight vCPUs. The
  process boundary was cpuset CPUs 2–3 and the measurement thread was pinned to
  CPU 2.
- `cpu_frequency_verified=false` on every record.
- x86-64, other ARM cores and other placements are **unmeasured**. This record
  makes no claim for them.

## Sources and build

| item | value |
|---|---|
| baseline source | `a829fd164e51b08adbef8bccdb533c8f99635b2d`; `src/farbfeld/encode.rs` sha256 `e0c9f032600ce7f31be50906786acda87e19282140cd4576218f63cc133aeec8` |
| candidate source | `src/farbfeld/encode.rs` sha256 `95e16bfa10427782766297885760557ae7621700d1c4d99aab19f275598d0543` |
| benchmark harness | zenbench `45532049d831a6f61e25b5d3be8d0c41f3e37922` (0.1.11 + reviewed observer placement), as a path dependency |
| measured executable | sha256 `1106446ea1d70c1351d435e97101d210aff2a472b97b787830d89c383b9f182c` |
| toolchain | rustc 1.99.0 (`b940084d7`), aarch64-unknown-linux-gnu, default release profile, built on a separate host |

The two measured source trees differ only in the crate name and
`src/farbfeld/encode.rs`. The landed file is byte-identical to the measured
candidate (sha256 above).

## Protocol and result

- **Protocol:** predeclared before the build and before timing (`PROTOCOL.md`,
  sha256 `d7f02fc66a87f96842187bdaf669304b8d83ab5caf433680345024be732b80b1`).
  - zenbench Confirm with 15 fresh processes × 100 fixed rounds, 1 % margin and
    95 % confidence.
  - Ten groups; inference family ALLPAIRS, 30 comparisons (Bonferroni);
    workload `zenbitmaps-farbfeld-encode-v2`.
  - A full byte/spec preflight ran in every process before timing.
- **Result:** all 20 named comparisons pass.

| group | comparison | rule | estimate | family CI |
|---|---|---|---|---|
| rgba8-64 | baseline→candidate | improve beyond 1 % | −76.08 % | [−77.18, −74.91] |
| rgba8-64 | baseline→A/A | within ±1 % | −0.07 % | [−0.36, +0.22] |
| rgba8-256 | baseline→candidate | improve beyond 1 % | −76.07 % | [−76.42, −75.71] |
| rgba8-256 | baseline→A/A | within ±1 % | +0.08 % | [−0.13, +0.30] |
| rgba8-1024 | baseline→candidate | improve beyond 1 % | −74.68 % | [−74.95, −74.40] |
| rgba8-1024 | baseline→A/A | within ±1 % | +0.05 % | [−0.25, +0.36] |
| rgba8-4096 | baseline→candidate | improve beyond 1 % | −54.57 % | [−54.73, −54.40] |
| rgba8-4096 | baseline→A/A | within ±1 % | +0.01 % | [−0.29, +0.30] |
| rgba8-63x65 | baseline→candidate | improve beyond 1 % | −75.84 % | [−77.09, −74.52] |
| rgba8-63x65 | baseline→A/A | within ±1 % | +0.04 % | [−0.22, +0.29] |
| rgba8-4097x3 | baseline→candidate | improve beyond 1 % | −76.03 % | [−76.30, −75.75] |
| rgba8-4097x3 | baseline→A/A | within ±1 % | −0.04 % | [−0.23, +0.15] |
| rgba8-1x1 | baseline→candidate | noninferior within 1 % | −10.21 % | [−10.54, −9.89] |
| rgba8-1x1 | baseline→A/A | within ±1 % | +0.09 % | [−0.16, +0.34] |
| rgba16-1024 | baseline→candidate | control within ±1 % | +0.04 % | [−0.39, +0.47] |
| rgba16-1024 | baseline→A/A | within ±1 % | −0.09 % | [−0.34, +0.16] |
| rgb8-1024 | baseline→candidate | control within ±1 % | +0.11 % | [−0.29, +0.52] |
| rgb8-1024 | baseline→A/A | within ±1 % | +0.07 % | [−0.15, +0.30] |
| decode-1024 | old decode→new decode | control within ±1 % | −0.05 % | [−0.26, +0.17] |
| decode-1024 | old decode→old A/A | within ±1 % | +0.01 % | [−0.24, +0.26] |

- **Process and resource checks:** all 15 processes completed, and all
  15,000 group-rounds recorded clean resource observations.
- **Wrapper line:**
  `done rc=1 1004s | peak-RSS 0.58GiB | min-avail 14443MiB | peak-load 1.23`,
  with a 6 GiB cap and two jobs.
  - The native exit code 1 comes from the seven inverted candidate→A/A pairs,
    which are reported as Regressions. Those pairs are not part of the mapped
    verdict.
  - zenbench's validity list was empty, so it did not use exit code 2.

## Recorded discrepancies

- **Build flags:** the build record shows an ambient
  `RUSTFLAGS="-C target-cpu=neoverse-n1"` overridden by an empty
  `CARGO_ENCODED_RUSTFLAGS`. The protocol had said "explicit empty RUSTFLAGS".
  The verbose build log contains no `target-cpu` or `target-feature` in any
  rustc invocation, so no tuning reached the compiler.
- **Lockfile:** the harness lock was re-resolved once by an unlocked
  `cargo metadata` call. The only difference from the frozen seed lock is
  zenbench's registry source/checksum, replaced by the declared path dependency.
- **Observer placement:** the resource observer on CPU 3 is inferred from the
  fail-closed placement source of zenbench `45532049` and the recorded placement
  (Linux, enforced uniform cpuset). The timed run retained no observer-mask
  trace.

## Artifacts (checked on dev)

`/mnt/v/output/zenbitmaps/coz-2026-10-18/native-fixed-pixel-455-n1-p15r100-v1/`

- Contents:
  - the original run directory: protocol, executable, dry run, raw Confirm
    output, replicate records and 23 content-addressed correctness artifacts;
  - the build evidence: toolchain, environment, verbose build log, metadata,
    compiled-source archive, source archives and the parity tool.
- `SHA256SUMS.dev-mirror.txt` (106 files): sha256
  `2a26371f837a78172cd557e22817d5de9b207bcda4f521166e90b7587621e6ee`; all 106
  files verify.
- No R2 or tower mirror has been verified.

## Coz baseline (before this change)

`/mnt/v/output/zenbitmaps/coz-baseline-ccb705d0-2026-10-18/` on dev.

- **Run:** `just coz-run` scope (`%/zenbitmaps/src/%`), `encode rgba8 1024 1024
  30000` (5 + 3 × 30,000 encodes), coz 0.2.5.
- **Profile:** `encode-rgba8-1024x1024-30000.jsonl`, sha256
  `8390f764aa283f80154c06d8707d71501a819fe2ef556d6dfb421082c68cc14a`.
- **Coverage limit:** all 399 experiments and all but 2 of 1,476,568 in-scope
  samples fall on one line, the `write_farbfeld_rows` call in `encode_farbfeld`.
  The profile cannot attribute cost inside the row writer.
- It is source attribution only and supports no speedup claim.
