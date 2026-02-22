#![no_main]
use libfuzzer_sys::fuzz_target;
use zenbitmaps::*;

fuzz_target!(|data: &[u8]| {
    // Try PNM decode, then roundtrip
    if let Ok(decoded) = decode(data, enough::Unstoppable) {
        // Re-encode based on layout (we can't know original format, so use PAM which handles all)
        let reencoded = encode_pam(
            decoded.pixels(),
            decoded.width,
            decoded.height,
            decoded.layout,
            enough::Unstoppable,
        );

        let Ok(reencoded) = reencoded else { return };
        let Ok(decoded2) = decode(&reencoded, enough::Unstoppable) else {
            panic!("re-encoded PAM data failed to decode");
        };

        assert_eq!(
            decoded.pixels(),
            decoded2.pixels(),
            "PNM roundtrip pixel mismatch"
        );
        assert_eq!(decoded.width, decoded2.width);
        assert_eq!(decoded.height, decoded2.height);
    }

    // Try BMP decode, then roundtrip
    if let Ok(decoded) = decode_bmp(data, enough::Unstoppable) {
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

        let Ok(reencoded) = reencoded else { return };
        let Ok(decoded2) = decode_bmp(&reencoded, enough::Unstoppable) else {
            panic!("re-encoded BMP data failed to decode");
        };

        assert_eq!(
            decoded.pixels(),
            decoded2.pixels(),
            "BMP roundtrip pixel mismatch"
        );
        assert_eq!(decoded.width, decoded2.width);
        assert_eq!(decoded.height, decoded2.height);
    }
});
