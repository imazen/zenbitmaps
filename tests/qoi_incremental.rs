#![cfg(all(feature = "qoi", feature = "zencodec"))]

use enough::Unstoppable;
use zencodec::decode::{DecoderConfig, IncrementalDecode};
use zenbitmaps::QoiDecoderConfig;

fn decode_incremental_in_chunks(
    encoded: &[u8],
    chunk_size: usize,
) -> Result<(u32, u32, Vec<u8>), zencodec::decode::BoxedError> {
    let job = QoiDecoderConfig::new().job();
    let mut dec = job.into_incremental_decoder(&[])?;

    let mut all_pixels = Vec::new();
    let mut offset = 0;
    let total_len = encoded.len();

    while !dec.is_complete() {
        let end = (offset + chunk_size).min(total_len);
        let chunk = &encoded[offset..end];
        let is_eof = end == total_len;

        let outcome = dec.push_chunk(chunk, is_eof, None)?;
        offset += outcome.bytes_consumed;

        while let Some((_y, strip)) = dec.pull_batch()? {
            all_pixels.extend_from_slice(&strip.contiguous_bytes());
        }

        if is_eof && outcome.bytes_consumed == 0 && !dec.is_complete() {
            // Cannot make forward progress on EOF
            break;
        }
    }

    let info = dec.info().expect("info should be available");
    assert!(dec.is_complete(), "decoder should be complete");
    Ok((info.width, info.height, all_pixels))
}

#[test]
fn test_qoi_incremental_various_chunk_sizes() {
    // Generate synthetic 64x64 gradient
    let synth: Vec<u8> = (0..64 * 64)
        .flat_map(|i| {
            let x = (i % 64) as u8;
            let y = (i / 64) as u8;
            [x * 4, y * 4, (x ^ y) * 4, 255]
        })
        .collect();

    let encoded = zenbitmaps::encode_qoi(
        &synth,
        64,
        64,
        zenbitmaps::PixelLayout::Rgba8,
        Unstoppable,
    )
    .unwrap();

    let ground_truth = zenbitmaps::decode_qoi(&encoded, Unstoppable).unwrap();
    let ground_truth_bytes = ground_truth.pixels();

    // Test different chunk sizes: whole buffer, medium, small, awkward prime, and 1 byte!
    let chunk_sizes = [encoded.len(), 1024, 64, 13, 7, 3, 1];

    for &cs in &chunk_sizes {
        let (w, h, pixels) = decode_incremental_in_chunks(&encoded, cs)
            .unwrap_or_else(|e| panic!("failed with chunk size {cs}: {e}"));

        assert_eq!(w, 64, "width mismatch for chunk size {cs}");
        assert_eq!(h, 64, "height mismatch for chunk size {cs}");
        assert_eq!(
            pixels.len(),
            ground_truth_bytes.len(),
            "pixel length mismatch for chunk size {cs}"
        );
        assert_eq!(
            pixels, ground_truth_bytes,
            "pixel content mismatch for chunk size {cs}"
        );
    }
}

#[test]
fn test_qoi_incremental_run_heavy_image() {
    // 100x100 solid color (stretches run logic across row boundaries)
    let solid = vec![42u8, 84, 126, 255].repeat(100 * 100);
    let encoded = zenbitmaps::encode_qoi(
        &solid,
        100,
        100,
        zenbitmaps::PixelLayout::Rgba8,
        Unstoppable,
    )
    .unwrap();

    let ground_truth = zenbitmaps::decode_qoi(&encoded, Unstoppable).unwrap();

    for &cs in &[encoded.len(), 50, 7, 1] {
        let (w, h, pixels) = decode_incremental_in_chunks(&encoded, cs).unwrap();
        assert_eq!(w, 100);
        assert_eq!(h, 100);
        assert_eq!(pixels, ground_truth.pixels(), "mismatch with chunk size {cs}");
    }
}

#[test]
fn test_qoi_incremental_rgb_1x1() {
    let px = [10u8, 20, 30];
    let encoded = zenbitmaps::encode_qoi(&px, 1, 1, zenbitmaps::PixelLayout::Rgb8, Unstoppable).unwrap();
    let ground_truth = zenbitmaps::decode_qoi(&encoded, Unstoppable).unwrap();

    for &cs in &[encoded.len(), 14, 5, 1] {
        let (w, h, pixels) = decode_incremental_in_chunks(&encoded, cs).unwrap();
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        assert_eq!(pixels, ground_truth.pixels(), "mismatch with chunk size {cs}");
    }
}

#[test]
fn test_qoi_incremental_truncation_detection() {
    let synth: Vec<u8> = (0..32 * 32)
        .flat_map(|i| [i as u8, (i * 2) as u8, (i * 3) as u8, 255])
        .collect();
    let encoded = zenbitmaps::encode_qoi(&synth, 32, 32, zenbitmaps::PixelLayout::Rgba8, Unstoppable).unwrap();

    // Truncate at header
    let job = QoiDecoderConfig::new().job();
    let mut dec = job.into_incremental_decoder(&[]).unwrap();
    let outcome = dec.push_chunk(&encoded[..10], true, None);
    assert!(outcome.is_err(), "truncation at header must error with is_eof=true");

    // Truncate mid-body
    let job = QoiDecoderConfig::new().job();
    let mut dec = job.into_incremental_decoder(&[]).unwrap();
    let mid = encoded.len() / 2;
    let _ = dec.push_chunk(&encoded[..mid], false, None).unwrap();
    while let Some(_) = dec.pull_batch().unwrap() {}
    let outcome = dec.push_chunk(&[], true, None);
    assert!(outcome.is_err(), "truncation mid-body must error with is_eof=true");
}

#[test]
fn test_qoi_dyn_incremental_decoder() {
    use zencodec::decode::DecodeJob as _;

    let synth: Vec<u8> = (0..16 * 16)
        .flat_map(|i| [i as u8, 128, 255 - i as u8, 255])
        .collect();
    let encoded = zenbitmaps::encode_qoi(&synth, 16, 16, zenbitmaps::PixelLayout::Rgba8, Unstoppable).unwrap();

    let job = QoiDecoderConfig::new().job();
    let mut dyn_dec = job.dyn_incremental_decoder(&[]).unwrap();

    let mut pixels = Vec::new();
    let mut offset = 0;
    while !dyn_dec.is_complete() {
        let chunk = &encoded[offset..];
        let outcome = dyn_dec.push_chunk(chunk, true, None).unwrap();
        offset += outcome.bytes_consumed;
        while let Some((_y, strip)) = dyn_dec.pull_batch().unwrap() {
            pixels.extend_from_slice(&strip.contiguous_bytes());
        }
    }

    assert_eq!(dyn_dec.info().unwrap().width, 16);
    assert_eq!(dyn_dec.info().unwrap().height, 16);
    assert_eq!(pixels, synth);
}

