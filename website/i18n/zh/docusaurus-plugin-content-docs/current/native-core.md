# 原生 Rust 核心与求解引擎（Native Core）


**席序（SeatTrellis）2.1.0** 使用共享 Rust 层实现算法、校验、状态迁移和格式渲染。CLI、服务、Tauri、网页工作台及 macOS 原生预览复用这些层。

---

## ⚡ 1. 架构定位

`seattrellis_core` 负责求解语义，domain、IO、export 和 application 各层分别负责编辑、持久化、渲染及用例编排：
- **规则编译器**：负责将高层 JSON 规则编译为高效的图论约束与代价矩阵；
- **回溯搜索与局部搜索**：完成约束剪枝、启发式求解与多候选生成，耗时取决于任务规模和限制；
- **合法性独立复核**：对输出或微调方案执行硬约束检验，不把搜索结果自动视为合法。

---

## 🚫 2. 彻底脱离 Python / 外部运行时

- **纯 Rust 编译**：v2.0.0 彻底移除了对 Python、OR-Tools、PyO3 桥接层及 Node.js 的依赖；
- **全平台原生分发**：以单个静态二进制或轻量系统安装包的形式运行在 macOS、Windows 和 Linux 上。
- **进程内桥接**：`seattrellis-native-bridge` 提供共享 [C ABI](native-bridge.md)，无需启动 HTTP 服务。
- **macOS 原生预览**：`clients/macos/` 使用 SwiftUI/AppKit 与该桥接；macOS CI 编译模型测试及未签名应用包，手工无障碍验收、签名及公证仍待完成。

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
