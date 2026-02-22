//! BMP bit expansion utilities.
//!
//! Forked from zune-bmp 0.5.2 by Caleb Etemesi (MIT/Apache-2.0/Zlib).

/// Expand sub-byte bit depths (1, 2, 4 bits per pixel) to full bytes.
///
/// When `plte_present` is true, output values are raw palette indices (scale=1).
/// When false, values are scaled to 0–255.
pub(crate) fn expand_bits_to_byte(depth: usize, plte_present: bool, input: &[u8], out: &mut [u8]) {
    let scale: u8 = if plte_present {
        1
    } else {
        match depth {
            1 => 0xFF,
            2 => 0x55,
            4 => 0x11,
            _ => return,
        }
    };

    if depth == 1 {
        let mut in_iter = input.iter();
        let mut out_iter = out.chunks_exact_mut(8);

        (&mut out_iter)
            .zip(&mut in_iter)
            .for_each(|(out_vals, in_val)| {
                let cur: &mut [u8; 8] = out_vals.try_into().unwrap();
                cur[0] = scale.wrapping_mul((in_val >> 7) & 0x01);
                cur[1] = scale.wrapping_mul((in_val >> 6) & 0x01);
                cur[2] = scale.wrapping_mul((in_val >> 5) & 0x01);
                cur[3] = scale.wrapping_mul((in_val >> 4) & 0x01);
                cur[4] = scale.wrapping_mul((in_val >> 3) & 0x01);
                cur[5] = scale.wrapping_mul((in_val >> 2) & 0x01);
                cur[6] = scale.wrapping_mul((in_val >> 1) & 0x01);
                cur[7] = scale.wrapping_mul(in_val & 0x01);
            });

        if let Some(in_val) = in_iter.next() {
            let remainder_iter = out_iter.into_remainder().iter_mut();
            remainder_iter.enumerate().for_each(|(pos, out_val)| {
                let shift = 7_usize.wrapping_sub(pos);
                *out_val = scale.wrapping_mul((in_val >> shift) & 0x01);
            });
        }
    } else if depth == 2 {
        let mut in_iter = input.iter();
        let mut out_iter = out.chunks_exact_mut(4);

        (&mut out_iter)
            .zip(&mut in_iter)
            .for_each(|(out_vals, in_val)| {
                let cur: &mut [u8; 4] = out_vals.try_into().unwrap();
                cur[0] = scale.wrapping_mul((in_val >> 6) & 0x03);
                cur[1] = scale.wrapping_mul((in_val >> 4) & 0x03);
                cur[2] = scale.wrapping_mul((in_val >> 2) & 0x03);
                cur[3] = scale.wrapping_mul(in_val & 0x03);
            });

        if let Some(in_val) = in_iter.next() {
            let remainder_iter = out_iter.into_remainder().iter_mut();
            remainder_iter.enumerate().for_each(|(pos, out_val)| {
                let shift = 6_usize.wrapping_sub(pos * 2);
                *out_val = scale.wrapping_mul((in_val >> shift) & 0x03);
            });
        }
    } else if depth == 4 {
        let mut in_iter = input.iter();
        let mut out_iter = out.chunks_exact_mut(2);

        (&mut out_iter)
            .zip(&mut in_iter)
            .for_each(|(out_vals, in_val)| {
                let cur: &mut [u8; 2] = out_vals.try_into().unwrap();
                cur[0] = scale.wrapping_mul((in_val >> 4) & 0x0f);
                cur[1] = scale.wrapping_mul(in_val & 0x0f);
            });

        if let Some(in_val) = in_iter.next() {
            let remainder_iter = out_iter.into_remainder().iter_mut();
            remainder_iter.enumerate().for_each(|(pos, out_val)| {
                let shift = 4_usize.wrapping_sub(pos * 4);
                *out_val = scale.wrapping_mul((in_val >> shift) & 0x0f);
            });
        }
    }
}

/// Bitfield shift/scale table for converting N-bit values to 8-bit.
pub(crate) const MUL_TABLE: [u32; 9] = [
    0,    // 0 bits
    0xff, // 1 bit:  0b11111111
    0x55, // 2 bits: 0b01010101
    0x49, // 3 bits: 0b01001001
    0x11, // 4 bits: 0b00010001
    0x21, // 5 bits: 0b00100001
    0x41, // 6 bits: 0b01000001
    0x81, // 7 bits: 0b10000001
    0x01, // 8 bits: 0b00000001
];

pub(crate) const SHIFT_TABLE: [i32; 9] = [0, 0, 0, 1, 0, 2, 4, 6, 0];

/// Extract and scale a bitfield value to 8-bit range.
pub(crate) fn shift_signed(mut v: u32, shift: i32, mut bits: u32) -> u32 {
    if shift < 0 {
        v <<= -shift;
    } else {
        v >>= shift;
    }
    bits = bits.clamp(0, 8);
    v >>= 8 - bits;
    (v.wrapping_mul(MUL_TABLE[bits as usize])) >> SHIFT_TABLE[bits as usize]
}
