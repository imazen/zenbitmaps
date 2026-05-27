//! Native, spec-compliant QOI chunk decoder.
//!
//! This is a self-contained replacement for the per-row decode loop that
//! previously delegated to `rapid_qoi::Qoi::decode_range`. The upstream
//! `decode_range` panics with `mid > len` when a `QOI_OP_RUN` chunk's
//! run-length extends past the end of the output slice it is handed
//! (`split_at_mut(run)` is not clamped to the remaining length). zenbitmaps
//! feeds the decoder one row at a time, so any run that legitimately crosses
//! a row boundary triggers the panic on spec-valid input.
//!
//! This implementation clamps every run to the remaining output and carries
//! the leftover run-length across calls via [`QoiDecodeState::run`], so a run
//! that spans multiple rows decodes correctly. Pixel output is byte-identical
//! to the QOI reference decoder.
//!
//! Reference: <https://qoiformat.org/qoi-specification.pdf>

/// Carried decode state for streaming QOI decode across output chunks (rows).
///
/// `N` is the number of channels (3 for RGB, 4 for RGBA).
pub(crate) struct QoiDecodeState<const N: usize> {
    /// Running array of previously seen pixels, indexed by QOI hash.
    index: [[u8; N]; 64],
    /// Previous pixel value.
    px: [u8; N],
    /// Run-length remaining from a `QOI_OP_RUN` that did not fit in the
    /// previous output chunk. Pixels repeating `px` are emitted before
    /// reading any further chunk bytes.
    run: usize,
}

const QOI_OP_INDEX: u8 = 0x00; // 00xxxxxx
const QOI_OP_DIFF: u8 = 0x40; // 01xxxxxx
const QOI_OP_LUMA: u8 = 0x80; // 10xxxxxx
const QOI_OP_RUN: u8 = 0xc0; // 11xxxxxx
const QOI_OP_RGB: u8 = 0xfe; // 11111110
const QOI_OP_RGBA: u8 = 0xff; // 11111111
const QOI_MASK_2: u8 = 0xc0; // 11000000

/// Compute the QOI index hash for a pixel.
///
/// `index_position = (r*3 + g*5 + b*7 + a*11) % 64`. For RGB pixels the alpha
/// is treated as the constant 255 (matching the QOI reference decoder).
#[inline]
fn hash<const N: usize>(px: &[u8; N]) -> usize {
    let r = px[0] as u32;
    let g = px[1] as u32;
    let b = px[2] as u32;
    let a = if N == 4 { px[3] as u32 } else { 255 };
    ((r.wrapping_mul(3))
        .wrapping_add(g.wrapping_mul(5))
        .wrapping_add(b.wrapping_mul(7))
        .wrapping_add(a.wrapping_mul(11))
        % 64) as usize
}

impl<const N: usize> QoiDecodeState<N> {
    /// Create fresh decode state per the QOI spec: the running index array is
    /// zero-initialized and the previous pixel starts at opaque black
    /// `{0,0,0,255}`.
    #[inline]
    pub(crate) fn new() -> Self {
        let mut px = [0u8; N];
        if N == 4 {
            px[3] = 255;
        }
        Self {
            index: [[0u8; N]; 64],
            px,
            run: 0,
        }
    }

    /// Decode QOI chunks from `bytes` into `out`, filling exactly `out.len()`
    /// bytes (which must be a whole multiple of `N`).
    ///
    /// Returns the number of bytes consumed from `bytes`. Decode state
    /// (`index`, `px`, `run`) is carried in `self` so the next call resumes
    /// where this one left off — mirroring the streaming contract of the
    /// previous `rapid_qoi::Qoi::decode_range` call.
    ///
    /// Returns `Err(())` on truncated input (a chunk that needs more bytes
    /// than are available).
    pub(crate) fn decode_into(&mut self, bytes: &[u8], out: &mut [u8]) -> Result<usize, ()> {
        debug_assert_eq!(out.len() % N, 0);
        let px_count = out.len() / N;
        let mut written = 0usize;
        let mut pos = 0usize;

        // First, drain any run carried over from a previous chunk.
        if self.run > 0 {
            let fill = self.run.min(px_count);
            for _ in 0..fill {
                out[written * N..written * N + N].copy_from_slice(&self.px);
                written += 1;
            }
            self.run -= fill;
            if written == px_count {
                return Ok(0);
            }
        }

        while written < px_count {
            let b1 = *bytes.get(pos).ok_or(())?;
            pos += 1;

            if b1 == QOI_OP_RGB {
                let r = *bytes.get(pos).ok_or(())?;
                let g = *bytes.get(pos + 1).ok_or(())?;
                let b = *bytes.get(pos + 2).ok_or(())?;
                pos += 3;
                self.px[0] = r;
                self.px[1] = g;
                self.px[2] = b;
                // alpha unchanged
            } else if b1 == QOI_OP_RGBA {
                let r = *bytes.get(pos).ok_or(())?;
                let g = *bytes.get(pos + 1).ok_or(())?;
                let b = *bytes.get(pos + 2).ok_or(())?;
                let a = *bytes.get(pos + 3).ok_or(())?;
                pos += 4;
                self.px[0] = r;
                self.px[1] = g;
                self.px[2] = b;
                if N == 4 {
                    self.px[3] = a;
                }
            } else {
                match b1 & QOI_MASK_2 {
                    QOI_OP_INDEX => {
                        self.px = self.index[(b1 & 0x3f) as usize];
                        // QOI_OP_INDEX does not re-insert into the index array;
                        // emit and continue without the hash update below.
                        out[written * N..written * N + N].copy_from_slice(&self.px);
                        written += 1;
                        continue;
                    }
                    QOI_OP_DIFF => {
                        let dr = ((b1 >> 4) & 0x03).wrapping_sub(2);
                        let dg = ((b1 >> 2) & 0x03).wrapping_sub(2);
                        let db = (b1 & 0x03).wrapping_sub(2);
                        self.px[0] = self.px[0].wrapping_add(dr);
                        self.px[1] = self.px[1].wrapping_add(dg);
                        self.px[2] = self.px[2].wrapping_add(db);
                    }
                    QOI_OP_LUMA => {
                        let b2 = *bytes.get(pos).ok_or(())?;
                        pos += 1;
                        let vg = (b1 & 0x3f).wrapping_sub(32);
                        let dr = ((b2 >> 4) & 0x0f).wrapping_sub(8).wrapping_add(vg);
                        let db = (b2 & 0x0f).wrapping_sub(8).wrapping_add(vg);
                        self.px[0] = self.px[0].wrapping_add(dr);
                        self.px[1] = self.px[1].wrapping_add(vg);
                        self.px[2] = self.px[2].wrapping_add(db);
                    }
                    QOI_OP_RUN => {
                        // run-length stored with bias -1, so add 1.
                        let run = (b1 & 0x3f) as usize + 1;
                        let remaining = px_count - written;
                        let fill = run.min(remaining);
                        for _ in 0..fill {
                            out[written * N..written * N + N].copy_from_slice(&self.px);
                            written += 1;
                        }
                        // Carry the unfilled remainder to the next chunk.
                        self.run = run - fill;
                        continue;
                    }
                    _ => unreachable!("2-bit tag space is exhaustive"),
                }
            }

            self.index[hash(&self.px)] = self.px;
            out[written * N..written * N + N].copy_from_slice(&self.px);
            written += 1;
        }

        Ok(pos)
    }
}
