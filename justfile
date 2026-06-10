# Format + regenerate the public-API surface snapshot (docs/public-api/)
fmt:
    cargo fmt
    cargo test -p zenbitmaps --test public_api_doc

# Regenerate the public-API surface snapshot only
api-doc:
    cargo test -p zenbitmaps --test public_api_doc

# Verify the committed snapshot is current (what CI runs)
api-doc-check:
    ZEN_API_DOC=check cargo test -p zenbitmaps --test public_api_doc

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
