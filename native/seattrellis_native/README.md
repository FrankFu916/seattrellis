# SeatTrellis native core

This crate is the Python binding for the experimental Rust core in
`../seattrellis_core`. It is not the default solver and does not replace the
Python fallback or Python OR-Tools backend.

Local development:

```bash
python -m pip install maturin
python -m maturin develop --manifest-path native/seattrellis_native/Cargo.toml --features extension-module
seattrellis solve --backend native ...
```

Rust-only checks:

```bash
cargo test --manifest-path native/Cargo.toml
```

In v1.4 the `native` backend still delegates search to the Python fallback
solver, then uses this Rust extension for structural assignment checks. The
point of the spike is to prove packaging and the Python/Rust call boundary
before moving heavier validation, scoring, and heuristic work into Rust.
