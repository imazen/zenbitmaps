#![cfg(feature = "zencodec")]
use std::borrow::Cow;
use zencodec::Metadata;
use zencodec::decode::{Decode, DecodeJob, DecodePolicy, DecoderConfig};
use zencodec::encode::{AnimationFrameEncoder, EncodeJob, EncodePolicy, Encoder, EncoderConfig};
use zenpixels::{PixelDescriptor, PixelSlice};

fn check<E, D>(enc: E, dec: D, pixels: PixelSlice<'_>)
where
    E: EncoderConfig + 'static,
    D: DecoderConfig + 'static,
    <E::Job as EncodeJob>::Enc: Encoder<Error = E::Error> + Send,
    <E::Job as EncodeJob>::AnimationFrameEnc: AnimationFrameEncoder,
{
    let bytes = enc.clone().job().encode(pixels.clone()).unwrap();
    let expected = dec
        .clone()
        .job()
        .decoder(Cow::Borrowed(bytes.data()), &[])
        .unwrap()
        .decode()
        .unwrap()
        .metadata();
    for (policy, accepted) in [
        (EncodePolicy::none(), true),
        (EncodePolicy::strip_all(), false),
    ] {
        zencodec_testkit::check_encode_policy_admission(
            enc.clone(),
            dec.clone(),
            pixels.clone(),
            Metadata::none(),
            EncodePolicy::none(),
            policy,
            accepted,
            &expected,
        )
        .unwrap();
    }
    for (policy, accepted) in [
        (DecodePolicy::none(), true),
        (DecodePolicy::strict(), false),
    ] {
        zencodec_testkit::check_decode_policy_admission(
            dec.clone(),
            bytes.data(),
            DecodePolicy::none(),
            policy,
            accepted,
            &expected,
        )
        .unwrap();
    }
    zencodec_testkit::check_inventory_dispatch(dec.clone(), bytes.data()).unwrap();
    #[cfg(feature = "inventory")]
    zencodec_testkit::check_inventory_input_limit(dec, bytes.data()).unwrap();
}

#[test]
fn all_available_formats_accept_strided_fixtures_and_honor_contracts() {
    use zenbitmaps::*;
    // Poison padding is outside the two RGB pixels in each row.
    let bytes = [1, 2, 3, 4, 5, 6, 255, 255, 255, 7, 8, 9, 10, 11, 12];
    let pixels = || PixelSlice::new(&bytes, 2, 2, 9, PixelDescriptor::RGB8_SRGB).unwrap();
    check(PnmEncoderConfig::new(), PnmDecoderConfig::new(), pixels());
    check(
        FarbfeldEncoderConfig::new(),
        FarbfeldDecoderConfig::new(),
        pixels(),
    );
    #[cfg(feature = "bmp")]
    check(BmpEncoderConfig::new(), BmpDecoderConfig::new(), pixels());
    #[cfg(feature = "qoi")]
    check(QoiEncoderConfig::new(), QoiDecoderConfig::new(), pixels());
    #[cfg(feature = "tga")]
    check(TgaEncoderConfig::new(), TgaDecoderConfig::new(), pixels());
    #[cfg(feature = "hdr")]
    {
        let bytes: Vec<u8> = [
            0.25f32, 0.5, 0.75, 1.0, 0.125, 2.0, 99.0, 99.0, 99.0, 0.5, 0.25, 1.0, 2.0, 4.0, 8.0,
        ]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
        check(
            HdrEncoderConfig::new(),
            HdrDecoderConfig::new(),
            PixelSlice::new(&bytes, 2, 2, 36, PixelDescriptor::RGBF32_LINEAR).unwrap(),
        );
    }
}
