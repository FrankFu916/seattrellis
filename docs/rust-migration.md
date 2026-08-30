# 从 v1 (Python) 升级至 v2 (Rust) 完整指南

[English](rust-migration.md) · [简体中文](rust-migration.md)

**席序（SeatTrellis）v2.0.0** 标志着从 Python 原型向现代纯 Rust 生产级系统的全面升级与蜕变。

---

## 🚀 1. v2.0.0 带来的飞跃

| 维度 | v1 (Python 遗留维护线) | v2 (当前纯 Rust 发行版) |
| :--- | :--- | :--- |
| **运行时依赖** | 需配置 Python 3.10+、pip 及 OR-Tools 等 C++ 扩展 | **零运行时依赖**，单二进制独立分发，开箱即用 |
| **求解性能** | 易受 Python GIL 与解释器开销限制 | **亚毫秒级图剪枝** 与原生高并发多候选生成 |
| **桌面端形态** | 依赖浏览器或 Streamlit 页面 | **原生 Tauri 2 桌面应用**，跨平台高分辨率窗口 |
| **导出能力** | 依赖 Python ReportLab、WeasyPrint 等包 | **8 种格式纯 Rust 原生光栅化与排版**，彻底解决字体乱码 |
| **系统安全性** | 文件 I/O 边界松散 | **强类型 Schema 验证**，内置 256 位会话令牌与安全网关 |

---

## 📦 2. 数据与项目迁移指南

如果您拥有 v1 时代保存的班级名册、教室布局或项目文件，可以通过以下方式平滑迁移：

### 命令行一键迁移
```bash
# 1. 预检迁移变化（不写文件）
seattrellis schema-migrate --input my-class/seattrellis.project.json --dry-run

# 2. 导出为 v2 格式文件
seattrellis schema-migrate --input my-class/seattrellis.project.json --output my-class/seattrellis.v2.project.json

# 3. 就地升级（自动创建 .bak 隐藏备份）
seattrellis schema-migrate --input my-class/students.json --in-place
```

### 图形界面向导迁移
在 Web 工作台或桌面应用的“班级项目面板”中，点击“项目格式迁移”，系统将直观展示字段转换预览，一键完成安全升级。

---

## 📌 3. 旧版维护说明

Python v1 版本已在 **1.9.0** 正式封存，仅在 `v1.x-maintenance` 分支维护关键安全补丁（`pip install seattrellis==1.9.0`），不再开发任何新功能。

---

## 📖 相关文档

- [快速上手指南](quickstart.zh.md)
- [系统架构解析](architecture.md)
- [输入数据格式规范](input-format.zh.md)
