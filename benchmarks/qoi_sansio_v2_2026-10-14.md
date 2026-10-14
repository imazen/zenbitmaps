# QOI on zencodec's sans-I/O boundary v2, 2026-10-14

Host: `mac` (Apple M4 Pro, 12 cores, macOS), rustc stable. zenbitmaps at the
commit that adds this file (parent `0c018f4c`); zencodec PR 147 branch
`feat/sansio-feed-step` at `34346a05` (v2: driver-owned input, single
`step`). zenbench `--cpu-policy=unrestricted` (UNVERIFIED_UNRESTRICTED: the
M4's asymmetric cores refuse verified placement). Spotlight indexing was
loading the host during an earlier attempt; these runs were taken after it
settled (load average 1 to 3).

```
just bench-qoi-sansio    # synthetic, 238.6 s
just bench-qoi-photos    # 8 imazen-26 photos, 156.9 s
```

Inputs for the photo bench are those of `qoi_photos_2026-10-14.md`.

## What changed since the feed/step records

- The decoder holds no input buffer. It decodes the rows the offered input
  guarantees (5 bytes per pixel, or everything once the input reaches the
  end of the source) straight from the driver's bytes into the sink, and
  otherwise asks for one worst-case row. The boundary stitching and the
  fallback row copy are gone.
- `push_rows` calls the encode kernel through an out-of-line function.
  Inlined into `push_rows`, 16-row strips of 2048-wide photos measured
  +11.0 to +13.2% against one-shot encode in this session; out of line,
  +0.6 to +2.6% (and +1.0 to +3.0% in the full run below).

## Results at the largest sizes (95% CI against the first row)

Real photos, 8 images:

| group | arm | mean | vs one-shot |
|---|---|---|---|
| decode w2048 (27.6 MP) | one-shot `decode_qoi` | 75.1 ms | |
| | step, whole slice | 72.9 ms | -4.0 to -2.3% |
| | step, 64 KiB windows | 73.0 ms | -3.3 to -1.7% |
| | step, stream pump 64 KiB | 74.3 ms | -2.2 to +0.0% |
| decode original (100.5 MP) | one-shot | 249.4 ms | |
| | step, whole slice | 243.1 ms | -3.0 to -2.1% |
| | step, stream pump 64 KiB | 246.6 ms | -1.3 to -1.1% |
| encode w2048 | one-shot `encode_qoi` | 107.8 ms | |
| | `push_rows`, 16-row strips | 109.9 ms | +1.0 to +3.0% |
| encode original | one-shot | 373.2 ms | |
| | `push_rows`, 16-row strips | 376.3 ms | +0.6 to +2.1% |
| transcode w2048 | one-shot decode + encode | 182.8 ms | |
| | step into `EncoderSink`, 64 KiB | 182.7 ms | -0.9 to +0.8% |
| transcode original | one-shot | 627.7 ms | |
| | step into `EncoderSink`, 64 KiB | 621.1 ms | -0.7 to +0.0% |

Synthetic 4096x4096 RGBA:

| group | arm | vs one-shot |
|---|---|---|
| decode noise (ratio 1.00) | step, whole slice | -0.7 to +1.2% |
| | step, 64 KiB chunks | -2.1 to +2.0% |
| | step, stream pump 64 KiB | +3.7 to +19.1% |
| decode photoish (ratio 0.37) | step, whole slice | +1.0 to +1.5% |
| | step, stream pump 64 KiB | +0.4 to +6.8% |
| decode pattern (ratio 0.55) | step, whole slice | -11.9 to -9.5% |
| encode noise | `push_rows`, 16-row strips | -0.9 to +4.1% |
| encode photoish | `push_rows`, 16-row strips | +2.7 to +5.5% |
| transcode noise | step into `EncoderSink`, whole | -2.8 to +1.1% |
| transcode photoish | step into `EncoderSink`, 64 KiB | -2.6 to +1.6% |

The feed/step design measured +16.5 to +17.1% for noise/4096 whole-slice
decode before its in-place fixes (`qoi_sansio_2026-10-14.md`). The stream
pump's remaining cost on incompressible input is the copy out of
`std::io::Read` into the pump's window, which a slice or `ReadAt` source
avoids. Groups below 1 MP have coefficients of variation of 24 to 47% on
this host and are not interpreted.

Not part of this change, recorded for follow-up: on synthetic 4096x4096
input, zencodec's `Encoder::encode` is +6.1 to +9.4% (noise) and +8.8 to
+13.7% (photoish) behind `encode_qoi`, while on 2048-wide photos the two
agree (-1.1 to +0.9%).

## Raw output

### Synthetic (`just bench-qoi-sansio`)

```
     Running benches/sansio_qoi.rs (target/release/deps/sansio_qoi-e40a49252ce63dd3)
  qoi-decode/pattern/256 (ratio 0.55)  40 rounds × 54 calls
  ├─ one-shot decode_qoi        195.4 ±44.5µs  [180.0–211.4]µs     335M [1]
  ├─ step, whole slice          181.8 ±40.8µs  [-20.3%–+6.9%]      361M [2] [3]
  ├─ step, 64 KiB chunks        179.6 ±43.4µs  [-22.5%–+1.1%]      365M [4] [5]
  ╰─ step, stream pump 64 KiB   198.6 ±48.8µs  [-12.9%–+17.0%]     330M [6] [7]
  qoi-encode/pattern/256  40 rounds × 38 calls
  ├─ one-shot encode_qoi        264.8 ±70.3µs  [242.2–287.6]µs     248M [1]
  ├─ Encoder::encode            278.6 ±96.4µs  [-8.3%–+19.6%]      235M [2] [3]
  ├─ push_rows, 16-row strips   260.6 ±45.5µs  [-26.3%–-6.5%]      251M [4]
  ╰─ push_rows, 1-row strips    284.4 ±66.6µs  [-17.7%–+5.5%]      230M [5] [6]
  qoi-transcode/pattern/256  40 rounds × 24 calls
  ├─ one-shot decode + encode     545.3 ±146.0µs  [502.5–590.4]µs     120M [1]
  ├─ step -> EncoderSink, whol…   469.3 ±120.0µs  [-25.9%–-2.0%]      140M [2]
  ╰─ step -> EncoderSink, 64 K…    484.8 ±98.8µs  [-23.6%–+1.7%]      135M [3] [4]
  qoi-decode/pattern/1024 (ratio 0.55)  40 rounds × 3 calls
  ├─ one-shot decode_qoi        3.3 ±0.9ms  [3.0–3.6]ms        320M [1]
  ├─ step, whole slice          3.1 ±1.1ms  [-18.3%–+10.9%]    333M [2] [3]
  ├─ step, 64 KiB chunks        2.9 ±0.7ms  [-26.9%–-6.1%]     362M [4]
  ╰─ step, stream pump 64 KiB   3.1 ±0.4ms  [-21.6%–+8.4%]     343M [5] [6]
  qoi-encode/pattern/1024  40 rounds × 2 calls
  ├─ one-shot encode_qoi        4.8 ±1.1ms  [4.4–5.3]ms        218M [1]
  ├─ Encoder::encode            5.2 ±1.6ms  [-19.0%–+15.9%]    203M [2] [3]
  ├─ push_rows, 16-row strips   4.5 ±1.4ms  [-15.7%–+8.9%]     232M [4] [5]
  ╰─ push_rows, 1-row strips    4.6 ±1.4ms  [-11.3%–+15.8%]    229M [6] [7]
  qoi-transcode/pattern/1024  40 rounds × 1 calls
  ├─ one-shot decode + encode     10.2 ±2.6ms  [9.4–11.0]ms      103M [1]
  ├─ step -> EncoderSink, whol…    8.8 ±1.9ms  [-24.9%–-2.1%]    119M [2]
  ╰─ step -> EncoderSink, 64 K…    9.1 ±2.2ms  [-23.4%–-2.7%]    115M [3]
  qoi-decode/pattern/4096 (ratio 0.55)  24 rounds × 1 calls
  ├─ one-shot decode_qoi        23.5 ±0.6ms  [22.5–24.6]ms      715M
  ├─ step, whole slice          20.5 ±0.8ms  [-11.9%–-9.5%]     817M
  ├─ step, 64 KiB chunks        20.9 ±0.9ms  [-12.9%–-10.1%]    801M
  ╰─ step, stream pump 64 KiB   21.5 ±0.7ms  [-7.3%–+1.9%]      781M [1]
  qoi-encode/pattern/4096  16 rounds × 1 calls
  ├─ one-shot encode_qoi        34.5 ±0.6ms  [34.0–35.1]ms     486M
  ├─ Encoder::encode            33.7 ±0.3ms  [-2.2%–-1.4%]     498M
  ├─ push_rows, 16-row strips   31.9 ±0.8ms  [-10.2%–-5.2%]    527M
  ╰─ push_rows, 1-row strips    32.1 ±0.4ms  [-9.4%–-4.2%]     522M
  qoi-transcode/pattern/4096  13 rounds × 1 calls
  ├─ one-shot decode + encode     57.2 ±1.4ms  [56.7–57.9]ms     293M
  ├─ step -> EncoderSink, whol…   51.8 ±1.3ms  [-11.0%–-8.2%]    324M
  ╰─ step -> EncoderSink, 64 K…   52.2 ±1.2ms  [-10.2%–-7.6%]    322M
  qoi-decode/noise/256 (ratio 1.00)  40 rounds × 60 calls
  ├─ one-shot decode_qoi        166.0 ±29.2µs  [156.6–175.2]µs     395M
  ├─ step, whole slice          172.2 ±40.0µs  [-5.6%–+13.6%]      380M [1] [2]
  ├─ step, 64 KiB chunks        174.4 ±41.1µs  [-5.7%–+10.6%]      376M [3] [4]
  ╰─ step, stream pump 64 KiB   186.9 ±49.5µs  [+2.9%–+22.8%]      351M [5]
  qoi-encode/noise/256  40 rounds × 25 calls
  ├─ one-shot encode_qoi         388.1 ±82.0µs  [360.0–416.3]µs     169M [1]
  ├─ Encoder::encode            419.4 ±100.8µs  [-2.2%–+18.6%]      156M [2] [3]
  ├─ push_rows, 16-row strips   399.7 ±102.6µs  [-4.9%–+11.6%]      164M [4] [5]
  ╰─ push_rows, 1-row strips     407.5 ±95.6µs  [-4.1%–+14.6%]      161M [6] [7]
  qoi-transcode/noise/256  40 rounds × 19 calls
  ├─ one-shot decode + encode     583.8 ±120.6µs  [544.5–623.3]µs     112M [1]
  ├─ step -> EncoderSink, whol…   568.0 ±190.8µs  [-11.2%–+6.1%]      115M [2] [3]
  ╰─ step -> EncoderSink, 64 K…    568.9 ±92.9µs  [-11.7%–+2.9%]      115M [4] [5]
  qoi-decode/noise/1024 (ratio 1.00)  40 rounds × 4 calls
  ├─ one-shot decode_qoi        2.5 ±0.8ms  [2.3–2.7]ms        421M [1]
  ├─ step, whole slice          2.6 ±0.8ms  [-7.5%–+14.6%]     407M [2] [3]
  ├─ step, 64 KiB chunks        2.4 ±0.5ms  [-10.4%–+10.6%]    429M [4] [5]
  ╰─ step, stream pump 64 KiB   2.6 ±0.8ms  [-6.9%–+16.6%]     400M [6] [7]
  qoi-encode/noise/1024  40 rounds × 1 calls
  ├─ one-shot encode_qoi        7.4 ±1.1ms  [6.7–8.2]ms       141M [1]
  ├─ Encoder::encode            8.2 ±1.9ms  [-4.5%–+19.1%]    128M [2] [3]
  ├─ push_rows, 16-row strips   7.0 ±1.3ms  [-10.1%–+8.0%]    149M [4] [5]
  ╰─ push_rows, 1-row strips    7.2 ±1.6ms  [-9.7%–+14.5%]    146M [6] [7]
  qoi-transcode/noise/1024  40 rounds × 1 calls
  ├─ one-shot decode + encode     10.2 ±1.8ms  [9.4–11.1]ms      103M [1]
  ├─ step -> EncoderSink, whol…   10.5 ±2.0ms  [-13.9%–+5.2%]    100M [2] [3]
  ╰─ step -> EncoderSink, 64 K…   10.0 ±1.6ms  [-14.6%–+4.2%]    105M [4] [5]
  qoi-decode/noise/4096 (ratio 1.00)  24 rounds × 1 calls
  ├─ one-shot decode_qoi        20.9 ±0.4ms  [20.1–21.9]ms     802M
  ├─ step, whole slice          21.2 ±0.5ms  [-0.7%–+1.2%]     793M [1]
  ├─ step, 64 KiB chunks        21.4 ±0.4ms  [-2.1%–+2.0%]     782M [2]
  ╰─ step, stream pump 64 KiB   23.3 ±1.0ms  [+3.7%–+19.1%]    721M
  qoi-encode/noise/4096  11 rounds × 1 calls
  ├─ one-shot encode_qoi        46.0 ±0.5ms  [45.6–46.6]ms    364M
  ├─ Encoder::encode            50.1 ±0.6ms  [+6.1%–+9.4%]    335M
  ├─ push_rows, 16-row strips   46.7 ±0.8ms  [-0.9%–+4.1%]    359M [1]
  ╰─ push_rows, 1-row strips    47.1 ±1.8ms  [+0.2%–+4.2%]    356M [2]
  qoi-transcode/noise/4096  10 rounds × 1 calls
  ├─ one-shot decode + encode     67.5 ±1.0ms  [66.8–68.3]ms    248M
  ├─ step -> EncoderSink, whol…   67.0 ±1.8ms  [-2.8%–+1.1%]    250M [1] [2]
  ╰─ step -> EncoderSink, 64 K…   67.5 ±0.7ms  [-1.6%–+1.6%]    248M [3]
  qoi-decode/photoish/256 (ratio 0.36)  40 rounds × 38 calls
  ├─ one-shot decode_qoi        316.1 ±71.6µs  [290.7–342.8]µs     207M [1]
  ├─ step, whole slice          287.0 ±87.0µs  [-21.9%–+3.8%]      228M [2] [3]
  ├─ step, 64 KiB chunks        273.6 ±60.2µs  [-22.7%–+0.5%]      240M [4] [5]
  ╰─ step, stream pump 64 KiB   288.7 ±67.5µs  [-21.3%–+4.1%]      227M [6] [7]
  qoi-encode/photoish/256  40 rounds × 21 calls
  ├─ one-shot encode_qoi        535.5 ±110.2µs  [504.2–566.7]µs     122M
  ├─ Encoder::encode            523.1 ±114.5µs  [-12.9%–+8.8%]      125M [1] [2]
  ├─ push_rows, 16-row strips   521.8 ±111.3µs  [-11.6%–+6.1%]      126M [3] [4]
  ╰─ push_rows, 1-row strips    535.1 ±136.9µs  [-9.8%–+9.8%]       122M [5] [6]
  qoi-transcode/photoish/256  40 rounds × 10 calls
  ├─ one-shot decode + encode     1.1 ±0.1ms  [1.0–1.1]ms      62.1M [1]
  ├─ step -> EncoderSink, whol…   1.1 ±0.1ms  [-3.9%–+6.6%]    62.0M [2] [3]
  ╰─ step -> EncoderSink, 64 K…   1.0 ±0.1ms  [-4.4%–+3.1%]    63.0M [4]
  qoi-decode/photoish/1024 (ratio 0.37)  40 rounds × 1 calls
  ├─ one-shot decode_qoi        7.5 ±0.9ms  [6.9–8.1]ms       140M [1]
  ├─ step, whole slice          7.8 ±1.2ms  [-6.1%–+7.1%]     134M [2] [3]
  ├─ step, 64 KiB chunks        7.6 ±1.0ms  [-8.9%–+4.6%]     138M [4] [5]
  ╰─ step, stream pump 64 KiB   7.8 ±0.8ms  [-2.5%–+12.6%]    134M [6] [7]
  qoi-encode/photoish/1024  40 rounds × 1 calls
  ├─ one-shot encode_qoi         9.8 ±1.9ms  [9.1–10.5]ms       107M [1]
  ├─ Encoder::encode            11.1 ±2.5ms  [+4.7%–+21.8%]    94.7M [2]
  ├─ push_rows, 16-row strips   10.1 ±1.7ms  [-7.7%–+7.1%]      104M [3] [4]
  ╰─ push_rows, 1-row strips    10.8 ±2.2ms  [-5.5%–+13.1%]    97.4M [5] [6]
  qoi-transcode/photoish/1024  40 rounds × 1 calls
  ├─ one-shot decode + encode     16.2 ±4.4ms  [15.0–17.3]ms     64.9M [1]
  ├─ step -> EncoderSink, whol…   14.6 ±3.2ms  [-22.2%–-2.3%]    71.9M [2]
  ╰─ step -> EncoderSink, 64 K…   14.7 ±3.2ms  [-20.2%–+2.1%]    71.5M [3] [4]
  qoi-decode/photoish/4096 (ratio 0.37)  9 rounds × 1 calls ⚠ only 9 rounds
  ├─ one-shot decode_qoi        55.1 ±0.3ms  [54.5–56.1]ms    305M
  ├─ step, whole slice          56.0 ±0.4ms  [+1.0%–+1.5%]    300M [1]
  ├─ step, 64 KiB chunks        56.4 ±1.1ms  [-0.5%–+5.2%]    298M [2]
  ╰─ step, stream pump 64 KiB   57.1 ±0.6ms  [+0.4%–+6.8%]    294M [3]
  qoi-encode/photoish/4096  7 rounds × 1 calls ⚠ only 7 rounds
  ├─ one-shot encode_qoi        76.9 ±0.5ms  [76.1–78.0]ms     218M
  ├─ Encoder::encode            85.7 ±1.0ms  [+8.8%–+13.7%]    196M
  ├─ push_rows, 16-row strips   78.9 ±2.0ms  [+2.7%–+5.5%]     213M
  ╰─ push_rows, 1-row strips    78.8 ±2.5ms  [-0.1%–+5.1%]     213M [1]
  qoi-transcode/photoish/4096  6 rounds × 1 calls ⚠ only 6 rounds
  ├─ one-shot decode + encode     133.6 ±3.0ms  [132.0–135.4]ms    126M
  ├─ step -> EncoderSink, whol…   133.8 ±1.7ms  [-2.1%–+2.6%]      125M [1]
  ╰─ step -> EncoderSink, 64 K…   132.7 ±1.3ms  [-2.6%–+1.6%]      126M [2]
```

### Real photos (`just bench-qoi-photos`)

```
  photo-decode/w64 (8 photos, 0.0 MP, ratio 0.78)  40 rounds × 22 calls
  ├─ one-shot decode_qoi        186.8 ±41.2µs  [171.2–202.7]µs     144M [1]
  ├─ step, whole slice          181.3 ±65.0µs  [-19.6%–-3.7%]      149M [2]
  ├─ step, 64 KiB windows       181.5 ±50.3µs  [-10.3%–+4.7%]      148M [3] [4]
  ╰─ step, stream pump 64 KiB   217.2 ±61.0µs  [-3.1%–+16.9%]      124M [5] [6]
  photo-encode/w64 (8 photos, 0.0 MP, ratio 0.78)  40 rounds × 92 calls
  ├─ one-shot encode_qoi        152.4 ±62.5µs  [131.2–176.1]µs     177M [1]
  ├─ Encoder::encode            134.2 ±19.2µs  [-36.6%–+12.2%]     201M [2] [3]
  ╰─ push_rows, 16-row strips   140.8 ±30.5µs  [-17.4%–+20.0%]     191M [4] [5]
  photo-transcode/w64 (8 photos, 0.0 MP, ratio 0.78)  40 rounds × 37 calls
  ├─ one-shot decode + encode     409.2 ±159.3µs  [363.6–456.5]µs     65.8M [1]
  ╰─ step -> EncoderSink, 64 K…   367.8 ±174.1µs  [-28.2%–+7.1%]      73.3M [2] [3]
  photo-decode/w256 (8 photos, 0.4 MP, ratio 0.65)  40 rounds × 4 calls
  ├─ one-shot decode_qoi        2.6 ±0.4ms  [2.4–2.7]ms       167M
  ├─ step, whole slice          2.5 ±0.7ms  [-12.0%–+5.4%]    173M [1] [2]
  ├─ step, 64 KiB windows       2.5 ±0.6ms  [-8.6%–+7.7%]     171M [3] [4]
  ╰─ step, stream pump 64 KiB   2.6 ±0.6ms  [-9.5%–+7.4%]     169M [5] [6]
  photo-encode/w256 (8 photos, 0.4 MP, ratio 0.65)  40 rounds × 3 calls
  ├─ one-shot encode_qoi        4.1 ±1.1ms  [3.7–4.4]ms       106M [1]
  ├─ Encoder::encode            3.8 ±0.5ms  [-21.9%–-4.2%]    114M [2]
  ╰─ push_rows, 16-row strips   3.9 ±0.6ms  [-13.9%–+7.7%]    110M [3] [4]
  photo-transcode/w256 (8 photos, 0.4 MP, ratio 0.65)  40 rounds × 1 calls
  ├─ one-shot decode + encode     8.7 ±1.7ms  [8.0–9.5]ms        49.4M [1]
  ╰─ step -> EncoderSink, 64 K…   8.6 ±1.6ms  [-13.2%–+11.0%]    50.1M [2] [3]
  photo-decode/w1024 (8 photos, 6.9 MP, ratio 0.55)  25 rounds × 1 calls
  ├─ one-shot decode_qoi        20.8 ±0.1ms  [19.5–22.2]ms     332M
  ├─ step, whole slice          19.7 ±0.3ms  [-2.6%–+7.1%]     350M [1]
  ├─ step, 64 KiB windows       20.4 ±1.7ms  [-0.9%–+6.9%]     338M [2]
  ╰─ step, stream pump 64 KiB   21.5 ±2.2ms  [-7.4%–+14.2%]    321M [3]
  photo-encode/w1024 (8 photos, 6.9 MP, ratio 0.55)  23 rounds × 1 calls
  ├─ one-shot encode_qoi        30.5 ±5.3ms  [28.9–32.0]ms     226M
  ├─ Encoder::encode            28.0 ±0.4ms  [-15.9%–+0.2%]    247M [1]
  ╰─ push_rows, 16-row strips   28.8 ±0.3ms  [-13.3%–+3.0%]    240M [2]
  photo-transcode/w1024 (8 photos, 6.9 MP, ratio 0.55)  21 rounds × 1 calls
  ├─ one-shot decode + encode     48.1 ±0.7ms  [46.8–49.5]ms    144M
  ╰─ step -> EncoderSink, 64 K…   48.6 ±5.0ms  [-4.3%–+6.3%]    142M [1]
  photo-decode/w2048 (8 photos, 27.6 MP, ratio 0.51)  28 rounds × 1 calls
  ├─ one-shot decode_qoi        75.1 ±0.6ms  [74.7–75.6]ms    368M
  ├─ step, whole slice          72.9 ±0.4ms  [-4.0%–-2.3%]    379M
  ├─ step, 64 KiB windows       73.0 ±0.5ms  [-3.3%–-1.7%]    378M
  ╰─ step, stream pump 64 KiB   74.3 ±1.3ms  [-2.2%–+0.0%]    372M [1]
  photo-encode/w2048 (8 photos, 27.6 MP, ratio 0.51)  25 rounds × 1 calls
  ├─ one-shot encode_qoi        107.8 ±2.1ms  [107.1–108.4]ms    256M
  ├─ Encoder::encode            107.7 ±1.3ms  [-1.1%–+0.9%]      256M [1]
  ╰─ push_rows, 16-row strips   109.9 ±2.3ms  [+1.0%–+3.0%]      251M
  photo-transcode/w2048 (8 photos, 27.6 MP, ratio 0.51)  22 rounds × 1 calls
  ├─ one-shot decode + encode     182.8 ±1.4ms  [182.1–183.4]ms    151M
  ╰─ step -> EncoderSink, 64 K…   182.7 ±2.1ms  [-0.9%–+0.8%]      151M [1]
  photo-decode/original (8 photos, 100.5 MP, ratio 0.46)  9 rounds × 1 calls ⚠ only 9 rounds
  ├─ one-shot decode_qoi        249.4 ±0.8ms  [248.8–249.9]ms    403M
  ├─ step, whole slice          243.1 ±0.8ms  [-3.0%–-2.1%]      413M
  ├─ step, 64 KiB windows       243.7 ±1.6ms  [-2.7%–-1.8%]      412M [1]
  ╰─ step, stream pump 64 KiB   246.6 ±1.1ms  [-1.3%–-1.1%]      407M
  photo-encode/original (8 photos, 100.5 MP, ratio 0.46)  8 rounds × 1 calls ⚠ only 8 rounds
  ├─ one-shot encode_qoi        373.2 ±3.2ms  [370.1–377.2]ms    269M
  ├─ Encoder::encode            372.3 ±1.6ms  [-0.4%–+0.5%]      270M [1]
  ╰─ push_rows, 16-row strips   376.3 ±0.5ms  [+0.6%–+2.1%]      267M [2]
  photo-transcode/original (8 photos, 100.5 MP, ratio 0.46)  7 rounds × 1 calls ⚠ only 7 rounds
  ├─ one-shot decode + encode     627.7 ±4.7ms  [621.3–637.9]ms    160M
  ╰─ step -> EncoderSink, 64 K…   621.1 ±1.3ms  [-0.7%–+0.0%]      162M [1] [2]
```
