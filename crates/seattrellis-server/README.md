# seattrellis-server

Loopback HTTP transport, security middleware, and embedded frontend hosting for [SeatTrellis (席序)](https://github.com/FrankFu916/seattrellis).

---

## 🌐 Features

- **Local Loopback Transport**: Axum-based HTTP server bound exclusively to `127.0.0.1`.
- **Security Middleware**: 256-bit bearer session tokens, Host header validation, and CSRF/Origin enforcement.
- **Embedded Frontend**: Packages and serves the React 19 web workbench directly from binary memory with zero external asset dependencies.

---

## 📄 License

Licensed under [Apache-2.0](../../LICENSE).
