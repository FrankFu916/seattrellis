# Rust native core

SeatTrellis keeps Python as the default runtime. The Rust native core is an
experimental v1.4 spike for low-level validation and future scoring/precompute
work. It does not replace the Python fallback solver or Python OR-Tools backend.

Current behavior:

- `--backend native` is explicit opt-in;
- normal `auto`, `fallback`, and `ortools` flows do not require Rust;
- the native backend currently delegates search to Python fallback, then uses
  the Rust extension to verify assignment structure;
- if the extension is not installed, SeatTrellis reports a clear missing native
  backend error instead of silently falling back.

## Installation status

The experimental `seattrellis_native` extension is not bundled with the main
`seattrellis` wheel and is not currently published by this project as a
separate wheel. There is no `native` runtime extra. PyPI users should use
`auto`, `fallback`, or `ortools`; do not select `native` unless
`import seattrellis_native` succeeds.

Local Rust checks:

```bash
cargo test --manifest-path native/Cargo.toml
```

To evaluate the extension, activate a virtual environment in a matching source
checkout and run:

```bash
python -m pip install -e .
python -m pip install "maturin>=1.8,<2"
python -m maturin develop --release --manifest-path native/seattrellis_native/Cargo.toml --features extension-module
python -c "import seattrellis_native; print(seattrellis_native.NATIVE_API_VERSION)"
seattrellis doctor
```

`maturin develop` requires an active virtual environment. The Rust target must
match the Python interpreter architecture; for example, an Apple Silicon Python
requires an `aarch64-apple-darwin` Rust toolchain.

Then run:

```bash
seattrellis solve \
  --students examples/students.csv \
  --layout examples/classroom.json \
  --rules examples/rules.json \
  --backend native
```

The native backend is not a release default until it proves a measurable benefit
on the 40/50/60-student benchmark suite and passes Python/Rust differential
tests for hard constraints and scoring.
