# Rust native core and Python compatibility extension

SeatTrellis now has a Rust-first compact desktop runtime. The `app/` and Tauri
paths use `seattrellis_core` directly and are covered in
[the migration guide](rust-migration.md). Python 1.x remains the compatibility
and library path during the staged migration.

This document specifically describes the separate `seattrellis_native` PyO3
extension. It is still an optional Python-side experiment and is not the same
thing as the standalone Rust App or CLI.

Current behavior:

- `--backend native` is explicit opt-in;
- normal `auto`, `fallback`, and `ortools` flows do not require Rust;
- the native backend currently delegates search to Python fallback, then uses
  one versioned, identity-free DTO to precompute graph distances, verify
  assignment structure and hard rules, and calculate a peer-mixing score;
- if the extension is not installed, SeatTrellis reports a clear missing native
  backend error instead of silently falling back.

## Installation status

The experimental `seattrellis_native` extension is not bundled with the main
`seattrellis` wheel and is not currently published by this project as a
separate wheel. There is no `native` runtime extra. PyPI users should use
`auto`, `fallback`, or `ortools`; do not select `native` unless the compatibility
check below succeeds.

Local Rust checks:

```bash
cargo test --manifest-path native/Cargo.toml
```

To evaluate the extension, activate a virtual environment in a matching source
checkout and run:

```bash
python -m pip install -e .
python -m pip install "maturin>=1.14.1,<2"
python -m maturin develop --release --manifest-path native/seattrellis_native/Cargo.toml --features extension-module
python -c "from seattrellis.solver.native import require_native_core; print(require_native_core().NATIVE_API_VERSION)"
seattrellis doctor
```

`maturin develop` requires an active virtual environment. The Rust target must
match the Python interpreter architecture; for example, an Apple Silicon Python
requires an `aarch64-apple-darwin` Rust toolchain.
`doctor` reads package metadata without loading the extension into its own
process; the explicit compatibility command above performs the API check.

Then run:

```bash
seattrellis solve \
  --students examples/students.csv \
  --layout examples/classroom.json \
  --rules examples/rules.json \
  --backend native
```

Pull requests build and install experimental wheels on Linux, Windows, and
macOS at the supported Python range boundaries, 3.11 and 3.14. Ubuntu also
checks Python 3.12 and 3.13. Those wheels are retained as short-lived CI
artifacts for inspection; they are not release assets and are not supported as
public binary distributions yet. The native workspace has a Rust 1.83 MSRV;
current stable Rust remains the recommended development toolchain.

The native wheel contract runs Python/Rust differential checks for hard rules,
graph topology and peer-mixing scoring. The v1.4 decision is to continue this
work as an optional validator and precompute experiment, but not make it a
default solver: the current search still runs in Python and therefore cannot
yet demonstrate the required end-to-end speedup on 40/50/60-student cases.
