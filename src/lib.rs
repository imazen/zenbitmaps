//! # zenpnm
//!
//! PNM/PAM/PFM and BMP image format decoder and encoder.
//!
//! ## Zero-Copy Decoding
//!
//! For PNM files with maxval=255 (the common case), decoding returns a borrowed
//! slice into the input buffer — no allocation or copy needed. Formats that
//! require transformation (BMP row flipping, 16-bit downscaling, PFM byte
//! reordering) allocate as needed.
//!
//! ## Supported Formats
//!
//! ### PNM family (`pnm` feature)
//! - **P5** (PGM binary) — grayscale, 8-bit and 16-bit
//! - **P6** (PPM binary) — RGB, 8-bit and 16-bit
//! - **P7** (PAM) — arbitrary channels (grayscale, RGB, RGBA, etc.), 8-bit and 16-bit
//! - **PFM** — floating-point grayscale and RGB (32-bit float per channel)
//!
//! ### BMP (`bmp` feature)
//! - Decode and encode of uncompressed BMP (24-bit RGB, 32-bit RGBA)
//!
//! ## Non-Goals
//!
//! - ASCII PNM formats (P1, P2, P3) — too slow for any real use
//! - Animated formats
//! - Color management (use zencodecs for that)
//!
//! ## Credits
//!
//! The PNM implementation draws heavily from [zune-ppm](https://github.com/etemesi254/zune-image)
//! by Caleb Etemesi (MIT/Apache-2.0/Zlib licensed). We credit that work and recommend it
//! if you need a PNM decoder integrated with the zune-image ecosystem.
//!
//! ## Usage
//!
//! ```no_run
//! use zenpnm::{DecodeRequest, EncodeRequest, ImageInfo};
//! use enough::Unstoppable;
//!
//! let data: &[u8] = &[]; // your PNM/BMP bytes
//!
//! // Probe without decoding
//! let info = ImageInfo::from_bytes(data).unwrap();
//! println!("{}x{} {:?}", info.width, info.height, info.format);
//!
//! // Decode (zero-copy when possible)
//! let decoded = DecodeRequest::new(data)
//!     .decode(Unstoppable)?;
//! // decoded.pixels is Cow::Borrowed when no transformation needed
//!
//! // Encode to PPM
//! # #[cfg(feature = "pnm")]
//! # {
//! # use zenpnm::{PixelLayout, pnm::PnmFormat};
//! let encoded = EncodeRequest::pnm(PnmFormat::Ppm)
//!     .encode(decoded.pixels(), decoded.width, decoded.height,
//!             decoded.layout, Unstoppable)?;
//! # }
//! # Ok::<(), zenpnm::PnmError>(())
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod error;
mod info;
mod limits;
mod pixel;

#[cfg(feature = "pnm")]
pub mod pnm;

#[cfg(feature = "bmp")]
pub mod bmp;

mod decode;
mod encode;

// Re-exports
pub use decode::{DecodeOutput, DecodeRequest};
pub use encode::EncodeRequest;
pub use enough::{Stop, Unstoppable};
pub use error::PnmError;
pub use info::{BitmapFormat, ImageInfo};
pub use limits::Limits;
pub use pixel::PixelLayout;
