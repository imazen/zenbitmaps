#![cfg(all(feature = "tga", feature = "zencodec"))]
use zenbitmaps::TgaEncoderConfig;
use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
use zenpixels::{PixelDescriptor, PixelSlice};

fn pixels() -> PixelSlice<'static> {
    PixelSlice::new(
        &[1, 2, 3, 4, 5, 6, 199, 199, 199, 7, 8, 9, 10, 11, 12],
        2,
        2,
        9,
        PixelDescriptor::RGB8_SRGB,
    )
    .unwrap()
}
fn encoder() -> zenbitmaps::TgaEncoder {
    TgaEncoderConfig::new().job().encoder().unwrap()
}

#[test]
fn declared_tga_rejects_one_shot_and_incomplete_rows() {
    let desc = PixelDescriptor::RGB8_SRGB;
    let mut e = encoder();
    e.begin(2, 2, desc).unwrap();
    assert!(e.encode(pixels()).is_err());
    let mut e = encoder();
    e.begin(2, 3, desc).unwrap();
    e.push_rows(pixels()).unwrap();
    assert!(e.finish().is_err());
}
#[test]
fn declared_tga_checks_before_mutating_and_preserves_strided_samples() {
    let desc = PixelDescriptor::RGB8_SRGB;
    let want = encoder().encode(pixels()).unwrap().into_vec();
    let mut e = encoder();
    assert!(e.begin(65536, 1, desc).is_err());
    e.begin(2, 2, desc).unwrap();
    assert!(e.begin(2, 2, desc).is_err());
    assert!(
        e.push_rows(PixelSlice::new(&[1, 2, 3], 1, 1, 3, desc).unwrap())
            .is_err()
    );
    e.push_rows(pixels()).unwrap();
    assert!(e.push_rows(pixels()).is_err());
    assert_eq!(e.finish().unwrap().into_vec(), want);
}
#[test]
fn declared_tga_runs_shared_conformance() {
    zencodec_testkit::check_streaming_output_honesty(
        TgaEncoderConfig::new(),
        zenbitmaps::TgaDecoderConfig::new(),
        &zencodec_testkit::TestImage::rgb8_gradient(5, 4),
    )
    .unwrap();
}

#[test]
fn row_boundaries_preserve_the_retained_pulse_reason() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use zencodec::how_far::{Child, Execution, PhaseSpec, PlanError, Pulse, Report};
    struct Deadline(Arc<AtomicBool>);
    impl enough::Stop for Deadline {
        fn check(&self) -> Result<(), enough::StopReason> {
            if self.0.load(Ordering::Relaxed) {
                Err(enough::StopReason::TimedOut)
            } else {
                Ok(())
            }
        }
    }
    impl Report for Deadline {
        fn advance(&self, _: u64) {}
    }
    impl Pulse for Deadline {
        fn split(&self, _: Execution, _: &[PhaseSpec<'_>]) -> Result<Vec<Child<'_>>, PlanError> {
            Err(PlanError::Unsupported)
        }
    }
    for stage in 0..3 {
        let expired = Arc::new(AtomicBool::new(false));
        let mut e = TgaEncoderConfig::new()
            .job()
            .with_pulse(zencodec::SharedPulse::new(Deadline(expired.clone())))
            .encoder()
            .unwrap();
        if stage > 0 {
            e.begin(2, 2, PixelDescriptor::RGB8_SRGB).unwrap();
        }
        if stage > 1 {
            e.push_rows(pixels()).unwrap();
        }
        expired.store(true, Ordering::Relaxed);
        let err = match stage {
            0 => e.begin(2, 2, PixelDescriptor::RGB8_SRGB).unwrap_err(),
            1 => e.push_rows(pixels()).unwrap_err(),
            _ => e.finish().unwrap_err(),
        };
        assert_eq!(
            err.error().category(),
            zencodec::ErrorCategory::Stopped(enough::StopReason::TimedOut)
        );
    }
}
