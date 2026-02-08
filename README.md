# zenpnm

PNM/PAM/PFM image format decoder and encoder, with optional basic BMP support.

`no_std` compatible (with `alloc`), `forbid(unsafe_code)`, panic-free. All arithmetic is checked. Suitable for server and embedded use.

## Formats

**PNM family** (always available):
- P5 (PGM binary) — grayscale, 8-bit and 16-bit
- P6 (PPM binary) — RGB, 8-bit and 16-bit
- P7 (PAM) — grayscale, RGB, RGBA, 8-bit and 16-bit
- PFM — floating-point grayscale and RGB (32-bit per channel)

**Basic BMP** (`basic-bmp` feature, opt-in):
- Uncompressed 24-bit RGB and 32-bit RGBA only
- Not auto-detected — you call `decode_bmp` / `encode_bmp` explicitly
- No RLE, indexed color, or advanced headers

## Zero-copy decoding

PNM files with maxval=255 (the common case) decode to a borrowed slice into your input buffer. No allocation, no copy. Formats requiring transformation (16-bit, non-255 maxval, PFM, BMP) allocate.

```rust
use zenpnm::*;
use enough::Unstoppable;

let pixels = vec![255u8, 0, 0, 0, 255, 0]; // 2 RGB pixels
let encoded = encode_ppm(&pixels, 2, 1, PixelLayout::Rgb8, Unstoppable)?;

let decoded = decode(&encoded, Unstoppable)?;
assert!(decoded.is_borrowed()); // zero-copy
assert_eq!(decoded.pixels(), &pixels[..]);
# Ok::<(), PnmError>(())
```

## Cooperative cancellation

Every function takes a `stop` parameter implementing `enough::Stop`. Pass `Unstoppable` when you don't need cancellation. For server use, pass a token that checks a shutdown flag — decode/encode will bail out promptly via `PnmError::Cancelled`.

## Resource limits

```rust
use zenpnm::*;
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
# Ok::<(), PnmError>(())
```

## API

All public functions are flat, one-shot calls at crate root.

**Decode:**
- `decode(data, stop)` — auto-detect PNM format from magic bytes
- `decode_with_limits(data, limits, stop)` — same, with resource limits
- `decode_bmp(data, stop)` — explicit BMP decode (requires `basic-bmp` feature)
- `decode_bmp_with_limits(data, limits, stop)`

**Encode:**
- `encode_ppm(pixels, w, h, layout, stop)` — P6 binary RGB
- `encode_pgm(pixels, w, h, layout, stop)` — P5 binary grayscale
- `encode_pam(pixels, w, h, layout, stop)` — P7, any supported layout
- `encode_pfm(pixels, w, h, layout, stop)` — PFM floating-point
- `encode_bmp(pixels, w, h, layout, stop)` — 24-bit BMP (requires `basic-bmp`)
- `encode_bmp_rgba(pixels, w, h, layout, stop)` — 32-bit BMP with alpha

**Types:**
- `DecodeOutput<'a>` — decoded image with `.pixels()`, `.width`, `.height`, `.layout`, `.is_borrowed()`, `.into_owned()`
- `PixelLayout` — pixel format enum (Gray8, Gray16, Rgb8, Rgba8, Bgr8, Bgra8, GrayF32, RgbF32)
- `Limits` — resource limits (max width/height/pixels/memory)
- `PnmError` — error type, `#[non_exhaustive]`

## Features

```toml
[dependencies]
zenpnm = "0.1"                    # PNM (always included)
zenpnm = { version = "0.1", features = ["basic-bmp"] }  # + BMP
zenpnm = { version = "0.1", features = ["rgb"] }         # + typed pixel API
zenpnm = { version = "0.1", features = ["imgref"] }      # + ImgVec/ImgRef (implies rgb)
zenpnm = { version = "0.1", features = ["all"] }          # everything
```

## Credits

PNM implementation draws from [zune-ppm](https://github.com/etemesi254/zune-image) by Caleb Etemesi (MIT/Apache-2.0/Zlib licensed).

## AI-Generated Code Notice

Developed with Claude (Anthropic). Not all code manually reviewed. Review critical paths before production use.

## License

MIT OR Apache-2.0
