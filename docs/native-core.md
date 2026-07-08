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

Local Rust checks:

```bash
cargo test --manifest-path native/Cargo.toml
```

Build the Python extension for local development:

```bash
python -m pip install maturin
python -m maturin develop --manifest-path native/seattrellis_native/Cargo.toml --features extension-module
```

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
