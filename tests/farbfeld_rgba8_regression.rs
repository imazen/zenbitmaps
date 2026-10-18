//! RGBA8 farbfeld encoding against an independent wire-format expectation:
//! every 8-bit value in each channel position, row tails, and the 16-row
//! cancellation cadence of the row writer.

use enough::{Stop, StopReason, Unstoppable};
use std::sync::atomic::{AtomicUsize, Ordering};
use zenbitmaps::{BitmapError, PixelLayout, encode_farbfeld};

#[test]
fn rgba8_matches_sixteen_bit_wire_samples_for_all_values_and_row_tails() {
    let mut seen = [[false; 256]; 4];
    for width in [1_u32, 3, 17] {
        for height in [1_u32, 2, 17] {
            let mut pixels = Vec::new();
            for i in 0..(width * height) {
                for (channel, channel_seen) in seen.iter_mut().enumerate() {
                    let value = (i * 73 + channel as u32 * 61 + 17) as u8;
                    pixels.push(value);
                    channel_seen[value as usize] = true;
                }
            }
            // Independent wire-format oracle: channels are unsigned 16-bit
            // big-endian values, with 8-bit endpoints expanded by 65535 / 255.
            let mut expected = b"farbfeld".to_vec();
            expected.extend_from_slice(&width.to_be_bytes());
            expected.extend_from_slice(&height.to_be_bytes());
            for &sample in &pixels {
                expected.extend_from_slice(&(u16::from(sample) * 257).to_be_bytes());
            }
            pixels.extend_from_slice(&[0xa5; 13]);
            let actual = encode_farbfeld(&pixels, width, height, PixelLayout::Rgba8, Unstoppable)
                .expect("valid RGBA8 image");
            assert_eq!(actual, expected, "{width}x{height}");
        }
    }
    assert!(
        seen.iter()
            .all(|channel| channel.iter().all(|&value| value))
    );
}

struct CancelOnThirdCheck(AtomicUsize);

impl Stop for CancelOnThirdCheck {
    fn check(&self) -> Result<(), StopReason> {
        if self.0.fetch_add(1, Ordering::SeqCst) + 1 >= 3 {
            Err(StopReason::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[test]
fn rgba8_still_checks_cancellation_at_row_sixteen() {
    for width in [1_u32, 3, 17] {
        for height in [16_u32, 17] {
            let pixels = vec![127; width as usize * height as usize * 4];
            let stop = CancelOnThirdCheck(AtomicUsize::new(0));
            let result = encode_farbfeld(&pixels, width, height, PixelLayout::Rgba8, &stop);
            if height == 16 {
                result.expect("initial and row-zero checks pass");
                assert_eq!(stop.0.load(Ordering::SeqCst), 2);
            } else {
                assert!(matches!(
                    result
                        .expect_err("row sixteen must check cancellation")
                        .error(),
                    BitmapError::Cancelled(StopReason::Cancelled)
                ));
                assert_eq!(stop.0.load(Ordering::SeqCst), 3);
            }
        }
    }
}
