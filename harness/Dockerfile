from rust:1.94.1-slim as builder

run cargo install cargo-watch

copy Cargo.toml /source/Cargo.toml
copy Cargo.lock /source/Cargo.lock
copy src /source/src

run cd /source && cargo build --release

cmd cd /source && cargo watch -w ./ -w /tools -x run

from debian:stable-slim 

copy --from=olivia/tools:0.0.0-dev /tools /tools
copy --from=builder /source/target/release/harness /harness

cmd /harness
