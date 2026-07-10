# SeatTrellis native core

This crate is the separately installable Python binding for the experimental
Rust core in `../seattrellis_core`. It is not the default solver and does not
replace the Python fallback or Python OR-Tools backend. Installing it adds the
`native` solver option; it does not replace the main `seattrellis` package.

Local development:

```bash
# Install the main application from the repository root.
python -m pip install -e .

# Build and install only the optional native extension.
python -m pip install "maturin>=1.8,<2"
python -m maturin develop --manifest-path native/seattrellis_native/Cargo.toml

# Verify that the application and extension coexist.
python -c "import seattrellis, seattrellis_native; print(seattrellis_native.__version__)"
seattrellis solve --backend native ...
```

Use a Rust compiler whose target architecture matches the Python interpreter.
For example, an Apple Silicon Python requires an `aarch64-apple-darwin` Rust
toolchain. `maturin develop` builds a local extension for the active Python;
it is not a release-wheel command.

Rust-only checks:

```bash
cargo test --manifest-path native/Cargo.toml
```

In v1.4 the `native` backend still delegates search to the Python fallback
solver, then uses this Rust extension for structural assignment checks. The
point of the spike is to prove packaging and the Python/Rust call boundary
before moving heavier validation, scoring, and heuristic work into Rust.
