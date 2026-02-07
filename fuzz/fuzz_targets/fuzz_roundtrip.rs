#![no_main]
use libfuzzer_sys::fuzz_target;
use zenpnm::*;

fuzz_target!(|data: &[u8]| {
    // If we can decode it, re-encoding and decoding again must produce identical pixels
    let Ok(decoded) = decode(data, enough::Unstoppable) else {
        return;
    };

    // Re-encode in the same format
    let reencoded = match decoded.format {
        BitmapFormat::Ppm => encode_ppm(
            decoded.pixels(), decoded.width, decoded.height,
            decoded.layout, enough::Unstoppable,
        ),
        BitmapFormat::Pgm => encode_pgm(
            decoded.pixels(), decoded.width, decoded.height,
            decoded.layout, enough::Unstoppable,
        ),
        BitmapFormat::Pam => encode_pam(
            decoded.pixels(), decoded.width, decoded.height,
            decoded.layout, enough::Unstoppable,
        ),
        BitmapFormat::Bmp => {
            if decoded.layout == PixelLayout::Rgba8 {
                encode_bmp_rgba(
                    decoded.pixels(), decoded.width, decoded.height,
                    decoded.layout, enough::Unstoppable,
                )
            } else {
                encode_bmp(
                    decoded.pixels(), decoded.width, decoded.height,
                    decoded.layout, enough::Unstoppable,
                )
            }
        }
        _ => return, // PFM roundtrip has float precision concerns, skip
    };

    let Ok(reencoded) = reencoded else { return };
    let Ok(decoded2) = decode(&reencoded, enough::Unstoppable) else {
        panic!("re-encoded data failed to decode");
    };

    assert_eq!(decoded.pixels(), decoded2.pixels(), "roundtrip pixel mismatch");
    assert_eq!(decoded.width, decoded2.width);
    assert_eq!(decoded.height, decoded2.height);
});
