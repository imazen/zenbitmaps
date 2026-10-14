# QOI through zencodec's sans-I/O contract, 2026-10-14

Host: `mac` (Apple M4 Pro, macOS), rustc stable. zenbitmaps at the commit
that adds this file; zencodec PR 147 branch `feat/sansio-feed-step` at
`51a40cc` (local path override). zenbench `--cpu-policy=unrestricted`
(UNVERIFIED_UNRESTRICTED: the M4's asymmetric cores refuse verified
placement), each group capped at 2 s / 40 rounds.

```
just bench-qoi-sansio
# = cargo bench --bench sansio_qoi --features all,zencodec,std -- --cpu-policy=unrestricted
```

Every arm is checked for exact output before timing. One-shot arms are
the crate's free functions (`decode_qoi`, `encode_qoi`); every other arm
goes through zencodec: `DecodeJob::dyn_incremental_decoder` driven by
`zencodec-io` (`drive_from_slice`, `pump_read_to_sink`), the
`Encoder` trait (`encode`, `push_rows`/`finish`), and `EncoderSink` for
a transcode with no intermediate image. RGBA8 input. Images: `pattern`
(the codecs.rs pattern), `noise` (every pixel a full opcode), `photoish`
(gradient plus light noise).

## Results (mean, Mpix/s in the last column)

```
  qoi-decode/pattern/256 (ratio 0.55)  40 rounds × 51 calls
  ├─ one-shot decode_qoi          185.8 ±38.7µs  [166.9–206.3]µs     353M [1]
  ├─ feed/step, whole slice       198.6 ±50.1µs  [-4.2%–+26.0%]      330M [2] [3]
  ├─ feed/step, 64 KiB chunks     196.4 ±60.9µs  [+1.5%–+25.8%]      334M [4]
  ╰─ feed/step, stream pump 64…   198.7 ±61.9µs  [-1.5%–+25.3%]      330M [5] [6]
  qoi-encode/pattern/256  40 rounds × 37 calls
  ├─ one-shot encode_qoi        264.1 ±68.9µs  [233.1–300.2]µs     248M [1]
  ├─ Encoder::encode            295.0 ±82.3µs  [+5.5%–+42.6%]      222M [2]
  ├─ push_rows, 16-row strips   274.0 ±63.3µs  [-5.3%–+20.6%]      239M [3] [4]
  ╰─ push_rows, 1-row strips    271.1 ±68.7µs  [+0.6%–+26.8%]      242M [5] [6]
  qoi-transcode/pattern/256  40 rounds × 22 calls
  ├─ one-shot decode + encode     559.2 ±162.0µs  [503.7–616.5]µs     117M [1]
  ├─ feed/step -> EncoderSink,…   526.6 ±105.9µs  [-15.3%–+11.2%]     124M [2] [3]
  ╰─ feed/step -> EncoderSink,…   506.1 ±102.1µs  [-23.2%–+3.9%]      129M [4] [5]
  qoi-decode/pattern/1024 (ratio 0.55)  40 rounds × 3 calls
  ├─ one-shot decode_qoi          3.1 ±0.8ms  [2.9–3.4]ms       334M [1]
  ├─ feed/step, whole slice       3.6 ±0.9ms  [+0.6%–+30.4%]    290M [2] [3]
  ├─ feed/step, 64 KiB chunks     3.4 ±0.9ms  [-3.5%–+20.1%]    308M [4] [5]
  ╰─ feed/step, stream pump 64…   3.4 ±0.7ms  [-4.9%–+22.8%]    308M [6] [7]
  qoi-encode/pattern/1024  40 rounds × 2 calls
  ├─ one-shot encode_qoi        4.5 ±1.1ms  [4.1–4.9]ms        232M [1]
  ├─ Encoder::encode            5.3 ±1.2ms  [+1.8%–+32.2%]     199M [2]
  ├─ push_rows, 16-row strips   4.6 ±1.1ms  [-11.0%–+11.8%]    230M [3] [4]
  ╰─ push_rows, 1-row strips    4.7 ±1.0ms  [-10.1%–+16.6%]    225M [5] [6]
  qoi-transcode/pattern/1024  40 rounds × 1 calls
  ├─ one-shot decode + encode     9.8 ±2.7ms  [9.0–10.7]ms      107M [1]
  ├─ feed/step -> EncoderSink,…   9.4 ±1.6ms  [-15.6%–+7.7%]    111M [2] [3]
  ╰─ feed/step -> EncoderSink,…   9.5 ±1.6ms  [-13.5%–+7.3%]    110M [4] [5]
  qoi-decode/pattern/4096 (ratio 0.55)  21 rounds × 1 calls
  ├─ one-shot decode_qoi          22.8 ±0.2ms  [21.9–23.8]ms     736M
  ├─ feed/step, whole slice       24.6 ±0.2ms  [+7.9%–+9.3%]     683M
  ├─ feed/step, 64 KiB chunks     24.8 ±0.3ms  [+6.5%–+10.2%]    677M
  ╰─ feed/step, stream pump 64…   25.2 ±0.6ms  [+3.1%–+17.8%]    667M
  qoi-encode/pattern/4096  15 rounds × 1 calls
  ├─ one-shot encode_qoi        31.2 ±0.5ms  [30.5–32.1]ms      537M
  ├─ Encoder::encode            35.9 ±0.4ms  [+11.6%–+18.5%]    467M
  ├─ push_rows, 16-row strips   34.1 ±1.0ms  [+3.8%–+15.1%]     492M
  ╰─ push_rows, 1-row strips    34.5 ±1.4ms  [+6.1%–+14.7%]     487M
  qoi-transcode/pattern/4096  12 rounds × 1 calls
  ├─ one-shot decode + encode     54.6 ±1.7ms  [53.7–55.5]ms    307M
  ├─ feed/step -> EncoderSink,…   57.8 ±1.8ms  [+3.0%–+9.0%]    290M
  ╰─ feed/step -> EncoderSink,…   57.5 ±0.4ms  [+3.1%–+7.4%]    292M
  qoi-decode/noise/256 (ratio 1.00)  40 rounds × 55 calls
  ├─ one-shot decode_qoi          171.2 ±37.3µs  [156.0–187.9]µs     383M [1]
  ├─ feed/step, whole slice       199.9 ±54.7µs  [+6.5%–+35.7%]      328M [2]
  ├─ feed/step, 64 KiB chunks     199.0 ±45.9µs  [+1.6%–+22.3%]      329M [3]
  ╰─ feed/step, stream pump 64…   186.2 ±55.4µs  [+6.8%–+27.6%]      352M [4]
  qoi-encode/noise/256  40 rounds × 25 calls
  ├─ one-shot encode_qoi         368.0 ±91.3µs  [337.7–398.2]µs     178M [1]
  ├─ Encoder::encode            424.8 ±111.7µs  [-2.2%–+23.8%]      154M [2] [3]
  ├─ push_rows, 16-row strips    412.7 ±96.4µs  [-0.9%–+20.5%]      159M [4] [5]
  ╰─ push_rows, 1-row strips     408.3 ±98.6µs  [-0.6%–+22.5%]      161M [6] [7]
  qoi-transcode/noise/256  40 rounds × 17 calls
  ├─ one-shot decode + encode     594.4 ±134.7µs  [551.0–637.8]µs     110M [1]
  ├─ feed/step -> EncoderSink,…    637.2 ±97.7µs  [-0.8%–+15.3%]      103M [2] [3]
  ╰─ feed/step -> EncoderSink,…   648.0 ±113.5µs  [+0.8%–+17.5%]      101M [4] [5]
  qoi-decode/noise/1024 (ratio 1.00)  40 rounds × 3 calls
  ├─ one-shot decode_qoi          2.6 ±0.5ms  [2.4–2.8]ms        402M [1]
  ├─ feed/step, whole slice       3.3 ±0.9ms  [+15.6%–+40.5%]    315M [2]
  ├─ feed/step, 64 KiB chunks     3.2 ±0.8ms  [+11.8%–+32.8%]    329M [3]
  ╰─ feed/step, stream pump 64…   3.2 ±0.8ms  [+10.5%–+31.0%]    333M [4]
  qoi-encode/noise/1024  40 rounds × 1 calls
  ├─ one-shot encode_qoi        6.7 ±1.1ms  [6.3–7.2]ms       156M [1]
  ├─ Encoder::encode            8.0 ±1.8ms  [+3.1%–+21.4%]    132M [2]
  ├─ push_rows, 16-row strips   7.5 ±1.2ms  [+8.9%–+21.6%]    139M
  ╰─ push_rows, 1-row strips    7.7 ±1.0ms  [+8.2%–+24.5%]    137M [3]
  qoi-transcode/noise/1024  40 rounds × 1 calls
  ├─ one-shot decode + encode     10.0 ±2.0ms  [9.4–10.7]ms       104M [1]
  ├─ feed/step -> EncoderSink,…   11.0 ±2.2ms  [-1.5%–+16.1%]    95.4M [2] [3]
  ╰─ feed/step -> EncoderSink,…   10.7 ±1.7ms  [-5.0%–+13.2%]    98.3M [4] [5]
  qoi-decode/noise/4096 (ratio 1.00)  22 rounds × 1 calls
  ├─ one-shot decode_qoi          20.6 ±0.2ms  [19.9–21.6]ms      813M
  ├─ feed/step, whole slice       24.0 ±0.2ms  [+16.5%–+17.1%]    699M
  ├─ feed/step, 64 KiB chunks     24.3 ±0.4ms  [+15.0%–+17.9%]    691M [1]
  ╰─ feed/step, stream pump 64…   24.1 ±0.4ms  [+9.0%–+24.5%]     696M
  qoi-encode/noise/4096  10 rounds × 1 calls
  ├─ one-shot encode_qoi        43.9 ±0.9ms  [43.2–44.7]ms      382M
  ├─ Encoder::encode            51.7 ±1.2ms  [+15.3%–+20.2%]    325M
  ├─ push_rows, 16-row strips   53.6 ±0.9ms  [+19.5%–+24.4%]    313M
  ╰─ push_rows, 1-row strips    55.9 ±3.1ms  [+23.3%–+31.2%]    300M
  qoi-transcode/noise/4096  9 rounds × 1 calls ⚠ only 9 rounds
  ├─ one-shot decode + encode     66.8 ±2.3ms  [65.1–69.1]ms      251M
  ├─ feed/step -> EncoderSink,…   78.4 ±3.3ms  [+12.0%–+22.1%]    214M
  ╰─ feed/step -> EncoderSink,…   78.6 ±0.9ms  [+10.6%–+19.0%]    213M
  qoi-decode/photoish/256 (ratio 0.36)  40 rounds × 37 calls
  ├─ one-shot decode_qoi          298.1 ±73.0µs  [274.3–322.4]µs     220M [1]
  ├─ feed/step, whole slice       299.7 ±92.7µs  [-12.2%–+13.4%]     219M [2] [3]
  ├─ feed/step, 64 KiB chunks     295.1 ±80.3µs  [-16.0%–+5.5%]      222M [4] [5]
  ╰─ feed/step, stream pump 64…   300.3 ±64.6µs  [-11.5%–+13.0%]     218M [6] [7]
  qoi-encode/photoish/256  40 rounds × 19 calls
  ├─ one-shot encode_qoi        536.5 ±115.1µs  [499.3–573.1]µs     122M [1]
  ├─ Encoder::encode            611.0 ±173.5µs  [+3.1%–+25.0%]      107M [2]
  ├─ push_rows, 16-row strips   492.4 ±117.2µs  [-18.2%–-1.9%]      133M [3]
  ╰─ push_rows, 1-row strips    493.8 ±110.4µs  [-17.9%–+2.0%]      133M [4] [5]
  qoi-transcode/photoish/256  40 rounds × 11 calls
  ├─ one-shot decode + encode     1.1 ±0.2ms  [1.0–1.1]ms       61.3M
  ├─ feed/step -> EncoderSink,…   1.0 ±0.3ms  [-17.5%–-2.2%]    67.9M [1]
  ╰─ feed/step -> EncoderSink,…   1.0 ±0.2ms  [-15.0%–-2.4%]    67.3M [2]
  qoi-decode/photoish/1024 (ratio 0.37)  40 rounds × 1 calls
  ├─ one-shot decode_qoi          7.8 ±1.0ms  [7.3–8.3]ms       135M [1]
  ├─ feed/step, whole slice       7.8 ±0.9ms  [-9.0%–+4.2%]     135M [2] [3]
  ├─ feed/step, 64 KiB chunks     7.9 ±1.0ms  [-1.6%–+11.3%]    132M [4] [5]
  ╰─ feed/step, stream pump 64…   7.9 ±1.6ms  [-2.6%–+10.6%]    132M [6] [7]
  qoi-encode/photoish/1024  40 rounds × 1 calls
  ├─ one-shot encode_qoi         9.8 ±1.9ms  [9.2–10.4]ms       107M [1]
  ├─ Encoder::encode            11.2 ±2.9ms  [+6.0%–+23.1%]    94.0M [2]
  ├─ push_rows, 16-row strips   10.0 ±1.9ms  [-4.5%–+9.8%]      104M [3]
  ╰─ push_rows, 1-row strips    10.0 ±1.3ms  [-4.6%–+10.6%]     104M [4]
  qoi-transcode/photoish/1024  40 rounds × 1 calls
  ├─ one-shot decode + encode     15.8 ±3.8ms  [14.5–17.0]ms     66.6M [1]
  ├─ feed/step -> EncoderSink,…   14.3 ±3.7ms  [-14.0%–+5.3%]    73.5M [2] [3]
  ╰─ feed/step -> EncoderSink,…   14.6 ±2.6ms  [-17.5%–+1.8%]    71.9M [4] [5]
  qoi-decode/photoish/4096 (ratio 0.37)  9 rounds × 1 calls ⚠ only 9 rounds
  ├─ one-shot decode_qoi          55.7 ±0.9ms  [54.9–56.6]ms    301M
  ├─ feed/step, whole slice       56.3 ±0.9ms  [+1.0%–+1.7%]    298M [1]
  ├─ feed/step, 64 KiB chunks     57.0 ±2.7ms  [+0.8%–+1.6%]    295M [2]
  ╰─ feed/step, stream pump 64…   56.4 ±2.5ms  [+0.8%–+1.3%]    297M [3]
  qoi-encode/photoish/4096  7 rounds × 1 calls ⚠ only 7 rounds
  ├─ one-shot encode_qoi        75.4 ±0.5ms  [74.7–76.4]ms      223M
  ├─ Encoder::encode            85.1 ±1.2ms  [+10.4%–+15.7%]    197M [1]
```

## Reading

- Decode through feed/step costs 1 to 17% over the one-shot decoder: 1%
  on photo-like data, 8% on the pattern, 17% on noise. Chunking (64 KiB,
  stream pump) costs nothing extra over feeding the whole file. The gap is
  the boundary-safe `decode_chunk` kernel the incremental decoder needs,
  against the one-shot `decode_range`; it is largest when every pixel is a
  5-byte opcode.
- `Encoder::encode` itself is 10 to 20% slower than `encode_qoi` on every
  image, before any streaming: that is the existing dead-alpha narrowing
  scan in the zencodec encode path. Streaming `push_rows` lands within a
  few percent of `Encoder::encode`, faster on photo-like data, slower on
  noise; 1-row and 16-row strips cost about the same.
- A decode-to-encode transcode with no intermediate image costs 2 to 17%
  over decoding into a full buffer and re-encoding it.

## Fixed during this work

- The first run had `zencodec-io`'s `Feeder` drain the front of a carried
  remainder on every re-offer: feeding a whole 64 MiB file to a decoder
  with a 1 MiB input cap made "feed/step, whole slice" 2.8x slower than
  one-shot on noise/4096 (56.1 ms vs 20.5 ms). Head-index carry fixed it
  (24.0 ms). zencodec `51a40ccc`.
- `push_rows` zero-filled worst-case output headroom on every strip; it
  now encodes into one reused scratch buffer.
