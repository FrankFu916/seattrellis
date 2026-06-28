# 桌面端技术调研

> 状态：调研中 | 最后更新：2026-06-28
>
> 本调研不阻塞 PyPI 发布和 Web 端开发，属于 Phase 3 的并行后台任务。

## 背景

SeatTrellis 当前以 CLI + Streamlit Web 为主要使用方式。长期规划中需要一个独立的桌面应用，满足以下需求：

- 无需安装 Python 和命令行知识
- 双击安装即可使用
- 本地数据管理更直观（文件浏览器 vs 命令行路径）
- 更丰富的交互（拖拽调整座位、可视化规则编辑器）

## 候选技术方案

### 1. Tauri（Rust + Web 前端）

| 维度 | 评估 |
|------|------|
| 打包体积 | ⭐⭐⭐⭐⭐ 极佳（~5-15 MB，不含 Python） |
| 跨平台 | ⭐⭐⭐⭐⭐ macOS / Windows / Linux |
| 安装体验 | ⭐⭐⭐⭐⭐ 标准安装包（.dmg / .msi / .AppImage） |
| 开发复杂度 | ⭐⭐ 需要 Rust + 前端框架 + Python 通信层 |
| Python core 集成 | ⭐⭐ 需通过 sidecar 进程或 HTTP 通信 |
| 生态成熟度 | ⭐⭐⭐⭐ 2.0 已稳定，社区活跃 |

**架构设想：** Tauri 作为壳 → 内嵌 Web 前端（复用现有 HTML/CSS 模板逻辑）→ 通过 HTTP 或子进程调用 Python CLI。

**风险：** Python runtime 需要随应用打包（PyInstaller / embedded Python），打包体积会增大到 50-80 MB。

### 2. PySide / PyQt（纯 Python GUI）

| 维度 | 评估 |
|------|------|
| 打包体积 | ⭐⭐ 较大（含 Qt 库 ~40-80 MB） |
| 跨平台 | ⭐⭐⭐⭐ macOS / Windows / Linux |
| 安装体验 | ⭐⭐⭐ 可用 PyInstaller 打包 |
| 开发复杂度 | ⭐⭐⭐ 熟悉 Qt 信号/槽模式 |
| Python core 集成 | ⭐⭐⭐⭐⭐ 直接调用，零通信开销 |
| 生态成熟度 | ⭐⭐⭐⭐ Qt 历史悠久 |

**优势：** 与 Python core 无缝集成，不引入新的运行时。

**劣势：** Qt 库体积大，UI 开发效率不如 Web 方案，许可证需注意（GPL/LGPL）。

### 3. NiceGUI（Python + Web UI）

| 维度 | 评估 |
|------|------|
| 打包体积 | ⭐⭐⭐ 中等（含 FastAPI + 前端） |
| 跨平台 | ⭐⭐⭐⭐ 基于浏览器 |
| 安装体验 | ⭐⭐⭐ 可打包为本地服务器 |
| 开发复杂度 | ⭐⭐⭐⭐ 纯 Python，学习成本低 |
| Python core 集成 | ⭐⭐⭐⭐⭐ 直接调用 |
| 生态成熟度 | ⭐⭐ 较新，社区规模有限 |

**模式：** NiceGUI 在本地启动 FastAPI 服务器，浏览器作为 UI。类似 Streamlit 但更接近传统 Web 开发。

**风险：** 不是真正的"桌面应用"，用户体验依赖本地浏览器。

### 4. FastAPI + Local Web（PyWebView 或类似）

| 维度 | 评估 |
|------|------|
| 打包体积 | ⭐⭐⭐ 中等 |
| 跨平台 | ⭐⭐⭐⭐ |
| 安装体验 | ⭐⭐⭐ 可用 PyInstaller |
| 开发复杂度 | ⭐⭐⭐⭐ FastAPI 学习成本低 |
| Python core 集成 | ⭐⭐⭐⭐⭐ 直接调用 |

**模式：** 本地 Python 启动 FastAPI → 前端（React/Vue 或简单 HTML）→ 用 pywebview 包装成桌面窗口。

### 5. Electron（淘汰）

| 维度 | 评估 |
|------|------|
| 打包体积 | ⭐ 极大（>150 MB 含 Chromium） |
| Python core 集成 | ⭐ 需子进程通信 |

**结论：** 体积过大，不适合教育场景。已淘汰。

## 初步推荐

**短期（v0.6-v0.8）：不做任何决定，保持调研状态。**

**中期（v1.0 后）：优先评估 Tauri + embedded Python 方案。**
- 打包体积可控（目标 < 60 MB）
- 复用现有 Web 端组件（座位图、候选切换等已有 HTML 基础）
- Rust 生态健康，Tauri 2.0 插件丰富

**备选：** 如果 Tauri + Python sidecar 复杂度过高，退回到 PySide 或 FastAPI + pywebview。

## 关键风险

1. **Python 嵌入打包**：将 Python runtime 随桌面应用分发需要 PyInstaller 或 conda-pack；跨平台打包机配置复杂。
2. **Web → Python 通信**：如果选 Tauri，需要设计 HTTP API 层；这部分与现有 `web/workflow.py` 可共享。
3. **字体与渲染**：桌面端 PDF/PNG 预览需要嵌入或依赖系统中文字体；参考 `docs/font-strategy.zh.md`。
4. **自动更新**：需要设计更新机制（Sparkle for macOS, MSI auto-update for Windows）。
5. **签名与公证**：macOS 需要 Apple Developer 签名 ($99/年)，Windows 需要代码签名证书。

## 暂不考虑的方案

- **Flutter Desktop**：Dart 生态与 Python 集成成本高
- **React Native Desktop**：生态不成熟
- **JavaFX / Swing**：体积大，与 Python 通信需 JNI/JEP
- **纯 Web 替代桌面端**：不满足"离线可用"的强制需求（但 Web PWA 可作为轻量替代，待评估）

## 下一步

- [ ] 搭建 Tauri + Python sidecar 最小原型（1 个 solve 命令）
- [ ] 测试跨平台打包体积和启动速度
- [ ] 评估 PyInstaller + Tauri sidecar 的打包流程
- [ ] 对比 PySide 简单 UI 的原型开发效率
- [ ] 输出最终技术选型建议文档（v0.8 前完成）
