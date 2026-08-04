from rust:1.94.1-slim as base

run rustup target add wasm32-wasip2
run cargo install cargo-watch

from base as tools

run cargo install cargo-component --locked

run mkdir -p /tools

copy tools /source/tools
workdir /source/tools

run cargo component build --release --target=wasm32-wasip2
run cp -rfv ./target/wasm32-wasip2/release/*.wasm /tools

cmd cargo watch -x 'component build --target=wasm32-wasip2' -s 'cp -rfv ./target/wasm32-wasip2/debug/*.wasm /tools'

from base as builder

copy harness /source/harness
copy tools/tool.wit /source/tools/tool.wit
workdir /source/harness

run cargo build --release

cmd cargo watch -w . -w /tools -x 'run'

from debian:stable-slim

copy --from=tools /tools /tools
copy --from=builder /target/release/harness /harness

cmd /harness
