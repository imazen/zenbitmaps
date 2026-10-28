//! Jobs retain the caller's complete `SharedPulse`, and incremental steps also
//! honor the borrowed per-step pulse. Every stop point surfaces as a typed
//! `ErrorCategory::Stopped` (or `StepError::Cancelled`) carrying the original
//! reason. The pulse here is a full `Pulse`, not the `StopOnly` adapter.
#![cfg(feature = "zencodec")]
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use enough::{Stop, StopReason};
use whereat::At;
use zencodec::decode::{Decode, DecodeJob, DecoderConfig, StepError};
use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
use zencodec::how_far::{Child, Execution, PhaseSpec, PlanError, Pulse, Report};
use zencodec::{CodecError, ErrorCategory, SharedPulse};
use zencodec_io::{CollectSink, DriveFailure, drive_from_slice};
use zenpixels::{PixelDescriptor, PixelSlice};

/// A deadline that has passed once `polls` reaches `fail_at`, and stays passed.
struct Deadline {
    polls: Arc<AtomicUsize>,
    fail_at: usize,
}
impl Stop for Deadline {
    fn check(&self) -> Result<(), StopReason> {
        if self.polls.fetch_add(1, Ordering::Relaxed) >= self.fail_at {
            Err(StopReason::TimedOut)
        } else {
            Ok(())
        }
    }
}
impl Report for Deadline {
    fn advance(&self, _completed: u64) {}
}
impl Pulse for Deadline {
    fn split(
        &self,
        _execution: Execution,
        _parts: &[PhaseSpec<'_>],
    ) -> Result<Vec<Child<'_>>, PlanError> {
        Err(PlanError::Unsupported)
    }
}

fn deadline(polls: &Arc<AtomicUsize>, fail_at: usize) -> Deadline {
    polls.store(0, Ordering::Relaxed);
    Deadline {
        polls: polls.clone(),
        fail_at,
    }
}

fn assert_timed_out(error: &At<CodecError>, what: &str) {
    assert_eq!(
        error.error().category(),
        ErrorCategory::Stopped(StopReason::TimedOut),
        "{what}: {error:?}"
    );
}

/// Every poll of the retained pulse can stop the operation with its reason.
fn each_stop_point<T>(what: &str, mut run: impl FnMut(Deadline) -> Result<T, At<CodecError>>) {
    let polls = Arc::new(AtomicUsize::new(0));
    run(deadline(&polls, usize::MAX)).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    let total = polls.load(Ordering::Relaxed);
    assert!(total > 0, "{what}: the retained pulse was never polled");
    for fail_at in 0..total {
        match run(deadline(&polls, fail_at)) {
            Ok(_) => panic!("{what}: completed past an expired deadline at poll {fail_at}/{total}"),
            Err(e) => assert_timed_out(&e, &format!("{what} poll {fail_at}/{total}")),
        }
    }
}

fn check_format<E, D>(what: &str, enc: E, dec: D, pixels: PixelSlice<'_>)
where
    E: EncoderConfig<Error = At<CodecError>>,
    <E::Job as EncodeJob>::Enc: Encoder<Error = At<CodecError>>,
    D: DecoderConfig<Error = At<CodecError>>,
{
    let bytes = enc.clone().job().encode(pixels.clone()).unwrap();
    let bytes = bytes.data();

    each_stop_point(&format!("{what} encode"), |pulse| {
        enc.clone()
            .job()
            .with_pulse(SharedPulse::new(pulse))
            .encode(pixels.clone())
    });
    each_stop_point(&format!("{what} decode"), |pulse| {
        dec.clone()
            .job()
            .with_pulse(SharedPulse::new(pulse))
            .decoder(Cow::Borrowed(bytes), &[])?
            .decode()
    });

    // Incremental decode: the job's retained pulse and the borrowed per-step
    // pulse each stop the decoder, with the original reason.
    let decoded = dec
        .clone()
        .job()
        .decoder(Cow::Borrowed(bytes), &[])
        .unwrap()
        .decode()
        .unwrap();
    let decoded = decoded.pixels();
    let row = decoded.width() as usize * decoded.descriptor().bytes_per_pixel();
    let expected: Vec<u8> = (0..decoded.rows())
        .flat_map(|y| decoded.row(y)[..row].to_vec())
        .collect();
    let polls = Arc::new(AtomicUsize::new(0));
    for borrowed in [false, true] {
        let drive = |fail_at: usize| {
            let p = deadline(&polls, fail_at);
            let (job_pulse, step_pulse) = if borrowed {
                (None, Some(p))
            } else {
                (Some(p), None)
            };
            let mut job = dec.clone().job();
            if let Some(p) = job_pulse {
                job = job.with_pulse(SharedPulse::new(p));
            }
            let mut decoder = job.dyn_incremental_decoder(&[]).unwrap();
            let mut sink = CollectSink::new();
            let step_pulse = step_pulse.as_ref().map(|p| p as &dyn Pulse);
            drive_from_slice(&mut *decoder, bytes, &mut sink, step_pulse, 7)
                .map(|_| sink.bytes().to_vec())
        };
        let mode = if borrowed { "borrowed" } else { "retained" };
        let out = drive(usize::MAX).unwrap_or_else(|e| panic!("{what} {mode}: {e}"));
        assert_eq!(out, expected, "{what} {mode}: incremental pixels");
        let total = polls.load(Ordering::Relaxed);
        assert!(total > 0, "{what} {mode}: pulse never polled");
        for fail_at in 0..total {
            match drive(fail_at) {
                Err(DriveFailure::Step(StepError::Cancelled(reason))) => {
                    assert_eq!(reason, StopReason::TimedOut, "{what} {mode} poll {fail_at}")
                }
                other => panic!("{what} {mode} poll {fail_at}/{total}: {other:?}"),
            }
        }
    }

    // The deprecated cancellation-only adapter still reaches the retained pulse.
    let polls = Arc::new(AtomicUsize::new(0));
    #[allow(deprecated)]
    let error = dec
        .clone()
        .job()
        .with_stop(zencodec::StopToken::new(deadline(&polls, 0)))
        .decoder(Cow::Borrowed(bytes), &[])
        .and_then(|d| d.decode())
        .unwrap_err();
    assert_timed_out(&error, &format!("{what} legacy with_stop"));
}

fn rgb8(width: u32, height: u32) -> Vec<u8> {
    (0..height)
        .flat_map(|y| (0..width).flat_map(move |x| [x as u8, y as u8, (x ^ y) as u8]))
        .collect()
}

#[test]
fn every_format_polls_the_retained_and_borrowed_pulse() {
    use zenbitmaps::*;
    // Two 256 KiB strips plus a short one, so incremental decoding polls
    // between strips.
    let (w, h) = (512u32, 384u32);
    let bytes = rgb8(w, h);
    let pixels =
        || PixelSlice::new(&bytes, w, h, w as usize * 3, PixelDescriptor::RGB8_SRGB).unwrap();
    check_format(
        "pnm",
        PnmEncoderConfig::new(),
        PnmDecoderConfig::new(),
        pixels(),
    );
    check_format(
        "farbfeld",
        FarbfeldEncoderConfig::new(),
        FarbfeldDecoderConfig::new(),
        pixels(),
    );
    #[cfg(feature = "bmp")]
    check_format(
        "bmp",
        BmpEncoderConfig::new(),
        BmpDecoderConfig::new(),
        pixels(),
    );
    #[cfg(feature = "qoi")]
    check_format(
        "qoi",
        QoiEncoderConfig::new(),
        QoiDecoderConfig::new(),
        pixels(),
    );
    #[cfg(feature = "tga")]
    check_format(
        "tga",
        TgaEncoderConfig::new(),
        TgaDecoderConfig::new(),
        pixels(),
    );
    #[cfg(feature = "hdr")]
    {
        let floats: Vec<u8> = (0..w * h * 3)
            .flat_map(|i| ((i % 509) as f32 / 64.0).to_ne_bytes())
            .collect();
        check_format(
            "hdr",
            HdrEncoderConfig::new(),
            HdrDecoderConfig::new(),
            PixelSlice::new(
                &floats,
                w,
                h,
                w as usize * 12,
                PixelDescriptor::RGBF32_LINEAR,
            )
            .unwrap(),
        );
    }
}
