check:
    cargo check --all-features

fmt:
    cargo fmt

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-features

build:
    cargo build --all-features

doc:
    cargo doc --all-features --no-deps

check-no-std:
    cargo check --no-default-features --target wasm32-unknown-unknown

test-i686:
    cross test --all-features --target i686-unknown-linux-gnu

test-armv7:
    cross test --all-features --target armv7-unknown-linux-gnueabihf

test-cross: test-i686 test-armv7
