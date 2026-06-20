//! Replay seed inputs from `fuzz/regression/` through every fuzz target
//! entry point. Shared scaffolding lives in `zen-fuzz-regress`.

use zenutils_fuzz::RegressionSuite;

#[test]
fn fuzz_regression() {
    RegressionSuite::new("fuzz/regression")
        .target("decode", |input| {
            let _ = zenbitmaps::decode(input, enough::Unstoppable);
            #[cfg(feature = "bmp")]
            {
                let _ = zenbitmaps::decode_bmp(input, enough::Unstoppable);
            }
            let _ = zenbitmaps::decode_farbfeld(input, enough::Unstoppable);
        })
        .target("roundtrip", |input| {
            // Mirror fuzz/fuzz_targets/fuzz_roundtrip.rs exactly, INCLUDING its
            // pixel-equality asserts — a bare "does not panic" replay would not
            // catch a lossy roundtrip (e.g. zenbitmaps#10, where 16-bit ASCII
            // PPM decoded to a 6-byte "Rgb8" buffer that `encode_pam` truncated
            // back to 3 bytes). The asserts ARE the regression gate.
            use zenbitmaps::{decode, encode_pam};
            if let Ok(decoded) = decode(input, enough::Unstoppable)
                && let Ok(reencoded) = encode_pam(
                    decoded.pixels(),
                    decoded.width,
                    decoded.height,
                    decoded.layout,
                    enough::Unstoppable,
                )
            {
                let decoded2 =
                    decode(&reencoded, enough::Unstoppable).expect("re-encoded PAM must decode");
                assert_eq!(
                    decoded.pixels(),
                    decoded2.pixels(),
                    "PNM PAM roundtrip pixel mismatch"
                );
                assert_eq!(decoded.width, decoded2.width);
                assert_eq!(decoded.height, decoded2.height);
            }
            #[cfg(feature = "bmp")]
            {
                use zenbitmaps::{PixelLayout, decode_bmp, encode_bmp, encode_bmp_rgba};
                if let Ok(decoded) = decode_bmp(input, enough::Unstoppable) {
                    let reencoded = if decoded.layout == PixelLayout::Rgba8 {
                        encode_bmp_rgba(
                            decoded.pixels(),
                            decoded.width,
                            decoded.height,
                            decoded.layout,
                            enough::Unstoppable,
                        )
                    } else {
                        encode_bmp(
                            decoded.pixels(),
                            decoded.width,
                            decoded.height,
                            decoded.layout,
                            enough::Unstoppable,
                        )
                    };
                    if let Ok(reencoded) = reencoded {
                        let decoded2 = decode_bmp(&reencoded, enough::Unstoppable)
                            .expect("re-encoded BMP must decode");
                        assert_eq!(
                            decoded.pixels(),
                            decoded2.pixels(),
                            "BMP roundtrip pixel mismatch"
                        );
                        assert_eq!(decoded.width, decoded2.width);
                        assert_eq!(decoded.height, decoded2.height);
                    }
                }
            }
        })
        .run();
}
