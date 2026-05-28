//! Vendored QOI codec core (descriptor, pixel traits, color space).
//!
//! Vendored from rapid-qoi v0.6.x (<https://github.com/zakarumych/rapid-qoi>),
//! © zakarumych, licensed MIT OR Apache-2.0. zenbitmaps ships LICENSE-MIT and
//! LICENSE-APACHE covering this code. Adapted from the crate root `lib.rs` into
//! a submodule of zenbitmaps: crate-level inner attributes (`no_std` toggling,
//! crate docs, `extern crate alloc`) were dropped because zenbitmaps already
//! configures `std`/`alloc` and `#![forbid(unsafe_code)]` at the crate root.
//! The `bytemuck`-based slice casts in `decode.rs`/`encode.rs` were replaced
//! with safe `chunks_exact` conversions to avoid a new direct dependency; the
//! decode `QOI_OP_RUN` arm carries the upstream clamp fix (see `decode.rs`).
//! No algorithmic changes — pixel output is byte-identical to upstream.
//!
//! QOI is "The Quite OK Image" format (<https://qoiformat.org>): a fast,
//! lossless RGB/RGBA codec. A QOI file is a 14-byte header, a stream of
//! 2-/8-bit-tagged chunks (`QOI_OP_INDEX`, `QOI_OP_DIFF`, `QOI_OP_LUMA`,
//! `QOI_OP_RUN`, `QOI_OP_RGB`, `QOI_OP_RGBA`), and an 8-byte end marker.

use core::{
    convert::TryInto,
    fmt::{self, Display},
};

mod decode;
mod encode;

// `DecodeError` / `EncodeError` are defined in the submodules and flow out of
// zenbitmaps as `Display`/`Debug` strings, so they are not re-exported here.

const QOI_OP_INDEX: u8 = 0x00; /* 00xxxxxx */
const QOI_OP_DIFF: u8 = 0x40; /* 01xxxxxx */
const QOI_OP_LUMA: u8 = 0x80; /* 10xxxxxx */
const QOI_OP_RUN: u8 = 0xc0; /* 11xxxxxx */
const QOI_OP_RGB: u8 = 0xfe; /* 11111110 */
const QOI_OP_RGBA: u8 = 0xff; /* 11111111 */

const QOI_MAGIC: u32 = u32::from_be_bytes(*b"qoif");
const QOI_HEADER_SIZE: usize = 14;
const QOI_PADDING: usize = 8;

/// Trait for pixel types.
/// Supports byte operations, channels accessing and modifying.
///
/// Vendored faithfully from upstream; zenbitmaps only exercises a subset of the
/// accessor/mutator methods, so the rest are allowed to be unused.
#[allow(dead_code)]
pub(crate) trait Pixel: Copy + Eq {
    const HAS_ALPHA: bool;

    fn new() -> Self;

    fn new_opaque() -> Self;

    fn read(&mut self, bytes: &[u8]);

    fn write(&self, bytes: &mut [u8]);

    fn var(&self, prev: &Self) -> Var;

    fn rgb(&self) -> [u8; 3];

    fn rgba(&self) -> [u8; 4];

    fn r(&self) -> u8;

    fn g(&self) -> u8;

    fn b(&self) -> u8;

    fn a(&self) -> u8;

    fn set_r(&mut self, r: u8);

    fn set_g(&mut self, g: u8);

    fn set_b(&mut self, b: u8);

    fn set_a(&mut self, a: u8);

    fn set_rgb(&mut self, r: u8, g: u8, b: u8);

    fn set_rgba(&mut self, r: u8, g: u8, b: u8, a: u8);

    fn add_rgb(&mut self, r: u8, g: u8, b: u8);

    fn hash(&self) -> u8;
}

impl Pixel for [u8; 3] {
    const HAS_ALPHA: bool = false;

    #[inline]
    fn new() -> Self {
        [0; 3]
    }

    #[inline]
    fn new_opaque() -> Self {
        [0; 3]
    }

    #[inline]
    fn read(&mut self, bytes: &[u8]) {
        self.copy_from_slice(bytes);
    }

    #[inline]
    fn write(&self, bytes: &mut [u8]) {
        assert_eq!(bytes.len(), self.len());
        bytes.copy_from_slice(self)
    }

    #[inline]
    fn var(&self, prev: &Self) -> Var {
        let r = self[0].wrapping_sub(prev[0]);
        let g = self[1].wrapping_sub(prev[1]);
        let b = self[2].wrapping_sub(prev[2]);

        Var { r, g, b }
    }

    #[inline]
    fn r(&self) -> u8 {
        self[0]
    }

    #[inline]
    fn g(&self) -> u8 {
        self[1]
    }

    #[inline]
    fn b(&self) -> u8 {
        self[2]
    }

    #[inline]
    fn rgb(&self) -> [u8; 3] {
        *self
    }

    #[inline]
    fn rgba(&self) -> [u8; 4] {
        [self[0], self[1], self[2], 255]
    }

    #[inline]
    fn a(&self) -> u8 {
        255
    }

    #[inline]
    fn set_r(&mut self, r: u8) {
        self[0] = r;
    }

    #[inline]
    fn set_g(&mut self, g: u8) {
        self[1] = g;
    }

    #[inline]
    fn set_b(&mut self, b: u8) {
        self[2] = b;
    }

    #[inline]
    fn set_a(&mut self, a: u8) {
        debug_assert_eq!(a, 255);
    }

    #[inline]
    fn set_rgb(&mut self, r: u8, g: u8, b: u8) {
        self[0] = r;
        self[1] = g;
        self[2] = b;
    }

    #[inline]
    fn set_rgba(&mut self, r: u8, g: u8, b: u8, a: u8) {
        debug_assert_eq!(a, 255);

        self[0] = r;
        self[1] = g;
        self[2] = b;
    }

    #[inline]
    fn add_rgb(&mut self, r: u8, g: u8, b: u8) {
        self[0] = self[0].wrapping_add(r);
        self[1] = self[1].wrapping_add(g);
        self[2] = self[2].wrapping_add(b);
    }

    #[inline]
    fn hash(&self) -> u8 {
        let [r, g, b] = *self;
        let v = u32::from_ne_bytes([r, g, b, 0xff]);
        let s = (((v as u64) << 32) | (v as u64)) & 0xFF00FF0000FF00FF;

        (s.wrapping_mul(0x0C001C000014002C_u64.to_le()) >> 58) as u8
    }
}

impl Pixel for [u8; 4] {
    const HAS_ALPHA: bool = true;

    #[inline]
    fn new() -> Self {
        [0; 4]
    }

    #[inline]
    fn new_opaque() -> Self {
        [0, 0, 0, 0xff]
    }

    #[inline]
    fn read(&mut self, bytes: &[u8]) {
        match bytes.try_into() {
            Ok(rgba) => {
                *self = rgba;
            }
            _ => unreachable(),
        }
    }

    #[inline]
    fn write(&self, bytes: &mut [u8]) {
        assert_eq!(bytes.len(), self.len());
        bytes.copy_from_slice(self)
    }

    #[inline]
    fn var(&self, prev: &Self) -> Var {
        let [r, g, b, a] = *self;
        let [pr, pg, pb, pa] = *prev;
        debug_assert_eq!(a, pa);

        let r = r.wrapping_sub(pr);
        let g = g.wrapping_sub(pg);
        let b = b.wrapping_sub(pb);

        Var { r, g, b }
    }

    #[inline]
    fn r(&self) -> u8 {
        self[0]
    }

    #[inline]
    fn g(&self) -> u8 {
        self[1]
    }

    #[inline]
    fn b(&self) -> u8 {
        self[2]
    }

    #[inline]
    fn rgb(&self) -> [u8; 3] {
        let [r, g, b, _] = *self;
        [r, g, b]
    }

    #[inline]
    fn rgba(&self) -> [u8; 4] {
        *self
    }

    #[inline]
    fn a(&self) -> u8 {
        self[3]
    }

    #[inline]
    fn set_r(&mut self, r: u8) {
        self[0] = r;
    }

    #[inline]
    fn set_g(&mut self, g: u8) {
        self[1] = g;
    }

    #[inline]
    fn set_b(&mut self, b: u8) {
        self[2] = b;
    }

    #[inline]
    fn set_a(&mut self, a: u8) {
        self[3] = a;
    }

    #[inline]
    fn set_rgb(&mut self, r: u8, g: u8, b: u8) {
        *self = [r, g, b, self[3]];
    }

    #[inline]
    fn set_rgba(&mut self, r: u8, g: u8, b: u8, a: u8) {
        *self = [r, g, b, a];
    }

    #[inline]
    fn add_rgb(&mut self, r: u8, g: u8, b: u8) {
        self[0] = self[0].wrapping_add(r);
        self[1] = self[1].wrapping_add(g);
        self[2] = self[2].wrapping_add(b);
    }

    #[inline]
    fn hash(&self) -> u8 {
        let v = u32::from_ne_bytes(*self);
        let s = (((v as u64) << 32) | (v as u64)) & 0xFF00FF0000FF00FF;

        (s.wrapping_mul(0x0C001C000014002C_u64.to_le()) >> 58) as u8
    }
}

/// Color variance value.
/// Wrapping difference between two pixels.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(crate) struct Var {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Var {
    #[inline]
    // Vendored upstream expression; the `as u8` casts are no-ops here but kept
    // to match upstream verbatim.
    #[allow(clippy::unnecessary_cast)]
    fn diff(&self) -> Option<u8> {
        let r = self.r.wrapping_add(2);
        let g = self.g.wrapping_add(2);
        let b = self.b.wrapping_add(2);

        match r | g | b {
            0x00..=0x03 => Some(QOI_OP_DIFF | (r << 4) as u8 | (g << 2) as u8 | b as u8),
            _ => None,
        }
    }

    #[inline]
    fn luma(&self) -> Option<[u8; 2]> {
        let r = self.r.wrapping_add(8).wrapping_sub(self.g);
        let g = self.g.wrapping_add(32);
        let b = self.b.wrapping_add(8).wrapping_sub(self.g);

        match (r | b, g) {
            (0x00..=0x0F, 0x00..=0x3F) => Some([QOI_OP_LUMA | g, r << 4 | b]),
            _ => None,
        }
    }
}

/// Image color space variants.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Colors {
    /// SRGB color channels.
    Srgb,

    /// SRGB color channels and linear alpha channel.
    SrgbLinA,

    /// Linear color channels.
    Rgb,

    /// Linear color and alpha channels.
    Rgba,
}

impl Colors {
    /// Returns `true` if color space has alpha channel.
    /// Returns `false` otherwise.
    #[inline]
    pub(crate) const fn has_alpha(&self) -> bool {
        match self {
            Colors::Rgb | Colors::Srgb => false,
            Colors::Rgba | Colors::SrgbLinA => true,
        }
    }

    /// Returns `4` if color space has alpha channel.
    /// Returns `3` otherwise.
    #[inline]
    pub(crate) const fn channels(&self) -> usize {
        match self {
            Colors::Rgb | Colors::Srgb => 3,
            Colors::Rgba | Colors::SrgbLinA => 4,
        }
    }
}

/// QOI descriptor value.\
/// This value is parsed from image header during decoding.\
/// Or provided by caller to drive encoding.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Qoi {
    /// Width of the image in pixels.
    pub width: u32,

    /// Height of the image in pixels.
    pub height: u32,

    /// Specifies image color space.
    pub colors: Colors,
}

#[inline]
#[cold]
const fn cold() {}

#[inline]
const fn likely(b: bool) -> bool {
    if !b {
        cold();
    }
    b
}

#[inline]
const fn unlikely(b: bool) -> bool {
    if b {
        cold();
    }
    b
}

/// Next best thing after `core::hint::unreachable_unchecked()`
/// If happens to be called this will stall CPU, instead of causing UB.
#[inline]
#[cold]
#[allow(clippy::empty_loop)]
const fn unreachable() -> ! {
    loop {}
}
