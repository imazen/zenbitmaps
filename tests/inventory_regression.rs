//! Replays `fuzz/regression/inventory/*` through every decoder's inventory:
//! each must produce a part list that tiles the input.
#![cfg(all(
    feature = "zencodec",
    feature = "bmp",
    feature = "qoi",
    feature = "tga",
    feature = "hdr"
))]

use zenbitmaps::*;
use zencodec::decode::{DecodeJob, DecoderConfig};

fn check<C: DecoderConfig>(cfg: C, name: &str, data: &[u8]) {
    let inv = cfg.job().inventory(data).unwrap().unwrap();
    inv.validate()
        .unwrap_or_else(|e| panic!("{name}: {e}\n{inv}"));
}

#[test]
fn inventory_regression_seeds_tile() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/regression/inventory");
    let mut n = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let data = std::fs::read(&p).unwrap();
        check(PnmDecoderConfig::new(), &name, &data);
        check(BmpDecoderConfig::new(), &name, &data);
        for policy in [
            zencodec::decode::DecodePolicy::none().with_strict(true),
            zencodec::decode::DecodePolicy::permissive(),
        ] {
            let inv = BmpDecoderConfig::new()
                .job()
                .with_policy(policy)
                .inventory(&data)
                .unwrap()
                .unwrap();
            inv.validate()
                .unwrap_or_else(|e| panic!("{name} under {policy:?}: {e}\n{inv}"));
        }
        check(FarbfeldDecoderConfig::new(), &name, &data);
        check(QoiDecoderConfig::new(), &name, &data);
        check(TgaDecoderConfig::new(), &name, &data);
        check(HdrDecoderConfig::new(), &name, &data);
        n += 1;
    }
    assert!(n >= 1, "no seeds in {}", dir.display());
}
