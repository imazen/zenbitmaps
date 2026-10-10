#![no_main]
use libfuzzer_sys::fuzz_target;
use zencodec::decode::{DecodeJob, DecoderConfig};

fn check<C: DecoderConfig>(cfg: C, data: &[u8]) {
    if let Ok(Some(inv)) = cfg.job().inventory(data) {
        inv.validate().expect("inventory must tile the input");
    }
}

fuzz_target!(|data: &[u8]| {
    check(zenbitmaps::PnmDecoderConfig::new(), data);
    check(zenbitmaps::BmpDecoderConfig::new(), data);
    // The BMP walker takes different paths under Strict and Permissive.
    for policy in [
        zencodec::decode::DecodePolicy::none().with_strict(true),
        zencodec::decode::DecodePolicy::permissive(),
    ] {
        if let Ok(Some(inv)) = zenbitmaps::BmpDecoderConfig::new()
            .job()
            .with_policy(policy)
            .inventory(data)
        {
            inv.validate().expect("BMP inventory must tile the input");
        }
    }
    check(zenbitmaps::FarbfeldDecoderConfig::new(), data);
    check(zenbitmaps::QoiDecoderConfig::new(), data);
    check(zenbitmaps::TgaDecoderConfig::new(), data);
    check(zenbitmaps::HdrDecoderConfig::new(), data);
});
