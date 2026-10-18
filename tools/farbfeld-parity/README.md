# Farbfeld Rgba8 parity tool

Development-only check that a candidate revision's farbfeld encoder produces
the same results as a baseline revision on the `Rgba8` path, and that both
match the farbfeld format definition. It does not change the production
dependency graph and is never built by `cargo test` or CI.

```sh
just farbfeld-parity --scratch ~/tmp/ff-parity-1 --candidate <rev> --groups core,f6
```

`run.py` resolves both revisions with `git rev-parse` and extracts them with
`git archive` into a new scratch directory. It never checks anything out, and
it never touches the working copy or any other repository. The scratch
directory must not exist yet. It must not be under `/tmp`, `/run` or `/dev/shm`,
it must not be on tmpfs or ramfs, and it must be outside this repository.

The scratch directory holds:
- `old/`: the baseline archive, with its package renamed `zenbitmaps_old`.
- `new/`: the candidate archive.
- `harness/`: a generated crate built from `harness/Cargo.toml.in` and
  `harness/src/main.rs`. Its `Cargo.lock` starts from the candidate's lockfile.
- `target/`, `logs/` (the full output of every build and run), `oracle/` and
  `identity.json`.

`identity.json` records:
- the declared and resolved revisions
- the encoder source's SHA-256 in both trees
- this tool's file hashes
- the rustc and cargo versions
- the harness lockfile and binary hashes
- each group's result

Nothing in the scratch directory is meant for Git.

- `--baseline` defaults to `a92accff501d22af2b75405a7b7b99c9520f2fb4`.
- `--candidate` defaults to `HEAD`. In a colocated jj checkout, `HEAD` is the
  parent of the working-copy commit (`@-`). To test `@` itself, pass its commit
  id.
- `--offline` passes `--offline` to cargo.

## Groups

Only the requested groups run (default `core`). A requested group that cannot
run fails: a missing `magick`, an uninstalled target, or a binary that cannot
execute all count as failures. The exit status is 0 only when every requested
group passes.

| Group | What it checks |
|---|---|
| `core` | 2,093 random inputs (23 widths × 13 heights × 7 trailing lengths), all 256 values per channel, 10 invalid inputs, cancellation at every check point, and 1920×1080 / 4096×4096 / 4095×4097. It requires: baseline == candidate == spec; returned capacity == length; identical errors, including `at!` file:line locations (only the two archive roots are normalized); identical `Stop::check` counts, and that count still matching the baseline's schedule (one check, then one every 16th row). Built with overflow checks and debug assertions on. |
| `f6` | Zero height with widths up to `u32::MAX`, for every layout, on the host. Built twice: overflow checks on, and the shipping release settings. Correct behavior is a 16-byte header-only file or a typed error. |
| `i686` | `core` built for `i686-unknown-linux-gnu`. The binary runs directly on the host, not through a cargo runner, so the host must run 32-bit x86 Linux binaries. |
| `i686-f6` | The `f6` probe on `i686-unknown-linux-gnu`, both profiles. |
| `imagemagick` | Writes candidate files with input-derived expectations, then decodes them with ImageMagick (`magick ... -depth 16 -endian MSB rgba:` and `-depth 8 rgba:`). The 16-bit read-back must equal the spec, and the 8-bit read-back must equal the raw input. ImageMagick only reads; it writes nothing the tool ships. |
| `mutants` | Replays a fixed excerpt of the in-repo test `rgba8_matches_native_rgba16_expansion_with_odd_rows_and_input_tail` (as of `b76f4229`) against 13 deliberately wrong encoders. Every one must be caught. |

The spec oracle in `harness/src/main.rs` is written from the format definition:
magic, big-endian `u32` width and height, then big-endian `u16` RGBA, where
8-bit `v` becomes `v * 257`. It calls nothing in zenbitmaps.

## Regression: zero-height wide-image panic on 32-bit targets

With `height == 0`, the pixel-count checks pass, but the per-row stride
`width * bytes_per_pixel` overflows a 32-bit `usize`. With overflow checks on,
that panics with "attempt to multiply with overflow". With the shipping
release settings it panics with "chunk size must be non-zero" whenever the
stride wraps to zero.

This affects `Rgba8`, `Rgba16`, `Bgra8` and `Bgrx8` at both settings, and `Rgb8`
and `Bgr8` with overflow checks only. `Gray8` is unaffected. At `040205d3` and
`b76f4229`, `i686-f6` therefore fails, with identical results in both trees.
The probe reports these panics as failures, never as accepted behavior. It
passes only when no case panics and both trees agree.

The default baseline includes the fix (`325bd951`) and the exact unsupported-layout
regression checks (`a92accff`). All five groups passed against that baseline on
2026-10-18, including native execution of i686 binaries.
