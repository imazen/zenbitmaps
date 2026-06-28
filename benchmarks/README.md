# zenbitmaps benchmarks

Throughput methodology and reproduction for the codec micro-benchmark. Numbers are
machine-specific — this document tells you how to produce your own; it does **not**
bake in measurements from an unrecorded environment.

## What is measured

`benches/codecs.rs` (run via [zenbench](https://github.com/imazen/zenbench)) times
decode and encode throughput for all six codecs on a single 1000×1000 synthetic
image:

| Group | Codecs | Input layout |
|-------|--------|--------------|
| `decode_1mpx` | ppm, farbfeld, bmp, qoi, tga, hdr | re-decode a buffer each codec encoded once up front |
| `encode_1mpx` | ppm, farbfeld, bmp, qoi, tga, hdr | encode the in-memory pixel buffer |

Source pixels are RGB8 for the byte-oriented formats, RGBA16 for farbfeld, and
RgbF32 for Radiance HDR (each format's natural unit). Throughput is reported
against the RGB8 pixel byte count (`1000 × 1000 × 3`).

## Methodology (how the harness stays honest)

- **zenbench**, not criterion — interleaved A/B execution (round-robin across the
  codecs in a group) so every contender sees the same thermal / scheduler state,
  with paired statistics. See the [zenbench](https://github.com/imazen/zenbench)
  README.
- **Single-threaded** — each codec runs one encode/decode on the calling thread.
- **No I/O in the timed region** — every buffer is encoded/loaded into RAM before
  the measured loop; the closure only decodes from `&[u8]` or encodes into a
  `Vec<u8>`.
- **Output is consumed** with `zenbench::black_box` so the work isn't optimized
  away.
- **PNM decode of maxval-255 input is zero-copy** (a borrowed slice into the input
  — no allocation), so its "throughput" is memcpy/validation-class, not a measure
  of pixel reconstruction work. Read it as a floor, not a codec speed.

## Reproduce

Build **without** `-C target-cpu=native` — runtime SIMD dispatch (archmage/garb)
is what ships, and `native` bakes in ISA extensions the released binary won't use.

```sh
git clone https://github.com/imazen/zenbitmaps && cd zenbitmaps
git checkout <full-commit-sha>          # the commit you are measuring
cargo bench --bench codecs --all-features
```

The `simd` feature (pulled in by `--all-features`) accelerates the BGR↔RGB swizzle
via [garb](https://github.com/imazen/garb) on the TGA and QOI encode paths; drop it
to measure the scalar fallback.

## Recording results

When you capture numbers, commit them next to this file as
`benchmarks/codecs_<YYYY-MM-DD>.csv` (or `.tsv`) with a header that records, per the
house benchmarking rules:

- CPU model, RAM, OS, `rustc -V`, and the build profile;
- the full commit SHA the numbers came from;
- the exact command and feature set.

Without those fields a number is not reproducible and should not be quoted in the
crate README.

## Extending: size sweep

This micro-benchmark is **single-size** (1000×1000), which conflates per-call fixed
overhead with per-pixel work. To separate them, sweep at least tiny (≤64×64), small
(256×256), medium (~1 MP), and large (4096×4096) and fit `total = α + β · pixels`,
reporting both the intercept (fixed overhead) and the slope (per-pixel cost). A
single "GiB/s" figure without that intercept is misleading at the small end.

## Choosing the chart

For "which codec is fastest?", use a horizontal bar chart sorted by throughput
(zenbench's `sort_by_speed`), one bar per codec, decode and encode as separate
series. Avoid pie/3D/dual-axis charts.
