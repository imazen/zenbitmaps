//! Compare MAXVAL interpretation with an independent Netpbm reader.
//! This is a diagnostic: disagreements are printed and return a failure.
use enough::Unstoppable;
use std::io::Write;
use std::process::{Command, Stdio};
use zenbitmaps::{PixelLayout, decode};

fn main() {
    let samples = [0_u16, 100, 512, 1023];
    let data: Vec<u8> = samples.iter().flat_map(|v| v.to_be_bytes()).collect();
    let fixtures = [
        ("P5", [b"P5\n4 1\n1023\n".as_slice(), &data].concat()),
        (
            "P7",
            [
                b"P7\nWIDTH 4\nHEIGHT 1\nDEPTH 1\nMAXVAL 1023\nTUPLTYPE GRAYSCALE\nENDHDR\n"
                    .as_slice(),
                &data,
            ]
            .concat(),
        ),
    ];
    let mut disagreements = 0;
    for (name, bytes) in fixtures {
        let decoded = decode(&bytes, Unstoppable).expect("native decode");
        assert_eq!(decoded.layout, PixelLayout::Gray16);
        let native: Vec<u16> = decoded
            .pixels()
            .chunks_exact(2)
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
            .chunks_exact(2)
            .map(|v| u16::from_ne_bytes([v[0], v[1]]))
            .collect();
        assert_eq!(independent.len(), samples.len());
        println!("{name} MAXVAL=1023 native={native:?} netpbm={independent:?}");
        disagreements += usize::from(native != independent);
    }
    assert_eq!(disagreements, 0, "MAXVAL interpretation disagrees");
}
