# Contributing to SeatTrellis

Thank you for helping improve SeatTrellis / 席序.

## Development Setup

SeatTrellis v2 is a Rust workspace with a React workbench. Prerequisites: Rust 1.88+ (MSRV), Node 20+.

```bash
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis

# The server embeds the React workbench, so build it first
cd clients/web && npm ci && npm run build && cd ../..

cargo build
cargo test --workspace
```

## Running Tests

```bash
cargo test --workspace                 # all crates
cargo clippy --all-targets --workspace -- -D warnings
cargo run -p xtask -- contract check   # generated schema/API contract drift check
cd clients/web && npm test && npm run typecheck
```

Please add or update tests for any new rule, importer, exporter, or CLI behavior. Every fix should carry a regression test.

## Code Style

- `cargo fmt` before committing; clippy must be clean with `-D warnings`.
- MSRV is Rust 1.88 — avoid newer std APIs.
- Comments and commit messages in English; user-facing docs are bilingual (zh/en).
- Keep the loopback security contract intact: new write paths must not bypass the token / Host / Origin middleware.

## Release Process

See `docs/publishing.md` and `docs/release-checklist.md`. Releases are cut from `main` for the v2 line; the v1 Python line is maintained on `v1.x-maintenance` (frozen at 1.9.0).
