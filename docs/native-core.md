# 原生 Rust 核心与求解引擎（Native Core）

[English](native-core.md) · [简体中文](native-core.md)

**席序（SeatTrellis）v2.0.0** 的全部核心算法、数据校验、状态迁移与格式渲染均由原生 Rust 独立实现。

---

## ⚡ 1. 架构定位

`seattrellis_core` 是整个系统的单一业务真相源（Single Source of Truth）：
- **规则编译器**：负责将高层 JSON 规则编译为高效的图论约束与代价矩阵；
- **回溯搜索与局部搜索**：毫秒级完成约束剪枝、启发式求解与多候选生成；
- **合规性独立复核**：对任何输出或微调方案执行全量硬约束检验，确保 100% 合规。

---

## 🚫 2. 彻底脱离 Python / 外部运行时

- **纯 Rust 编译**：v2.0.0 彻底移除了对 Python、OR-Tools、PyO3 桥接层及 Node.js 的依赖；
- **全平台原生分发**：以单个静态二进制或轻量系统安装包的形式运行在 macOS、Windows 和 Linux 上。

---

## 🧪 3. 核心测试与验证

```bash
# 运行 core 核心测试
cargo test --locked -p seattrellis_core

# 运行全量工作区静态检查
cargo clippy --all-targets --workspace -- -D warnings
```

---

## 📖 相关文档

- [系统架构解析](architecture.md)
- [开发与测试指南](development.md)
