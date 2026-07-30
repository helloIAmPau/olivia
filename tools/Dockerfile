from rust:1.94.1-slim

run rustup target add wasm32-unknown-unknown
run cargo install cargo-watch

copy --exclude=target/ --exclude=Dockerfile . /source
run mkdir -p /tools && cd /source && cargo build --release --target wasm32-unknown-unknown && cp -rfv ./target/wasm32-unknown-unknown/release/*.wasm /tools

cmd cd /source && cargo watch -x 'build --target wasm32-unknown-unknown' -s 'cp -rfv ./target/wasm32-unknown-unknown/debug/*.wasm /tools'
