#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try decoding arbitrary bytes — must never panic
    let _ = zenpnm::decode(data, enough::Unstoppable);

    // Also fuzz ImageInfo probing
    let _ = zenpnm::ImageInfo::from_bytes(data);
});
