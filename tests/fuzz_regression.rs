//! Replay seed inputs from `fuzz/regression/` through every fuzz target
//! entry point. Shared scaffolding lives in `zen-fuzz-regress`.

use zen_fuzz_regress::RegressionSuite;

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
            use zenbitmaps::{decode, encode_pam};
            if let Ok(decoded) = decode(input, enough::Unstoppable) {
                let _ = encode_pam(
                    decoded.pixels(),
                    decoded.width,
                    decoded.height,
                    decoded.layout,
                    enough::Unstoppable,
                );
            }
            #[cfg(feature = "bmp")]
            {
                use zenbitmaps::{decode_bmp, encode_bmp, encode_bmp_rgba, PixelLayout};
                if let Ok(decoded) = decode_bmp(input, enough::Unstoppable) {
                    if decoded.layout == PixelLayout::Rgba8 {
                        let _ = encode_bmp_rgba(
                            decoded.pixels(),
                            decoded.width,
                            decoded.height,
                            decoded.layout,
                            enough::Unstoppable,
                        );
                    } else {
                        let _ = encode_bmp(
                            decoded.pixels(),
                            decoded.width,
                            decoded.height,
                            decoded.layout,
                            enough::Unstoppable,
                        );
                    }
                }
            }
        })
        .run();
}
