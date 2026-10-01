# seattrellis-server

Loopback HTTP transport, security middleware, and embedded frontend hosting for [SeatTrellis (席序)](https://github.com/FrankFu916/seattrellis).

---

## 🌐 Features

- **Local Loopback Transport**: Axum-based HTTP server bound exclusively to `127.0.0.1`.
- **Security Middleware**: 256-bit bearer session tokens, Host header validation, and CSRF/Origin enforcement.
- **Embedded Frontend**: Packages and serves the React 19 web workbench directly from binary memory with zero external asset dependencies. Build inputs are snapshotted under Cargo's `OUT_DIR`, so later frontend builds cannot remove assets needed by rustdoc.
- **Bounded Lifecycle**: Authentication precedes body reading. Bodies have a 64 MiB individual and 128 MiB aggregate budget, with 64 work slots, 128 connections, and 15-second header/body read deadlines. Cancellation bypasses occupied slots. Shutdown handles Ctrl-C, SIGTERM, and the native-shell flag, with a five-second grace period.
- **Shared Class Documents**: `classes/document/serialize` captures expected revisions; `classes/document/open` validates every source, assignment, and lock before publishing the batch. The GUI handles file dialogs or browser upload/download. Reopened drafts support editing, audit, repair, and export through the same Rust use cases.

Long solves send `X-Request-Id` and cancel with `POST /api/v1/jobs/{request_id}/cancel`. The solver checks a cooperative control, and cancelled requests remove every draft/source context they created. A configured solve deadline includes its requested budget plus 15 seconds; rotation budgets multiply by the number of periods. Other operations default to 120 seconds, and transport budgets are capped at one day.

Sources and editors are inserted and evicted together under a fixed lock order. A candidate or rotation set publishes atomically, and retained source documents have a 128 MiB aggregate budget. Portable documents retain the full source roster and rules, assignments, and independent student/seat locks; intermediate incomplete edits remain editable and cannot be exported as valid solved plans.

---

## 📄 License

Licensed under [Apache-2.0](../../LICENSE).
