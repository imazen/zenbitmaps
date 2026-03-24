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

feature-check:
    cargo check --no-default-features
    cargo check --features std
    cargo check --features bmp
    cargo check --features rgb
    cargo check --features imgref
    cargo check --features zencodec
    cargo test --no-default-features
    cargo test --features bmp
    cargo test --features zencodec
    cargo test --all-features

ci: fmt clippy test feature-check

check-no-std:
    cargo check --no-default-features --target wasm32-unknown-unknown

test-i686:
    cross test --all-features --target i686-unknown-linux-gnu

test-armv7:
    cross test --all-features --target armv7-unknown-linux-gnueabihf

test-cross: test-i686 test-armv7
