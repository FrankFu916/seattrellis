# 开发者与本地构建指南（Development Guide）

[English](development.md) · [简体中文](development.md)

欢迎参与 **席序（SeatTrellis）** 项目的开发与贡献！本项目采用现代、严谨的工程化实践进行构建。

---

## 🛠️ 1. 环境准备

- **Rust 工具链**：MSRV 1.88+（推荐安装最新 Stable 版本）；
- **Node.js & npm**：仅用于前端开发与构建（Node.js 20+，npm 10+）；
- **开发操作系统**：macOS、Linux 或 Windows。

---

## 📦 2. 本地构建与全量测试

由于 `seattrellis-server` 在编译时会将前端静态产物（`clients/web/dist`）嵌入二进制中，因此在执行全工作区编译前，需先构建前端：

```bash
# 1. 编译 React 19 前端静态资源
cd clients/web && npm ci && npm run build && cd ../..

# 2. 运行核心 Crates 测试
cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis

# 3. 运行静态代码检查（Clippy）
cargo clippy --all-targets --workspace -- -D warnings

# 4. 运行前端类型检查与单元测试
cd clients/web && npm test && npm run typecheck && cd ../..

# 5. 校验 OpenAPI 契约与生成的 Schema 一致性
cargo run -p xtask -- contract check
```

---

## 📐 3. 开发架构约束准则

1. **Rust 为单一业务真相源**：所有约束判定、评分计算、状态机流转与安全策略必须在 Rust 核心层中实现，前端仅负责渲染与交互采集。
2. **所有导出与产物独立复核**：任何生成的方案快照或编辑操作，必须在产出前通过独立校验器，严禁硬编码 `valid: true`。
3. **严格的状态机与退出码契约**：严格遵循 7 种求解状态与 CLI 退出码规范（0/2/3/4/5/70/130），不可行判定必须有严格数学或剪枝证明。

---

## 📖 相关文档

- [系统架构解析](architecture.md)
- [测试与质量保障规范](testing.md)
- [代码贡献规范](https://github.com/FrankFu916/seattrellis/blob/main/CONTRIBUTING.md)
