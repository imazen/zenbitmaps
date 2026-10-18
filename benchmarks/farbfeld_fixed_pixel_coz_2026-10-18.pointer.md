# Farbfeld fixed-pixel append: Coz profiles before and after

**These profiles are source-attribution evidence only.** They are not timing evidence.

- The accepted speed record is
  [`farbfeld_fixed_pixel_n1_confirm_2026-10-18.pointer.md`](farbfeld_fixed_pixel_n1_confirm_2026-10-18.pointer.md).
- Wall times and `elapsed_ns` rows recorded under Coz include injected delays. They are not
  ordinary speed.
- Experiment and sample counts depend on how long each run lasted. They do not show a
  speedup.

## Matched runs

Both runs used the same setup and ran once each with fixed counts:

- **Host and toolchain:** dev (x86-64), Rust 1.99.0, no `target-cpu`.
- **Profiler:** coz 0.2.5 with coz-rs 0.1.3.
- **Worker build:** `just coz-build` (`--locked`, release with line tables) in a fresh target
  directory.
- **Command:** the `just coz-run` scope `%/zenbitmaps/src/%`, argv
  `encode rgba8 1024 1024 30000` (5 + 3 × 30,000 encodes), and `ZENBITMAPS_COZ_WARMUP_MS`
  unset.

| | before | after |
|---|---|---|
| compiled source | `ccb705d0`; `src/farbfeld/encode.rs` sha256 `e0c9f032600ce7f31be50906786acda87e19282140cd4576218f63cc133aeec8` | `14ca757c`; `src/farbfeld/encode.rs` sha256 `95e16bfa10427782766297885760557ae7621700d1c4d99aab19f275598d0543` |
| worker sha256 | `00989ec8eb25ae3dead233b4a881150615432486fcfa753cdfdb98bf7d7be180` | `0ce3967555ef7d2bacec19dfb1c0fb2c1956f388cece31bf6882b5ff2ef25bf5` |
| profile sha256 | `8390f764aa283f80154c06d8707d71501a819fe2ef556d6dfb421082c68cc14a` | `ba95f4aac66c77d9fb97fc656b7912c01d01710ac7bb7c09d84e406111837353` |
| input sha256 (LCG seed `0x7942c31b`, 4,194,304 B) | `c307b88b497e7af2475aa1e2badb4c56a576a78e6653c805916be8874d537a86` | same |
| output sha256 (8,388,624 B) | `dbd753c852b2d0acb68c3c570bc9202dccd4a7e6fe63ebb35db2e8fa6aed3489` | identical |
| experiments | 399 | 87 |
| selected lines | 1 | 1 |
| speedup points | 21 | 21 |
| in-scope samples | 1,476,568 | 93,072 |

In both runs every experiment has selected samples, and all experiments sit on the same
line: `src/farbfeld/encode.rs:62`, the `write_farbfeld_rows` call in `encode_farbfeld`.
That line also holds every in-scope sample except 2.

**Coverage limit:** neither profile attributes any cost inside the row writer, which is where
the change is.

## Archive

**dev** keeps the working copies under `/mnt/v/output/zenbitmaps/`:

- `coz-2026-10-18/native-fixed-pixel-455-n1-p15r100-v1/`;
- `coz-build-fix-2026-10-18/`;
- `coz-baseline-ccb705d0-2026-10-18/`;
- `coz-post-14ca757c-2026-10-18/`;
- `d6-landing-2026-10-18/`.

**tower** (NAS) holds a copy at `/mnt/tower/output/zenbitmaps-coz-archive-2026-10-18/`:

- **Contents:** the N1 confirmation directory with its manifest log, the build-fix, both
  profile directories and the landing gates. That is 160 files and 306,904,343 bytes, with no
  Cargo target output.
- **Manifest:** `_archive-logs/source.sha256`, sha256
  `99cf799a9328e09ccfe7204d85081e4bbbe070ce00aa802e5c376df25e375dc8`. It is byte-identical to
  `_archive-logs/dest-direct.sha256`, which re-hashed every destination file with direct reads
  after the copy.
- The N1 directory's own `SHA256SUMS.dev-mirror.txt` verifies 106/106 on tower.
