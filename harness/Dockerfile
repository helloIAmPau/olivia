from rust:1.94.1-slim as builder

run cargo install cargo-watch

copy Cargo.toml /source/Cargo.toml
copy Cargo.lock /source/Cargo.lock
copy src /source/src

run cd /source && cargo build --release

cmd cd /source && cargo watch -x run

from debian:stable-slim 

copy --from=builder /source/target/release/harness /harness

cmd /harness
