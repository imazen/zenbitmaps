# QOI on real photos through zencodec's sans-I/O contract, 2026-10-14

Host: `mac` (Apple M4 Pro, macOS), rustc stable. zenbitmaps at the commit
that adds this file; zencodec PR 147 branch `feat/sansio-feed-step` at
`e97476cc`. zenbench `--cpu-policy=unrestricted` (UNVERIFIED_UNRESTRICTED:
the M4's asymmetric cores refuse verified placement). Wall time 166 s.

```
just bench-qoi-photos
```

## Inputs

The first two SDR renders of each imazen-26 camera category, fetched by
the recipe from the render index (`variant-sets/png-v3-index.tsv`):

| id | category | size |
|---|---|---|
| 1000 | 1000-lilith-photos-general | 4032x3024 |
| 1001 | 1000-lilith-photos-general | 4000x3000 |
| 1200 | 1200-lilith-interiors | 4000x3000 |
| 1201 | 1200-lilith-interiors | 3000x4000 |
| 1400 | 1400-lilith-nature | 4608x3456 |
| 1401 | 1400-lilith-nature | 4000x3000 |
| 1600 | 1600-lilith-food | 4032x3024 |
| 1601 | 1600-lilith-food | 4032x3024 |

All eight are opaque 8-bit RGB. Each is decoded with zenpng and resampled
with zenresize (Mitchell-Netravali filter, RGB8_SRGB format) to widths 64, 256, 1024
and 2048 at the original aspect ratio, plus the original; never upscaled.
One iteration processes all eight photos at that size; every arm is
checked for exact output before timing.

## Results (corpus total per iteration; Mpix/s last)

```
  photo-decode/w64 (8 photos, 0.0 MP, ratio 0.78)  40 rounds × 20 calls
  ├─ one-shot decode_qoi          188.6 ±48.2µs  [168.7–209.0]µs     143M [1]
  ├─ feed/step, whole slice       182.0 ±59.1µs  [-10.5%–+6.5%]      148M [2] [3]
  ├─ feed/step, 64 KiB windows    187.6 ±52.1µs  [-11.4%–+2.7%]      144M [4] [5]
  ╰─ feed/step, stream pump 64…   198.2 ±45.5µs  [+0.4%–+20.6%]      136M [6] [7]
  photo-encode/w64 (8 photos, 0.0 MP, ratio 0.78)  40 rounds × 87 calls
  ├─ one-shot encode_qoi        149.6 ±43.7µs  [133.5–166.4]µs     180M [1]
  ├─ Encoder::encode            136.4 ±23.0µs  [-26.5%–+9.6%]      197M [2] [3]
  ╰─ push_rows, 16-row strips   166.6 ±35.2µs  [-3.9%–+27.3%]      162M [4] [5]
  photo-transcode/w64 (8 photos, 0.0 MP, ratio 0.78)  40 rounds × 32 calls
  ├─ one-shot decode + encode     425.0 ±168.9µs  [376.7–472.8]µs     63.4M [1]
  ╰─ feed/step -> EncoderSink,…    413.1 ±67.3µs  [-16.2%–+11.0%]     65.2M [2] [3]
  photo-decode/w256 (8 photos, 0.4 MP, ratio 0.65)  40 rounds × 4 calls
  ├─ one-shot decode_qoi          2.6 ±0.5ms  [2.4–2.8]ms       166M [1]
  ├─ feed/step, whole slice       2.5 ±0.6ms  [-11.0%–+6.0%]    170M [2] [3]
  ├─ feed/step, 64 KiB windows    2.5 ±0.4ms  [-9.3%–+4.1%]     173M [4]
  ╰─ feed/step, stream pump 64…   2.6 ±0.5ms  [-7.2%–+9.2%]     165M [5] [6]
  photo-encode/w256 (8 photos, 0.4 MP, ratio 0.65)  40 rounds × 2 calls
  ├─ one-shot encode_qoi        4.6 ±0.5ms  [4.3–5.0]ms       92.9M [1]
  ├─ Encoder::encode            4.3 ±0.4ms  [-17.3%–+3.4%]    99.7M [2] [3]
  ╰─ push_rows, 16-row strips   4.4 ±0.5ms  [-11.0%–+5.8%]    97.8M [4] [5]
  photo-transcode/w256 (8 photos, 0.4 MP, ratio 0.65)  40 rounds × 1 calls
  ├─ one-shot decode + encode     8.9 ±2.1ms  [8.1–9.8]ms       48.2M [1]
  ╰─ feed/step -> EncoderSink,…   8.4 ±1.0ms  [-17.7%–+4.7%]    51.6M [2] [3]
  photo-decode/w1024 (8 photos, 6.9 MP, ratio 0.55)  25 rounds × 1 calls
  ├─ one-shot decode_qoi          20.7 ±0.5ms  [19.6–22.0]ms     334M
  ├─ feed/step, whole slice       19.6 ±0.2ms  [-3.9%–+0.2%]     351M [1]
  ├─ feed/step, 64 KiB windows    20.5 ±1.5ms  [+0.7%–+11.2%]    336M [2]
  ╰─ feed/step, stream pump 64…   21.8 ±2.4ms  [-4.6%–+15.5%]    316M [3]
  photo-encode/w1024 (8 photos, 6.9 MP, ratio 0.55)  23 rounds × 1 calls
  ├─ one-shot encode_qoi        30.7 ±6.7ms  [29.0–32.2]ms     225M
  ├─ Encoder::encode            28.1 ±0.3ms  [-15.8%–+0.0%]    245M [1]
  ╰─ push_rows, 16-row strips   29.2 ±0.2ms  [-12.5%–+3.2%]    237M [2]
  photo-transcode/w1024 (8 photos, 6.9 MP, ratio 0.55)  21 rounds × 1 calls
  ├─ one-shot decode + encode     48.0 ±0.5ms  [46.9–49.3]ms    144M
  ╰─ feed/step -> EncoderSink,…   49.3 ±4.2ms  [-2.0%–+7.1%]    140M [1]
  photo-decode/w2048 (8 photos, 27.6 MP, ratio 0.51)  27 rounds × 1 calls
  ├─ one-shot decode_qoi          75.7 ±0.7ms  [75.1–76.5]ms    365M
  ├─ feed/step, whole slice       74.3 ±0.6ms  [-2.3%–-0.4%]    372M [1]
  ├─ feed/step, 64 KiB windows    74.2 ±0.5ms  [-2.2%–-0.6%]    372M [2]
  ╰─ feed/step, stream pump 64…   76.0 ±1.5ms  [-0.8%–+1.5%]    363M [3]
  photo-encode/w2048 (8 photos, 27.6 MP, ratio 0.51)  25 rounds × 1 calls
  ├─ one-shot encode_qoi        108.0 ±2.7ms  [107.4–108.7]ms    256M
  ├─ Encoder::encode            107.5 ±0.5ms  [-1.5%–+0.5%]      257M [1]
  ╰─ push_rows, 16-row strips   110.8 ±0.7ms  [+1.6%–+3.5%]      249M
  photo-transcode/w2048 (8 photos, 27.6 MP, ratio 0.51)  22 rounds × 1 calls
  ├─ one-shot decode + encode     183.5 ±1.8ms  [182.8–184.2]ms    150M
  ╰─ feed/step -> EncoderSink,…   185.6 ±2.3ms  [+0.4%–+1.9%]      149M [1]
  photo-decode/original (8 photos, 100.5 MP, ratio 0.46)  8 rounds × 1 calls ⚠ only 8 rounds
  ├─ one-shot decode_qoi          253.8 ±2.5ms  [249.7–259.2]ms    396M
  ├─ feed/step, whole slice       248.9 ±3.5ms  [-1.6%–-0.5%]      404M [1]
  ├─ feed/step, 64 KiB windows    250.4 ±3.1ms  [-2.1%–-0.6%]      401M [2]
  ╰─ feed/step, stream pump 64…   253.5 ±1.7ms  [-0.2%–+1.4%]      396M [3]
  photo-encode/original (8 photos, 100.5 MP, ratio 0.46)  8 rounds × 1 calls ⚠ only 8 rounds
  ├─ one-shot encode_qoi        371.9 ±2.4ms  [370.7–373.2]ms    270M
  ├─ Encoder::encode            371.8 ±2.3ms  [-0.5%–+0.4%]      270M [1]
  ╰─ push_rows, 16-row strips   380.8 ±1.4ms  [+1.8%–+2.9%]      264M
  photo-transcode/original (8 photos, 100.5 MP, ratio 0.46)  7 rounds × 1 calls ⚠ only 7 rounds
  ├─ one-shot decode + encode     627.6 ±3.9ms  [624.0–633.3]ms    160M
  ╰─ feed/step -> EncoderSink,…   632.3 ±5.9ms  [+0.1%–+1.5%]      159M [1] [2]
  total: 155.6s  (406 noisy rounds)
```

## Reading

- Decode through feed/step, from a slice in one piece or in 64 KiB
  windows, is 1 to 2% faster than the one-shot decoder at 2048 px and at
  original size, and within noise below that. The stream pump, which pays
  one read copy the in-memory one-shot path never does, is within 2%.
- Streaming `push_rows` encode is 2 to 3% behind the one-shot encoder at
  2048 px and original size: the output grows by doubling, and each growth
  copies what was encoded so far. `Encoder::encode` is at parity.
- A transcode straight from the incremental decoder into the streaming
  encoder (no image buffer) is within 0 to 2% of decoding to a buffer and
  re-encoding it.
- At 64 px wide the groups take about 0.2 ms for all eight photos and the
  intervals are wide (CV around 25%); only the stream pump stands out,
  0 to 20% slower, from per-decode setup.

## Fixed during this work

| change | measured on | before | after |
|---|---|---|---|
| `push_rows` runs the encode kernel on stack copies of its state (it held references into the encoder struct, which kept the previous pixel and run count in memory) | photo-encode/w2048 | +9.3 to +11.2% | +1.6 to +3.5% |
| decoder reads offered bytes in place (`feed_and_step`), stitching at most 4 bytes across a window boundary | synthetic noise/4096 decode | +17% | within noise |
| zencodec-io's slice driver feeds sub-slices instead of copying into fetch buffers | synthetic noise/4096 decode | +17 to +22% | +5 to +8% (before in-place) |
| pump buffer starts at 8 KiB | photo-decode/w64, pump | +4 to +30% | +0 to +20% |
