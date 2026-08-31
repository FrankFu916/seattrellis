<div align="center">
  <img src="docs/assets/logo.svg" width="128" alt="席序 SeatTrellis 标志" />

  # **席序 SeatTrellis**

  **让班级排座回归简单、科学与公正。**

  一款专注于隐私保护与本地计算的智能课堂排座工具。<br />
  导入名单、配置规则、一键求解、交互微调、导出打印。<br />
  **无须注册账号，无需云端同步，学生数据全流程留在您的电脑本地。**

  [![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)
  [![Rust](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml)
  [![Release](https://img.shields.io/github/v/release/FrankFu916/seattrellis)](https://github.com/FrankFu916/seattrellis/releases)
  [![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
  [![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](https://github.com/FrankFu916/seattrellis/releases)

  [📥 下载桌面版](https://github.com/FrankFu916/seattrellis/releases) · `cargo install seattrellis`

  [最新发布](https://github.com/FrankFu916/seattrellis/releases) · [快速上手](docs/quickstart.zh.md) · [规则手册](docs/rules.zh.md) · [English](README.md)
</div>

---

排座位是每位班主任和任课老师每学期都要面对的难题：
- 视力不好的孩子需要靠前，个子高的同学不能挡住后排；
- 某些同学之间需要互相学习，某些同学在一起容易讲小话；
- 既要兼顾平时表现与成绩互助，又要定期轮换保持机会均等；
- 每次手工排座耗费大半天，排完还难以向学生和家长解释排座依据。

**席序（SeatTrellis）为您化繁为简**：只需设定教学需求与偏好，内置算法即可秒级求解、给出清晰的评分依据，并完整记录历史轮换轨迹。

![座位表示例](docs/assets/demo-seating.png)

## ✨ 核心特色

| 功能模块 | 详细说明 |
| :--- | :--- |
| 🧩 **严守硬性底线（Hard Constraints）** | 支持指定固定座位、必须相邻、禁止相邻、最小间隔距离、小组捆绑或隔离等要求。所有标记“已解决”的方案均经过独立校验器二次复核，确保**零违规**。 |
| 🎯 **兼顾柔性偏好（Soft Preferences）** | 智能权衡视力照顾、身高梯度、成绩互助、公平轮换、近期同桌回避等诉求。每一项规则均有独立评分明细，清晰回答“为什么这样排”。 |
| 🔀 **多候选对比与精确复现** | 一键生成多个高质量候选方案供老师挑选；固定随机种子（Seed）后，随时重新计算均可获得 100% 一致的结果。 |
| ✋ **直观的交互式微调** | 支持鼠标拖拽、座位互换、位置锁定、撤销/重做以及智能局部修复。任何手动调整都会实时触发规则合规性检查。 |
| 📅 **多学期公平轮换** | 自动追踪历史排座记录，生成跨周期轮换方案，并用本地热力图展示座位占用变化与相邻轮次移动距离。 |
| 🖨️ **8 种主流格式导出** | 支持一键导出 SVG、HTML、打印专用 HTML、PNG 图片、PDF、Excel（XLSX）、Word（DOCX）及 PowerPoint（PPTX），教师版与学生公示版随心切换。 |
| 🔒 **本地优先，隐私安全** | 全流程纯本地离线运算，无遥测、无数据上报；公开版导出自动对姓名和学号进行脱敏处理。 |

---

## 🚀 快速上手

### 1. 桌面端应用（推荐教师使用）

直接从 [GitHub Releases](https://github.com/FrankFu916/seattrellis/releases) 下载适用于您系统的安装包：

| 操作系统 | 推荐安装格式 |
| :--- | :--- |
| **macOS** (Apple Silicon) | `.dmg` 安装镜像 或 `.app.tar.gz` |
| **Windows** (x64) | `.msi` 安装包 或 NSIS `.exe` 安装引导 |
| **Linux** (amd64) | `.deb` 安装包 |

> 💡 **提示**：安装包默认未经商业证书签名。macOS 首次打开时如遇提示，可右键点击应用图标并选择“打开”；Windows 若弹出 SmartScreen 保护，点击“仍要运行”即可。

### 2. 命令行工具（推荐自动化与开发者使用）

通过 Rust 包管理器快速安装：

```bash
cargo install seattrellis

# 1. 预检数据与规则完整性
seattrellis validate --problem problem.json

# 2. 求解并生成排座方案
seattrellis solve --problem problem.json --output plan.json

# 3. 导出为高保真图片或文档
seattrellis export --problem problem.json --solution plan.json --format png --output plan.png
```

### 3. 三分钟读懂规则配置

规则文件采用直观的 JSON 格式，清晰划分为**必须满足的硬约束**与**加权优化的软偏好**：

```json
{
  "seed": 42,
  "hard": {
    "fixed_seats":       [{ "student": "STU001", "seat_id": "R1C1" }],
    "cannot_be_adjacent": [{ "students": ["STU004", "STU007"] }]
  },
  "soft": {
    "vision_front": { "enabled": true, "weight": 20 },
    "height_back":  { "enabled": true, "weight": 5 },
    "fair_rotation": { "enabled": true, "weight": 10, "lookback": 4 }
  }
}
```

- 完整字段规范与格式参考：[数据格式指南](docs/input-format.zh.md) 与 [排座规则手册](docs/rules.zh.md)。
- 内置开箱即用的 14 种教学场景（如日常教学、期中期末考试、前后排轮换等）：[场景预设参考](docs/presets.md)。

---

## 🔄 从 v1 (Python) 版本升级

如果您之前使用的是 Python 开发的 v1 版本，可以通过 `seattrellis schema-migrate` 命令或图形界面中的项目迁移向导将历史数据无缝升级至 v2，系统会在迁移前自动创建备份。

旧版 Python 包已在 `1.9.0` 版本封存维护（`pip install seattrellis==1.9.0`），v2 版本为独立高效的纯 Rust 实现，不再依赖 Python 环境。详情请参考 [v1 升级指南](docs/rust-migration.md)。

---

## 📖 文档导航

| 入门与使用 | 规则与格式 | 架构与进阶 |
| :--- | :--- | :--- |
| 📖 [快速上手指南](docs/quickstart.zh.md) | 📐 [排座规则手册](docs/rules.zh.md) | 🏗️ [系统架构解析](docs/architecture.md) |
| 🖥️ [Web 与桌面工作台指南](docs/web.zh.md) | 📄 [输入数据格式规范](docs/input-format.zh.md) | ⚙️ [CLI 命令行参考 (27个子命令)](docs/cli.md) |
| 🖨️ [多格式导出与排版](docs/export.zh.md) | 🎒 [班级项目管理工作流](docs/project.zh.md) | 🔒 [本地隐私与安全规范](docs/privacy.md) |

---

## 🛡️ 隐私与数据安全承诺

席序将学生数据隐私置于首位。所有计算、排座与导出均在您本地计算机的内存与硬盘中进行，绝不进行任何形式的云端回传或遥测统计。在公开排座结果时，系统提供一键匿名化功能，防止学生个人敏感信息泄露。

---

## 💻 参与开发与构建

席序采用高性能、类型安全的现代技术栈构建：
- **后端核心**：Rust 1.88+、9 个模块化分层 Crates
- **桌面与前端**：Tauri 2、React 19、TypeScript
- **质量保障**：690+ 项 Rust 单元测试与集成测试、160+ 项前端测试、端到端自动化测试与基准性能测试门禁。

```bash
# 1. 前端构建
cd clients/web && npm ci && npm run build && cd ../..

# 2. 运行完整 Rust 校验与测试套件
cargo test --locked --workspace
cargo clippy --locked --all-targets --workspace -- -D warnings
```

---

## 📄 开源许可

本项目遵循 [Apache-2.0 开源许可协议](LICENSE)。详情参见 [NOTICE](NOTICE)。
