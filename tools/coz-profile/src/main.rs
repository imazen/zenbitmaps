//! Farbfeld whole-operation profiling and external-process timing worker.
#![forbid(unsafe_code)]

use enough::Unstoppable;
use std::hint::black_box;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;
use zenbitmaps::PixelLayout;

fn sha256(data: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    // Validation is outside timing. Do not inject Coz into this child, which
    // could otherwise write a second process's profile to the same output.
    let mut child = Command::new("sha256sum")
        .env_remove("LD_PRELOAD")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let written = child.stdin.take().expect("sha256 stdin").write_all(data);
    let output = child.wait_with_output()?;
    written?;
    assert!(output.status.success(), "sha256sum failed");
    let text = String::from_utf8(output.stdout)?;
    let digest = text.split_whitespace().next().expect("sha256 digest");
    assert!(digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()));
    Ok(digest.to_owned())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    coz::thread_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        (5..=6).contains(&args.len()),
        "mode layout width height iterations [dump-path]"
    );
    let encoding = match args[0].as_str() {
        "encode" => true,
        "decode" => false,
        _ => panic!("mode must be encode or decode"),
    };
    let layout = match args[1].as_str() {
        "rgba8" => PixelLayout::Rgba8,
        "rgb8" => PixelLayout::Rgb8,
        "bgra8" => PixelLayout::Bgra8,
        "bgrx8" => PixelLayout::Bgrx8,
        "bgr8" => PixelLayout::Bgr8,
        "gray8" => PixelLayout::Gray8,
        "rgba16" => PixelLayout::Rgba16,
        _ => panic!("unsupported input layout"),
    };
    let width: u32 = args[2].parse()?;
    let height: u32 = args[3].parse()?;
    let iterations: usize = args[4].parse()?;
    assert!(width > 0 && height > 0 && iterations > 0);
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(layout.bytes_per_pixel()))
        .expect("input size overflow");
    // Deterministic synthetic byte input, not a photograph or quality dataset.
    let mut state = 0x7942_c31bu32;
    let pixels: Vec<u8> = (0..bytes)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect();
    let encoded = zenbitmaps::encode_farbfeld(&pixels, width, height, layout, Unstoppable)?;
    let run_once = || {
        if encoding {
            let output =
                zenbitmaps::encode_farbfeld(black_box(&pixels), width, height, layout, Unstoppable)
                    .expect("encode");
            black_box(output.as_slice());
        } else {
            let output =
                zenbitmaps::decode_farbfeld(black_box(&encoded), Unstoppable).expect("decode");
            black_box(output.pixels());
        }
        coz::progress!("farbfeld-operation");
    };
    for _ in 0..5 {
        run_once();
    }
    for _ in 0..3 {
        let start = Instant::now();
        for _ in 0..iterations {
            run_once();
        }
        println!("elapsed_ns={}", start.elapsed().as_nanos());
    }
    // Full output bytes are checked outside the measured batches.
    let result = if encoding {
        zenbitmaps::encode_farbfeld(&pixels, width, height, layout, Unstoppable)?
    } else {
        zenbitmaps::decode_farbfeld(&encoded, Unstoppable)?
            .pixels()
            .to_vec()
    };
    let digest = sha256(&result)?;
    println!(
        "result={{\"width\":{width},\"height\":{height},\"input_layout\":\"{}\",\"bytes\":{},\"sha256\":\"{digest}\"}}",
        args[1],
        result.len()
    );
    if let Some(path) = args.get(5) {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .write_all(&result)?;
    }
    Ok(())
}
