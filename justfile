# Format + regenerate the public-API surface snapshots (docs/public-api/).
# The snapshot runner lives in the standalone apidoc/ package, so it is never
# built or run by plain `cargo test` or any CI job.
fmt:
    cargo fmt
    cargo test --manifest-path apidoc/Cargo.toml

# Regenerate the public-API surface snapshots only
api-doc:
    cargo test --manifest-path apidoc/Cargo.toml

# Verify the committed snapshots are current
api-doc-check:
    ZEN_API_DOC=check cargo test --manifest-path apidoc/Cargo.toml

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

# Six format families, exact encoded bytes and pixels across runtime tiers.
arm-codec-audit filter="":
    CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 OMP_NUM_THREADS=4 TMPDIR="$HOME/tmp" nice -n 19 cargo bench --bench codecs --all-features -- --group="{{filter}}"

# Cross-check the inventories against exiftool and ImageMagick (needs both installed).
inventory-oracle:
    INVENTORY_ORACLE_EXIFTOOL=$(command -v exiftool) INVENTORY_ORACLE_MAGICK=$(command -v magick) \
        cargo test --features all,inventory --test inventory_oracle -- --ignored --nocapture

metadata-check:
    cargo test --features all,inventory --test inventory --test inventory_corpus --test inventory_mem --test inventory_review --test inventory_review2 --test inventory_regression --test inventory_feature
    cargo clippy --features all,inventory --lib --tests -- -D warnings
    cargo check --no-default-features --features inventory

metadata-feature-check:
    cargo test --features all,zencodec --test inventory_feature
    cargo test --features all,inventory --test inventory_feature

metadata-portability:
    CARGO_TARGET_I686_UNKNOWN_LINUX_GNU_RUNNER=env cargo test --target i686-unknown-linux-gnu --features all,inventory --test inventory --test inventory_corpus --test inventory_review --test inventory_review2 --test inventory_regression --test inventory_feature
    cargo check --manifest-path fuzz/Cargo.toml --bin inventory
    just api-doc

metadata-api-check:
    #!/usr/bin/env bash
    set -euo pipefail
    baseline=$(cargo read zenbitmaps==0.1.5 --path-only)
    RUSTC_BOOTSTRAP=1 cargo rustdoc --lib --features all,inventory -- -Zunstable-options --output-format json
    RUSTC_BOOTSTRAP=1 cargo rustdoc --manifest-path "$baseline/Cargo.toml" --target-dir "$PWD/target/published-api/0.1.5" --lib --all-features -- -Zunstable-options --output-format json
    cargo semver-checks check-release --current-rustdoc target/doc/zenbitmaps.json --baseline-rustdoc target/published-api/0.1.5/doc/zenbitmaps.json --release-type patch

metadata-cancellation-check:
    cargo test --features all,inventory --test inventory_feature

metadata-traversal-stop-check:
    cargo test --features all,inventory --test inventory_review2 long_inventory_walks_retain_one_shot_timeout -- --exact
    just metadata-check

metadata-cross-corpus-check:
    cargo test --features all,inventory --test inventory_corpus -- --test-threads=1
    actionlint

# Native-pixel policy and inventory checks for all six adapters.
shared-contract-check:
    cargo test --features all,inventory --test shared_contracts
    cargo test --features all,zencodec --test shared_contracts
    cargo clippy --features all,inventory --test shared_contracts -- -D warnings

# QOI through zencodec's sans-I/O contract vs the one-shot paths.
# Capped at 2 s per group (27 groups); narrow with `just bench-qoi-sansio GROUP`.
bench-qoi-sansio filter="":
    CARGO_BUILD_JOBS=4 nice -n 19 cargo bench --bench sansio_qoi --features all,zencodec,std -- --cpu-policy=unrestricted {{ if filter == "" { "" } else { "--group=" + filter } }}
