# 版本命名与兼容性规范（Versioning）

[English](versioning.md) · [简体中文](versioning.md)

**席序（SeatTrellis）** 严格遵循 [语义化版本规范 2.0.0 (Semantic Versioning)](https://semver.org/lang/zh-CN/)。

---

## 🏷️ 1. 版本号语义

版本格式定义为：`MAJOR.MINOR.PATCH`

- **主版本号 (MAJOR)**：引入不兼容的 API 变更、数据 Schema 结构破坏性重构或核心行为改变；
- **次版本号 (MINOR)**：引入向后兼容的新功能、新偏好规则或新格式导出器；
- **修订号 (PATCH)**：提供向后兼容的问题修复与算法性能优化。

---

## 📦 2. 数据 Schema 兼容与升级策略

- 所有持久化数据（如快照、项目清单、轮换计划）均显式携带 `schema_version`；
- 系统提供 `schema-migrate` 工具支持将旧版本数据安全迁移至新版本，并在修改前自动创建隐藏备份；
- 系统严格禁止对更高版本的未识别文件进行降级操作，防止数据损毁。

---

## 🖥️ 3. 运行环境支持矩阵

| 运行环境 | 最低版本要求 (Baseline) |
| :--- | :--- |
| **Rust 编译环境** | MSRV 1.88+ |
| **操作系统** | macOS 13+ / Windows 10+ / Ubuntu 22.04+ (或等同 Linux 发行版) |
| **桌面端运行时** | Tauri 2 原生窗口 |
| **前端浏览器** | Chrome 100+ / Edge 100+ / Safari 16+ / Firefox 100+ |

---

## 📖 相关文档

- [发布规范与完整性保障](publishing.md)
- [从 v1 升级指南](rust-migration.md)
