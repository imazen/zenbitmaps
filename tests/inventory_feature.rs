#![cfg(feature = "zencodec")]
use zencodec::decode::DecodeJob as _;

#[cfg(not(feature = "inventory"))]
fn check_disabled<C: zencodec::decode::DecoderConfig>(config: C) {
    use zencodec::{ErrorCategory, RequestError, UnsupportedOperation};
    let err = config.job().inventory(b"").unwrap_err();
    assert_eq!(
        err.error().category(),
        ErrorCategory::Request(RequestError::Unsupported(UnsupportedOperation::Inventory))
    );
}

#[cfg(feature = "inventory")]
fn check_enabled<C: zencodec::decode::DecoderConfig>(config: C) {
    use enough::{Stop, StopReason};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct AtPoll {
        polls: Arc<AtomicUsize>,
        fail_at: usize,
    }
    impl Stop for AtPoll {
        fn check(&self) -> Result<(), StopReason> {
            if self.polls.fetch_add(1, Ordering::Relaxed) == self.fail_at {
                Err(StopReason::TimedOut)
            } else {
                Ok(())
            }
        }
    }
    for bytes in [
        b"".as_slice(),
        b"malformed".as_slice(),
        b"P6\n1 1\n255\n\0\0\0".as_slice(),
    ] {
        let polls = Arc::new(AtomicUsize::new(0));
        let run = |fail_at| {
            config
                .clone()
                .job()
                .with_pulse(zencodec::SharedPulse::new(
                    zencodec::how_far::StopOnly::new(AtPoll {
                        polls: polls.clone(),
                        fail_at,
                    }),
                ))
                .inventory(bytes)
        };
        run(usize::MAX).unwrap();
        let total = polls.load(Ordering::Relaxed);
        assert!(total > 0);
        for fail_at in 0..total {
            polls.store(0, Ordering::Relaxed);
            let error = run(fail_at).unwrap_err();
            assert_eq!(
                error.error().category(),
                zencodec::ErrorCategory::Stopped(StopReason::TimedOut),
                "poll {fail_at}/{total}"
            );
        }
    }
}

macro_rules! check_format {
    ($name:ident, $config:ty) => {
        #[test]
        fn $name() {
            #[cfg(feature = "inventory")]
            check_enabled(<$config>::new());
            #[cfg(not(feature = "inventory"))]
            check_disabled(<$config>::new());
        }
    };
}
check_format!(pnm, zenbitmaps::PnmDecoderConfig);
check_format!(farbfeld, zenbitmaps::FarbfeldDecoderConfig);
#[cfg(feature = "bmp")]
check_format!(bmp, zenbitmaps::BmpDecoderConfig);
#[cfg(feature = "hdr")]
check_format!(hdr, zenbitmaps::HdrDecoderConfig);
#[cfg(feature = "qoi")]
check_format!(qoi, zenbitmaps::QoiDecoderConfig);
#[cfg(feature = "tga")]
check_format!(tga, zenbitmaps::TgaDecoderConfig);

#[cfg(all(feature = "inventory", feature = "qoi"))]
#[test]
fn qoi_inventory_observes_timeout_during_opcode_traversal() {
    use zencodec::decode::DecoderConfig as _;
    struct StopOnce(std::sync::atomic::AtomicUsize);
    impl enough::Stop for StopOnce {
        fn check(&self) -> Result<(), enough::StopReason> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 3 {
                Err(enough::StopReason::TimedOut)
            } else {
                Ok(())
            }
        }
    }
    let mut bytes = b"qoif".to_vec();
    bytes.extend_from_slice(&4096u32.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&[3, 0]);
    bytes.resize(14 + 4096, 0);
    bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
    let error = zenbitmaps::QoiDecoderConfig::new()
        .job()
        .with_pulse(zencodec::SharedPulse::new(
            zencodec::how_far::StopOnly::new(StopOnce(0.into())),
        ))
        .inventory(&bytes)
        .unwrap_err();
    assert_eq!(
        error.error().category(),
        zencodec::ErrorCategory::Stopped(enough::StopReason::TimedOut)
    );
}
