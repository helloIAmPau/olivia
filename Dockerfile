from rust:1.94.1-slim as base

run cargo install cargo-watch

env CARGO_TARGET_DIR=/target
workdir /source

copy Cargo.toml ./Cargo.toml
copy harness ./harness
copy tools ./tools

from base as tools

run rustup target add wasm32-unknown-unknown

run mkdir -p /tools \
 && cargo build --release -p exec --target wasm32-unknown-unknown \
 && cp -rfv /target/wasm32-unknown-unknown/release/*.wasm /tools

cmd cargo watch -w tools -x 'build -p exec --target wasm32-unknown-unknown' -s 'cp -rfv /target/wasm32-unknown-unknown/debug/*.wasm /tools'

from base as builder

run cargo build --release -p harness

cmd cargo watch -w harness -w tools/common -x 'run -p harness'

from debian:stable-slim

copy --from=tools /tools /tools
copy --from=builder /target/release/harness /harness

cmd /harness
