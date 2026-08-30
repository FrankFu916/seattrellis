# Contributing to SeatTrellis / 席序贡献指南

Thank you for your interest in improving **SeatTrellis (席序)**! We welcome contributions from developers, designers, educators, and translators.

---

## 🛠️ Development Setup

SeatTrellis v2 is structured as a modular Rust workspace paired with a React 19 web workbench.

### Prerequisites
- **Rust Toolchain**: 1.88+ (MSRV);
- **Node.js & npm**: Node.js 22.12+, npm 10+ (for building the frontend).

### Clone & Build
```bash
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis

# 1. Build the React workbench (embedded into the server binary)
cd clients/web && npm ci && npm run build && cd ../..

# 2. Build and run tests
cargo build --workspace
cargo test --workspace
```

---

## 🧪 Testing & Quality Gates

Before submitting a Pull Request, ensure that all local quality checks pass cleanly:

```bash
# 1. Run all Rust unit and integration tests
cargo test --workspace

# 2. Run Clippy static analysis
cargo clippy --all-targets --workspace -- -D warnings

# 3. Check for OpenAPI & TypeScript contract drift
cargo run -p xtask -- contract check

# 4. Run frontend tests and type checks
cd clients/web && npm test && npm run typecheck && cd ../..
```

---

## 📐 Code Style & Conventions

- **Formatting**: Run `cargo fmt` prior to committing.
- **MSRV**: Maintain compatibility with Rust 1.88 (avoid newer standard library features).
- **Security Middleware**: Maintain the local-first security boundary. New write endpoints must be protected with bearer token, Host, and Origin checks.
- **Documentation & Commits**: Write commit messages and code comments in English; user-facing guides are maintained bilingually (English / 简体中文).

---

## 🚀 Release Process

For detailed release workflows, see [Publishing Guide](docs/publishing.md) and [Release Checklist](docs/release-checklist.md).
