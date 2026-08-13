# Rust core

SeatTrellis v2 是纯 Rust 实现：`seattrellis_core` 是唯一的语义真相，负责规则
编译、合法性校验、编辑状态机、migration、隐私和求解状态；CLI、loopback App
服务器、Tauri 桌面壳和 React 工作台都建立在这套 core 之上。

历史说明：v1 迁移期间曾有一个 PyO3 临时兼容扩展（`seattrellis_native`，通过
`--backend native` 选择），它只在 Python 兼容路径中作为可选的验证/评分实验，
从未作为默认求解器。v2 迁移完成后该扩展已退役，v2 树中不再包含 Python 集成；
`native/` 目录与 PyO3 crate 在 v2 final 前已删除。

当前运行时（无 Python、无 Node.js、无 OR-Tools）：

- `seattrellis_cli`：独立求解 + 导出工具，28 个子命令；
- `seattrellis_app`：只绑定 `127.0.0.1` 的 loopback HTTP 服务，嵌入 React
  工作台资源，为浏览器和 Tauri 壳提供本地 API；
- `app/src-tauri/`：Tauri 2 桌面壳。

构建与测试：

```bash
cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis_cli
cargo clippy --all-targets -p seattrellis_core -p seattrellis_cli -- -D warnings
```

v1（Python）行冻结在 1.9.0，由 `v1.x-maintenance` 分支维护，只作为行为基准的
oracle；它不影响 v2 的构建、运行或分发。
