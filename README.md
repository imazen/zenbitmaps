# zenbitmaps

[![Crates.io](https://img.shields.io/crates/v/zenbitmaps.svg)](https://crates.io/crates/zenbitmaps)
[![docs.rs](https://docs.rs/zenbitmaps/badge.svg)](https://docs.rs/zenbitmaps)
[![License](https://img.shields.io/crates/l/zenbitmaps.svg)](LICENSE)

PNM/PAM/PFM, BMP, and farbfeld image format decoder and encoder.

`no_std` compatible (with `alloc`), `forbid(unsafe_code)`, panic-free. All arithmetic is checked. Suitable for server and embedded use.

## Getting started

```toml
[dependencies]
zenbitmaps = "0.1"                                         # PNM + farbfeld
zenbitmaps = { version = "0.1", features = ["bmp"] }       # + full BMP support
zenbitmaps = { version = "0.1", features = ["rgb"] }       # + typed pixel API (RGB8, RGBA8, etc.)
zenbitmaps = { version = "0.1", features = ["imgref"] }    # + ImgVec/ImgRef 2D buffers (implies rgb)
zenbitmaps = { version = "0.1", features = ["all"] }       # everything
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
    None => { /* unknown */ }
    _ => { /* future formats */ }
}
```

`decode()` uses this internally. You only need `detect_format()` if you want to inspect the format before committing to a full decode, or if you need to route data to format-specific functions.

## Supported formats

**PNM family** (always available):
- P5 (PGM binary) — grayscale, 8-bit and 16-bit
- P6 (PPM binary) — RGB, 8-bit and 16-bit
- P7 (PAM) — arbitrary channels (grayscale, RGB, RGBA), 8-bit and 16-bit
- PFM — floating-point grayscale and RGB (32-bit per channel)
- Auto-detected by `decode()` via `P5`/`P6`/`P7`/`Pf`/`PF` magic

**Farbfeld** (always available):
- RGBA 16-bit per channel
- Auto-detected by `decode()` via `"farbfeld"` magic

**BMP** (`bmp` feature, opt-in):
- All standard bit depths: 1, 2, 4, 8, 16, 24, 32
- Compression: uncompressed, RLE4, RLE8, BITFIELDS
- Palette expansion, bottom-up/top-down, grayscale detection
- `BmpPermissiveness` levels: Strict, Standard (default), Permissive
- Native byte order decoding via `decode_bmp_native()` (skips BGR→RGB swizzle)
- Auto-detected by `decode()` via `"BM"` magic

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
| `bmp` | Full BMP decode/encode (all bit depths, RLE, bitfields) |
| `rgb` | Typed pixel API (`RGB8`, `RGBA8`, `as_pixels()`, `encode_*_pixels()`) |
| `imgref` | 2D buffer API (`ImgVec`/`ImgRef`, `as_imgref()`, `decode_into()`) — implies `rgb` |
| `all` | `bmp` + `rgb` + `imgref` |

## API

All public functions are flat, one-shot calls at crate root.

**Decode (auto-detect):**
- `detect_format(data)` — identify format from magic bytes
- `decode(data, stop)` — auto-detect and decode
- `decode_with_limits(data, limits, stop)`

**Decode (format-specific):**
- `decode_farbfeld(data, stop)` / `decode_farbfeld_with_limits(...)`
- `decode_bmp(data, stop)` / `decode_bmp_with_limits(...)` — RGB output (`bmp`)
- `decode_bmp_native(data, stop)` / `decode_bmp_native_with_limits(...)` — BGR output (`bmp`)
- `decode_bmp_permissive(data, permissiveness, stop)` / `..._with_limits(...)` (`bmp`)

**Encode (raw bytes):**
- `encode_ppm`, `encode_pgm`, `encode_pam`, `encode_pfm` — PNM family
- `encode_farbfeld` — farbfeld
- `encode_bmp`, `encode_bmp_rgba` — BMP (`bmp`)

**Typed pixel** (`rgb`): `decode_pixels`, `encode_ppm_pixels`, `encode_pam_pixels`, etc.

**ImgRef/ImgVec** (`imgref`): `decode_img`, `decode_into`, `encode_ppm_img`, etc.

**Types:**
- `DecodeOutput<'a>` — decoded image (`.pixels()`, `.width`, `.height`, `.layout`, `.is_borrowed()`, `.as_pixels()`, `.as_imgref()`, `.to_imgvec()`)
- `ImageFormat` — format enum (Pnm, Bmp, Farbfeld)
- `PixelLayout` — pixel format (Gray8, Gray16, Rgb8, Rgba8, Rgba16, Bgr8, Bgra8, Bgrx8, GrayF32, RgbF32)
- `BmpPermissiveness` — decode strictness (Strict, Standard, Permissive) (`bmp`)
- `Limits` — resource limits (max width/height/pixels/memory)
- `BitmapError` — error type, `#[non_exhaustive]`

## Credits

- PNM: draws from [zune-ppm](https://github.com/etemesi254/zune-image) by Caleb Etemesi (MIT/Apache-2.0/Zlib)
- BMP: forked from [zune-bmp](https://github.com/etemesi254/zune-image) 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib)
- Farbfeld: forked from [zune-farbfeld](https://github.com/etemesi254/zune-image) 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib)

## AI-Generated Code Notice

Developed with Claude (Anthropic). Not all code manually reviewed. Review critical paths before production use.

## License

MIT OR Apache-2.0
