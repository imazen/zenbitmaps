# zenbitmaps [![CI](https://img.shields.io/github/actions/workflow/status/imazen/zenbitmaps/ci.yml?style=flat-square)](https://github.com/imazen/zenbitmaps/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/zenbitmaps?style=flat-square)](https://crates.io/crates/zenbitmaps) [![lib.rs](https://img.shields.io/crates/v/zenbitmaps?style=flat-square&label=lib.rs&color=blue)](https://lib.rs/crates/zenbitmaps) [![docs.rs](https://img.shields.io/docsrs/zenbitmaps?style=flat-square)](https://docs.rs/zenbitmaps) [![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=flat-square)](https://github.com/imazen/zenbitmaps#license)

Fast, safe bitmap format codecs. `no_std` + `alloc`, `forbid(unsafe_code)`, panic-free.

| Format | Feature | Decode | Encode | Detection |
|--------|---------|--------|--------|-----------|
| **PNM** (PGM/PPM/PAM/PFM) | *(default)* | 1327 GiB/s | 10.5 GiB/s | `P5`/`P6`/`P7`/`Pf`/`PF` magic |
| **Farbfeld** | *(default)* | 3.04 GiB/s | 1.50 GiB/s | `farbfeld` magic |
| **TGA** | `tga` | 4.85 GiB/s | 5.88 GiB/s | Header heuristic + v2 footer |
| **BMP** | `bmp` | 4.66 GiB/s | 1.23 GiB/s | `BM` magic |
| **QOI** | `qoi` | 1.32 GiB/s | 870 MiB/s | `qoif` magic |
| **Radiance HDR** | `hdr` | 1.05 GiB/s | 361 MiB/s | `#?RADIANCE` / `#?RGBE` |

<sub>1 megapixel RGB8, single-threaded. PPM decode is zero-copy. See [benchmarks](#performance).</sub>

## Getting started

```toml
[dependencies]
zenbitmaps = "0.2"                                         # PNM + farbfeld
zenbitmaps = { version = "0.2", features = ["bmp"] }       # + BMP
zenbitmaps = { version = "0.2", features = ["qoi"] }       # + QOI (via rapid-qoi)
zenbitmaps = { version = "0.2", features = ["tga"] }       # + TGA
zenbitmaps = { version = "0.2", features = ["hdr"] }       # + Radiance HDR
zenbitmaps = { version = "0.2", features = ["all"] }       # everything
```

## Quick example

```rust
use zenbitmaps::*;
use enough::Unstoppable;

// Encode pixels to PPM
let pixels = vec![255u8, 0, 0, 0, 255, 0]; // 2 RGB pixels
let encoded = encode_ppm(&pixels, 2, 1, PixelLayout::Rgb8, Unstoppable)?;

// Decode (auto-detects PNM/BMP/farbfeld from magic bytes)
let decoded = decode(&encoded, Unstoppable)?;
assert!(decoded.is_borrowed()); // zero-copy for PPM with maxval=255
assert_eq!(decoded.pixels(), &pixels[..]);
# Ok::<(), BitmapError>(())
```

## Format detection

`detect_format()` identifies the format from magic bytes without decoding:

```rust
use zenbitmaps::*;

match detect_format(&data) {
    Some(ImageFormat::Pnm) => { /* PGM, PPM, PAM, or PFM */ }
    Some(ImageFormat::Bmp) => { /* Windows bitmap */ }
    Some(ImageFormat::Farbfeld) => { /* farbfeld RGBA16 */ }
    Some(ImageFormat::Qoi) => { /* QOI */ }
    Some(ImageFormat::Hdr) => { /* Radiance HDR */ }
    Some(ImageFormat::Tga) => { /* TGA (Targa) */ }
    None => { /* unknown */ }
    _ => { /* future formats */ }
}
```

`decode()` uses this internally and dispatches to the right codec.

## Supported formats

**PNM family** (always available):
- P5 (PGM binary) — grayscale, 8-bit and 16-bit
- P6 (PPM binary) — RGB, 8-bit and 16-bit
- P7 (PAM) — arbitrary channels (grayscale, RGB, RGBA), 8-bit and 16-bit
- PFM — floating-point grayscale and RGB (32-bit per channel)
- Magic: `P5`/`P6`/`P7`/`Pf`/`PF`

**Farbfeld** (always available):
- RGBA 16-bit per channel, big-endian
- Magic: `farbfeld`

**BMP** (`bmp` feature):
- All standard bit depths: 1, 2, 4, 8, 16, 24, 32
- Compression: uncompressed, RLE4, RLE8, BITFIELDS
- Palette expansion, bottom-up/top-down, grayscale detection
- `BmpPermissiveness` levels: Strict, Standard (default), Permissive
- Native byte order decoding via `decode_bmp_native()` (skips BGR→RGB swizzle)
- Magic: `BM`

**QOI** (`qoi` feature, via [rapid-qoi](https://github.com/zakarumych/rapid-qoi)):
- RGB8 and RGBA8, lossless
- Row-level streaming decode via `decode_range`
- Streaming encode via `push_rows`/`finish`
- Magic: `qoif`

**TGA** (`tga` feature):
- Uncompressed and RLE-compressed (types 1-3, 9-11)
- True color (15/16/24/32-bit), grayscale, color-mapped
- All image origins (top/bottom, left/right)
- Fast path: memcpy + SIMD batch BGR→RGB swizzle for 24/32-bit
- Detection: header heuristic (TGA has no magic bytes)

**Radiance HDR** (`hdr` feature):
- RGBE format with new-style per-channel RLE
- Decodes to `RgbF32` (linear float)
- RGBE↔f32 via IEEE 754 bit manipulation (no libm, no unsafe)
- Encodes from `RgbF32` or `Rgb8`
- Magic: `#?RADIANCE` / `#?RGBE`

## Zero-copy decoding

PNM files with maxval=255 (the common case) decode to a borrowed slice into your input buffer. No allocation, no copy. Formats requiring transformation (BMP row flip, farbfeld endian swap, 16-bit, non-255 maxval, PFM) allocate.

With the `rgb` feature, `as_pixels()` gives you a zero-copy typed view:

```rust
let decoded = decode(&data, Unstoppable)?;
let pixels: &[RGB8] = decoded.as_pixels()?; // zero-copy reinterpret
```

With the `imgref` feature, `as_imgref()` gives you a zero-copy 2D view:

```rust
let decoded = decode(&data, Unstoppable)?;
let img: imgref::ImgRef<'_, RGB8> = decoded.as_imgref()?; // zero-copy 2D view
```

`to_imgvec()` is also available when you need an owned copy.

## BGRA pipeline

BMP files store pixels in BGR/BGRA order. Use `decode_bmp_native()` to skip the BGR→RGB swizzle and work directly in native byte order:

```rust
let decoded = decode_bmp_native(&bmp_data, Unstoppable)?;
// decoded.layout is Bgr8, Bgra8, or Gray8

// All encoders accept BGR/BGRA input — swizzle happens automatically
let pam = encode_pam(decoded.pixels(), decoded.width, decoded.height,
                     decoded.layout, Unstoppable)?;
```

PGM, PPM, PAM, farbfeld, and BMP encoders all accept Bgr8, Bgra8, and Bgrx8 input.

## Typed pixel API (`rgb` feature)

With the `rgb` feature, you get type-safe pixel encode/decode using the `rgb` crate's types:

```rust
use zenbitmaps::*;
use enough::Unstoppable;

let pixels = vec![RGB8 { r: 255, g: 0, b: 0 }, RGB8 { r: 0, g: 255, b: 0 }];
let encoded = encode_ppm_pixels(&pixels, 2, 1, Unstoppable)?;
let (decoded, w, h) = decode_pixels::<RGB8>(&encoded, Unstoppable)?;
# Ok::<(), BitmapError>(())
```

Available types: `RGB8`, `RGBA8`, `BGR8`, `BGRA8` (type aliases for `rgb` crate types).

## ImgRef/ImgVec API (`imgref` feature)

With the `imgref` feature (implies `rgb`), you can work with 2D image buffers that handle stride/padding:

```rust
let img = imgref::ImgVec::new(pixels, width, height);
let encoded = encode_ppm_img(img.as_ref(), Unstoppable)?;
let decoded_img = decode_img::<RGB8>(&encoded, Unstoppable)?;
```

`decode_into()` decodes directly into a pre-allocated `ImgRefMut` buffer, handling arbitrary stride.

## Cooperative cancellation

Every function takes a `stop` parameter implementing `enough::Stop`. Pass `Unstoppable` when you don't need cancellation. For server use, pass a token that checks a shutdown flag — decode/encode will bail out promptly via `BitmapError::Cancelled`.

## Resource limits

```rust
use zenbitmaps::*;
use enough::Unstoppable;

let limits = Limits {
    max_width: Some(4096),
    max_height: Some(4096),
    max_pixels: Some(16_000_000),
    max_memory_bytes: Some(64 * 1024 * 1024),
    ..Default::default()
};
# let data = encode_ppm(&[0u8; 3], 1, 1, PixelLayout::Rgb8, Unstoppable).unwrap();
let decoded = decode_with_limits(&data, &limits, Unstoppable)?;
# Ok::<(), BitmapError>(())
```

## Features

| Feature | What it adds |
|---------|-------------|
| *(default)* | PNM (P5/P6/P7/PFM) + farbfeld decode/encode |
| `bmp` | BMP decode/encode (all bit depths, RLE, bitfields, palettes) |
| `qoi` | QOI decode/encode via rapid-qoi (streaming, lossless) |
| `tga` | TGA decode/encode (truecolor, grayscale, color-mapped, RLE) |
| `hdr` | Radiance HDR decode/encode (RGBE, RLE, f32 output) |
| `simd` | SIMD-accelerated BGR↔RGB swizzle via [garb](https://lib.rs/crates/garb) |
| `rgb` | Typed pixel API (`RGB8`, `RGBA8`, `as_pixels()`, `encode_*_pixels()`) |
| `imgref` | 2D buffer API (`ImgVec`/`ImgRef`, `as_imgref()`, `decode_into()`) — implies `rgb` |
| `zencodec` | zencodec trait integration (implies `rgb` + `imgref`) |
| `std` | Enable `std` support (not required — `no_std` + `alloc` by default) |
| `all` | All format + pixel API features |

## API

All public functions are flat, one-shot calls at crate root.

**Decode (auto-detect):**
- `detect_format(data)` — identify format from magic bytes
- `decode(data, stop)` — auto-detect and decode
- `decode_with_limits(data, limits, stop)`

**Decode (format-specific):**
- `decode_farbfeld` / `decode_farbfeld_with_limits`
- `decode_bmp` / `decode_bmp_with_limits` — RGB output (`bmp`)
- `decode_bmp_native` / `decode_bmp_native_with_limits` — BGR output (`bmp`)
- `decode_bmp_permissive` / `..._with_limits` (`bmp`)
- `decode_qoi` / `decode_qoi_with_limits` (`qoi`)
- `decode_tga` / `decode_tga_with_limits` (`tga`)
- `decode_hdr` / `decode_hdr_with_limits` (`hdr`)

**Encode (raw bytes):**
- `encode_ppm`, `encode_pgm`, `encode_pam`, `encode_pfm` — PNM family
- `encode_farbfeld` — farbfeld
- `encode_bmp`, `encode_bmp_rgba` — BMP (`bmp`)
- `encode_qoi` — QOI (`qoi`)
- `encode_tga` — TGA (`tga`)
- `encode_hdr` — Radiance HDR (`hdr`)

**Typed pixel** (`rgb`): `decode_pixels`, `encode_ppm_pixels`, `encode_pam_pixels`, etc.

**ImgRef/ImgVec** (`imgref`): `decode_img`, `decode_into`, `encode_ppm_img`, etc.

**Types:**
- `DecodeOutput<'a>` — decoded image (`.pixels()`, `.width`, `.height`, `.layout`, `.is_borrowed()`, `.as_pixels()`, `.as_imgref()`, `.to_imgvec()`)
- `ImageFormat` — format enum (Pnm, Bmp, Farbfeld, Qoi, Tga, Hdr)
- `PixelLayout` — pixel format (Gray8, Gray16, Rgb8, Rgba8, Rgba16, Bgr8, Bgra8, Bgrx8, GrayF32, RgbF32)
- `BmpPermissiveness` — decode strictness (Strict, Standard, Permissive) (`bmp`)
- `Limits` — resource limits (max width/height/pixels/memory)
- `BitmapError` — error type, `#[non_exhaustive]`

## Performance

Run: `cargo bench --bench codecs --all-features`

Benchmarks measured at 1000x1000 pixels, single-threaded, default target (no `-C target-cpu=native`). PPM decode is zero-copy (returns a borrowed slice). TGA decode uses a fast memcpy + batch BGR swizzle path for 24/32-bit uncompressed images. The `simd` feature enables garb's SIMD-accelerated swizzle for TGA and QOI encode.

TGA detection uses header heuristics since TGA has no magic bytes. False positive rate is approximately 1 in 11 million on random data. When the full file is available, the TGA v2 footer (`TRUEVISION-XFILE.\0`) is checked first as a definitive signal.

## Credits

- PNM: draws from [zune-ppm](https://github.com/etemesi254/zune-image) by Caleb Etemesi (MIT/Apache-2.0/Zlib)
- BMP: forked from [zune-bmp](https://github.com/etemesi254/zune-image) 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib)
- Farbfeld: forked from [zune-farbfeld](https://github.com/etemesi254/zune-image) 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib)
- QOI: uses [rapid-qoi](https://github.com/zakarumych/rapid-qoi) by Zakarum (MIT/Apache-2.0)
- TGA, HDR: from-scratch implementations, no external dependencies

## AI-Generated Code Notice

Developed with Claude (Anthropic). Not all code manually reviewed. Review critical paths before production use.

## Image tech I maintain

| | |
|:--|:--|
| State of the art codecs* | [zenjpeg] · [zenpng] · [zenwebp] · [zengif] · [zenavif] ([rav1d-safe] · [zenrav1e] · [zenavif-parse] · [zenavif-serialize]) · [zenjxl] ([jxl-encoder] · [zenjxl-decoder]) · [zentiff] · **zenbitmaps** · [heic] · [zenraw] · [zenpdf] · [ultrahdr] · [mozjpeg-rs] · [webpx] |
| Compression | [zenflate] · [zenzop] |
| Processing | [zenresize] · [zenfilters] · [zenquant] · [zenblend] |
| Metrics | [zensim] · [fast-ssim2] · [butteraugli] · [resamplescope-rs] · [codec-eval] · [codec-corpus] |
| Pixel types & color | [zenpixels] · [zenpixels-convert] · [linear-srgb] · [garb] |
| Pipeline | [zenpipe] · [zencodec] · [zencodecs] · [zenlayout] · [zennode] |
| ImageResizer | [ImageResizer] (C#) — 24M+ NuGet downloads across all packages |
| [Imageflow][] | Image optimization engine (Rust) — [.NET][imageflow-dotnet] · [node][imageflow-node] · [go][imageflow-go] — 9M+ NuGet downloads across all packages |
| [Imageflow Server][] | [The fast, safe image server](https://www.imazen.io/) (Rust+C#) — 552K+ NuGet downloads, deployed by Fortune 500s and major brands |

<sub>* as of 2026</sub>

### General Rust awesomeness

[archmage] · [magetypes] · [enough] · [whereat] · [zenbench] · [cargo-copter]

[And other projects](https://www.imazen.io/open-source) · [GitHub @imazen](https://github.com/imazen) · [GitHub @lilith](https://github.com/lilith) · [lib.rs/~lilith](https://lib.rs/~lilith) · [NuGet](https://www.nuget.org/profiles/imazen) (over 30 million downloads / 87 packages)

## License

MIT OR Apache-2.0

[zenjpeg]: https://github.com/imazen/zenjpeg
[zenpng]: https://github.com/imazen/zenpng
[zenwebp]: https://github.com/imazen/zenwebp
[zengif]: https://github.com/imazen/zengif
[zenavif]: https://github.com/imazen/zenavif
[zenjxl]: https://github.com/imazen/zenjxl
[zentiff]: https://github.com/imazen/zentiff
[heic]: https://github.com/imazen/heic-decoder-rs
[zenraw]: https://github.com/imazen/zenraw
[zenpdf]: https://github.com/imazen/zenpdf
[ultrahdr]: https://github.com/imazen/ultrahdr
[jxl-encoder]: https://github.com/imazen/jxl-encoder
[zenjxl-decoder]: https://github.com/imazen/zenjxl-decoder
[rav1d-safe]: https://github.com/imazen/rav1d-safe
[zenrav1e]: https://github.com/imazen/zenrav1e
[mozjpeg-rs]: https://github.com/imazen/mozjpeg-rs
[zenavif-parse]: https://github.com/imazen/zenavif-parse
[zenavif-serialize]: https://github.com/imazen/zenavif-serialize
[webpx]: https://github.com/imazen/webpx
[zenflate]: https://github.com/imazen/zenflate
[zenzop]: https://github.com/imazen/zenzop
[zenresize]: https://github.com/imazen/zenresize
[zenfilters]: https://github.com/imazen/zenfilters
[zenquant]: https://github.com/imazen/zenquant
[zenblend]: https://github.com/imazen/zenblend
[zensim]: https://github.com/imazen/zensim
[fast-ssim2]: https://github.com/imazen/fast-ssim2
[butteraugli]: https://github.com/imazen/butteraugli
[zenpixels]: https://github.com/imazen/zenpixels
[zenpixels-convert]: https://github.com/imazen/zenpixels
[linear-srgb]: https://github.com/imazen/linear-srgb
[garb]: https://github.com/imazen/garb
[zenpipe]: https://github.com/imazen/zenpipe
[zencodec]: https://github.com/imazen/zencodec
[zencodecs]: https://github.com/imazen/zencodecs
[zenlayout]: https://github.com/imazen/zenlayout
[zennode]: https://github.com/imazen/zennode
[Imageflow]: https://github.com/imazen/imageflow
[Imageflow Server]: https://github.com/imazen/imageflow-server
[imageflow-dotnet]: https://github.com/imazen/imageflow-dotnet
[imageflow-node]: https://github.com/imazen/imageflow-node
[imageflow-go]: https://github.com/imazen/imageflow-go
[ImageResizer]: https://github.com/imazen/resizer
[archmage]: https://github.com/imazen/archmage
[magetypes]: https://github.com/imazen/archmage
[enough]: https://github.com/imazen/enough
[whereat]: https://github.com/lilith/whereat
[zenbench]: https://github.com/imazen/zenbench
[cargo-copter]: https://github.com/imazen/cargo-copter
[resamplescope-rs]: https://github.com/imazen/resamplescope-rs
[codec-eval]: https://github.com/imazen/codec-eval
[codec-corpus]: https://github.com/imazen/codec-corpus
