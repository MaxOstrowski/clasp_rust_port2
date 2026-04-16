# rust_clasp

`rust_clasp` is an in-progress Rust port of
[clasp](https://potassco.org/clasp/), the Potassco answer set solver.
The upstream C++ implementation under `original_clasp/` remains the behavioral
reference for algorithms, CLI semantics, output, and tests.

## Current Status

The repository is still in the early porting phase.

## Workspace Layout


## Development

The bootstrap workspace is validated with the standard Rust toolchain:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
