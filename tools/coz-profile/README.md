# Farbfeld profiling worker

This standalone development crate calls zenbitmaps' existing public encoder
and decoder. It does not change the production dependency graph.

```sh
cargo build --release --manifest-path tools/coz-profile/Cargo.toml
./tools/coz-profile/target/release/zenbitmaps-coz-profile encode rgba8 512 512 1000
```

Arguments are `encode|decode layout width height iterations [dump-path]`.
Layouts: `rgba8`, `rgb8`, `bgra8`, `bgrx8`, `bgr8`, `gray8`, `rgba16`.
Input is deterministic synthetic bytes (seed `7942c31b`), not a photograph
or a quality dataset. Model- or content-quality claims are outside this tool.

Each process runs five untimed warmup operations, optionally followed by
a wall-time warmup frozen by the external plan in `ZENBITMAPS_COZ_WARMUP_MS`,
then three timed batches. Each `elapsed_ns=` row is the total elapsed nanoseconds for the
requested number of complete operations, including output allocation and
deallocation. Input generation and SHA256 checks are outside those batches.
The output slice is consumed with `black_box` on every iteration.

For [Coz](https://github.com/plasma-umass/coz), the
[Rust bindings](https://github.com/plasma-umass/coz/tree/master/rust) mark each
completed operation. The release profile includes DWARF line tables and thin
LTO. Verify source-attributed experiments and progress on a short probe before
collecting longer profiles. Use the resource controls required by your host.

For `zenbench pair`, use a prefix extractor `elapsed_ns=`, unit `ns`, expected
count 3, and a JSON correctness extractor with prefix `result=` and pointers
`/sha256`, `/bytes`, `/width`, `/height`, `/input_layout`. Declare all inputs,
the actual iteration counts, retained binary hashes, and CPU placement.
Freeze the plan and A/A controls before confirmation; exploratory medians
are not confirmation.

`sha256sum` is required for complete-byte output validation. The validation
child is joined and does not inherit `LD_PRELOAD`, so it cannot append a second
process's Coz profile to the parent's file. Optional dumps use create-new
semantics and never replace an existing artifact. Encode dumps are farbfeld
files; decode dumps are native-endian RGBA16 bytes. Keep generated data out
of Git and preserve its provenance.

This worker covers farbfeld operations only. Other bitmap formats, streaming
adapters, and cancellation behavior require their own tests and measurements.

## Coz run

`just coz-build` builds this worker from the committed lockfile (`--locked`).
`just coz-run` refuses to replace an existing profile and limits experiments to
`zenbitmaps/src`. `COZ_BIN` selects a profiler other than `coz` on `PATH`.

```sh
just coz-build
just coz-run "$HOME/tmp/zenbitmaps-encode-rgba8.jsonl" encode rgba8 512 512 1000
```
