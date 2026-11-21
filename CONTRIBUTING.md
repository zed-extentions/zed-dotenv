# Contributing

Report issues and propose changes in this repository. Maintainers review contributions before incorporating them into a release.

## Development

Install Rust and the `wasm32-wasip2` target, then run:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --target wasm32-wasip2 --release
```

Keep domain logic independent of `zed_extension_api` and add tests for behavior changes. Do not include credentials or real secret values in examples or tests.
