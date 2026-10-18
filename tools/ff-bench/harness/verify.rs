//! Standalone out-of-timer correctness gate: runs the exact same
//! `preflight()` the bench binary runs in every fresh process — complete
//! old==new byte comparison and the spec oracle on every workload cell,
//! including the h=0 structural probe and the decode control — and exports
//! the CAS artifacts when $FF_BENCH_ARTIFACT_DIR is set. Exits nonzero on
//! any mismatch (preflight panics; a clean run prints one PASS per cell).

#[path = "../common.rs"]
mod ff_bench;

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(
        std::env::var_os("FF_BENCH_ARTIFACT_DIR")
            .expect("FF_BENCH_ARTIFACT_DIR is required for retained correctness artifacts"),
    );
    for line in ff_bench::preflight(Some(dir)) {
        println!("VERIFY {line}");
    }
    println!("verify: all cells identical and spec-clean");
}
