//! Compare MAXVAL interpretation with an independent Netpbm reader.
//! This is a diagnostic: disagreements are printed and return a failure.
use enough::Unstoppable;
use std::io::Write;
use std::process::{Command, Stdio};
use zenbitmaps::{PixelLayout, decode};

fn main() {
    let samples = [0_u16, 100, 512, 1023];
    let data: Vec<u8> = samples.iter().flat_map(|v| v.to_be_bytes()).collect();
    let rgb: Vec<u8> = [100_u16, 512, 1023]
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect();
    let rgba: Vec<u8> = [100_u16, 512, 1023, 256]
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect();
    let fixtures = [
        (
            "P2",
            b"P2\n4 1\n1023\n0 100 512 1023\n".to_vec(),
            PixelLayout::Gray16,
        ),
        (
            "P3",
            b"P3\n1 1\n1023\n100 512 1023\n".to_vec(),
            PixelLayout::Rgb16,
        ),
        (
            "P5",
            [b"P5\n4 1\n1023\n".as_slice(), &data].concat(),
            PixelLayout::Gray16,
        ),
        (
            "P6",
            [b"P6\n1 1\n1023\n".as_slice(), &rgb].concat(),
            PixelLayout::Rgb16,
        ),
        (
            "P7 gray",
            [
                b"P7\nWIDTH 4\nHEIGHT 1\nDEPTH 1\nMAXVAL 1023\nTUPLTYPE GRAYSCALE\nENDHDR\n"
                    .as_slice(),
                &data,
            ]
            .concat(),
            PixelLayout::Gray16,
        ),
        (
            "P7 RGB",
            [
                b"P7\nWIDTH 1\nHEIGHT 1\nDEPTH 3\nMAXVAL 1023\nTUPLTYPE RGB\nENDHDR\n".as_slice(),
                &rgb,
            ]
            .concat(),
            PixelLayout::Rgb16,
        ),
        (
            "P7 RGBA",
            [
                b"P7\nWIDTH 1\nHEIGHT 1\nDEPTH 4\nMAXVAL 1023\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
                    .as_slice(),
                &rgba,
            ]
            .concat(),
            PixelLayout::Rgba16,
        ),
    ];
    let mut disagreements = 0;
    for (name, bytes, layout) in fixtures {
        let decoded = decode(&bytes, Unstoppable).expect("native decode");
        assert_eq!(decoded.layout, layout);
        let native: Vec<u16> = decoded
            .pixels()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_ne_bytes([v[0], v[1]]))
            .collect();
        let mut oracle =
            Command::new(std::env::var_os("PAMDEPTH").unwrap_or_else(|| "pamdepth".into()))
                .arg("65535")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Netpbm pamdepth must be installed");
        oracle.stdin.take().unwrap().write_all(&bytes).unwrap();
        let output = oracle.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let normalized = decode(&output.stdout, Unstoppable).expect("Netpbm output decode");
        let independent: Vec<u16> = normalized
            .pixels()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_ne_bytes([v[0], v[1]]))
            .collect();
        assert_eq!(independent.len(), native.len());
        println!("{name} MAXVAL=1023 native={native:?} netpbm={independent:?}");
        disagreements += usize::from(native != independent);
    }
    assert_eq!(disagreements, 0, "MAXVAL interpretation disagrees");
    #[cfg(feature = "zencodec")]
    native_deep_encode();
}

/// Read native encoder output as numeric samples using Netpbm alone.
#[cfg(feature = "zencodec")]
fn native_deep_encode() {
    use zencodec::encode::{EncodeJob as _, Encoder as _, EncoderConfig as _};
    use zenpixels::{PixelDescriptor, PixelSlice};
    for descriptor in [
        PixelDescriptor::GRAY16_SRGB,
        PixelDescriptor::RGB16_SRGB,
        PixelDescriptor::RGBA16_SRGB,
    ] {
        let words: Vec<u16> = (0..3 * descriptor.channels())
            .map(|i| (1 + i * 997) as u16)
            .collect();
        let bytes: Vec<u8> = words.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let pixels = PixelSlice::new(&bytes, 3, 1, bytes.len(), descriptor).unwrap();
        let encoded = zenbitmaps::PnmEncoderConfig::new()
            .job()
            .encoder()
            .unwrap()
            .encode(pixels)
            .unwrap();
        let mut oracle =
            Command::new(std::env::var_os("PAMTABLE").unwrap_or_else(|| "pamtable".into()))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Netpbm pamtable must be installed");
        oracle
            .stdin
            .take()
            .unwrap()
            .write_all(encoded.data())
            .unwrap();
        let result = oracle.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let independent: Vec<u16> = std::str::from_utf8(&result.stdout)
            .unwrap()
            .split(|c: char| !c.is_ascii_digit())
            .filter(|v| !v.is_empty())
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(independent, words, "{descriptor:?}");
        println!(
            "native {:?} encode: Netpbm exact {} samples",
            descriptor.layout(),
            words.len()
        );
    }
}
