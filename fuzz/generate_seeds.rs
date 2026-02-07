#!/usr/bin/env -S cargo +nightly -Zscript
//! Generate seed corpus files for fuzzing.
//! Run: cargo +nightly -Zscript fuzz/generate_seeds.rs

fn main() {
    use std::fs;
    let dir = "fuzz/corpus/fuzz_decode";
    fs::create_dir_all(dir).unwrap();

    // PPM 2x2
    let ppm = b"P6\n2 2\n255\n\xff\x00\x00\x00\xff\x00\x00\x00\xff\x80\x80\x80";
    fs::write(format!("{dir}/ppm_2x2.ppm"), ppm).unwrap();

    // PGM 3x2
    let pgm = b"P5\n3 2\n255\n\x00\x40\x80\xc0\xff\x64";
    fs::write(format!("{dir}/pgm_3x2.pgm"), pgm).unwrap();

    // PAM RGBA 1x1
    let pam = b"P7\nWIDTH 1\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n\xff\x00\x00\xff";
    fs::write(format!("{dir}/pam_rgba_1x1.pam"), pam).unwrap();

    // PFM gray 1x1
    let mut pfm = b"Pf\n1 1\n-1.0\n".to_vec();
    pfm.extend_from_slice(&1.0f32.to_le_bytes());
    fs::write(format!("{dir}/pfm_gray_1x1.pfm"), pfm).unwrap();

    // Minimal BMP 1x1 24-bit
    let mut bmp = vec![0u8; 58]; // 54 header + 4 pixel (3 + 1 padding)
    bmp[0] = b'B'; bmp[1] = b'M';
    bmp[2..6].copy_from_slice(&58u32.to_le_bytes()); // file size
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes()); // data offset
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes()); // DIB header size
    bmp[18..22].copy_from_slice(&1i32.to_le_bytes()); // width
    bmp[22..26].copy_from_slice(&1i32.to_le_bytes()); // height
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes()); // planes
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes()); // bpp
    bmp[54] = 0xff; bmp[55] = 0x00; bmp[56] = 0x00; // BGR
    fs::write(format!("{dir}/bmp_1x1.bmp"), bmp).unwrap();

    // Truncated/malformed seeds for edge coverage
    fs::write(format!("{dir}/empty.bin"), b"").unwrap();
    fs::write(format!("{dir}/just_p6.bin"), b"P6").unwrap();
    fs::write(format!("{dir}/bm_short.bin"), b"BM\x00\x00").unwrap();
    fs::write(format!("{dir}/p7_no_endhdr.bin"), b"P7\nWIDTH 1\nHEIGHT 1\n").unwrap();

    println!("Generated seed corpus in {dir}/");
}
