<div align="center">
  <img src="docs/assets/logo.svg" width="128" alt="席序 SeatTrellis logo" />

  # **席序 SeatTrellis**

  **把排座这件事，交给一台不讲人情的机器。**

  本地优先的课堂排座工具 —— 导入名单、生成座位表、手工微调、导出打印。
  无账号、无云同步，学生数据永不出电脑。

  [![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)
  [![Rust](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml)
  [![Release](https://img.shields.io/github/v/release/FrankFu916/seattrellis)](https://github.com/FrankFu916/seattrellis/releases)
  [![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
  [![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](releases)

  `下载桌面版` · `cargo install seattrellis_cli` · 纯 Rust，零 Python 依赖

  [下载最新版](releases) · [快速开始](docs/quickstart.zh.md) · [规则手册](docs/rules.zh.md) · [English](README.md)
</div>

---

每个班总有一些座位是"敏感位"：近视的要靠前，高个子得靠后，有两个孩子
不能坐在一起，还有家长拜托"多照顾一下"。手工排一次要一个下午，换学期
重排又是一个下午，还说不出一句"为什么这么排"。

**席序把这件事变成一次点击**：你定规则，它求解、解释、留档。

![座位表示例](docs/assets/demo-seating.png)

## 它能做什么

| | |
|---|---|
| 🧩 **硬约束，保证满足** | 固定座位、必须相邻、禁止相邻、最小距离、小组同座/隔离——任何标记"已解决"的方案都经过独立校验器复核，绝不违反 |
| 🎯 **软偏好，可解释** | 视力靠前、身高靠后、成绩均衡、公平轮换、近期邻座回避……每条规则有独立评分，方案能回答"为什么他坐这里" |
| 🔀 **候选与复现** | 一次生成多个候选方案对比推荐；固定 seed 任何一天重跑，结果一字不差 |
| ✋ **手工微调** | 拖拽、交换、锁定、撤销/重做、违规修复——改完自动过约束校验 |
| 📅 **多学期轮换** | 公平轮换计划 + 邻座重复摘要，长期班级一键滚动 |
| 🖨️ **八种导出** | SVG / HTML / 打印 HTML / PNG / PDF / XLSX / DOCX / PPTX，教师版与公开匿名版一键切换 |
| 🔒 **本地优先** | 全部计算在本机完成，无账号、无遥测、无云同步；公开导出自动匿名化姓名与学号 |

## 快速开始

### 桌面版（推荐给老师）

从 [Releases](releases) 下载对应平台安装包：

| 平台 | 格式 |
|---|---|
| macOS (Apple Silicon) | `.dmg` / `.app.tar.gz` |
| Windows (x64) | `.msi` / NSIS `.exe` |
| Linux (amd64) | `.deb` |

安装包未签名，完整性请对照 `SHA256SUMS` / `DESKTOP-SHA256SUMS` 校验；
macOS 首次打开需右键 →「打开」，Windows 可能出现 SmartScreen 提示。

### 命令行（推荐给自动化）

```bash
cargo install seattrellis_cli

seattrellis_cli validate --problem problem.json   # 预检规则与数据
seattrellis_cli solve    --problem problem.json --output plan.json
seattrellis_cli export   --problem problem.json --solution plan.json --format png --output plan.png
```

### 三条规则看懂配置

规则是一个 JSON 文件，分 `hard`（必须满足）与 `soft`（加权偏好）：

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

完整字段见 [输入格式](docs/input-format.zh.md) 与 [规则手册](docs/rules.zh.md)；
14 个内置场景预设（考试、日常、轮换……）见 [预设](docs/presets.md)。

## 从 v1 升级

v1（Python）项目文件由 `seattrellis_cli schema-migrate` 或工作台迁移流程
自动升级，迁移前自动备份。Python 包冻结在 1.9.0，仅作维护线
（`pip install seattrellis==1.9.0`），不再有 v2 依赖。详见
[从 v1 迁移](docs/rust-migration.md)。

## 文档

| | | |
|---|---|---|
| [快速开始](docs/quickstart.zh.md) | [CLI 参考](docs/cli.md)（27 个子命令） | [输入格式](docs/input-format.zh.md) |
| [规则手册](docs/rules.zh.md) | [导出](docs/export.zh.md) | [架构](docs/architecture.md) |
| [工作台（Web）](docs/web.zh.md) | [隐私](docs/privacy.md) | [开发指南](docs/development.md) |

## 隐私

所有数据在本机处理。请勿将真实学生名单、学号、成绩、班级或学校信息提交
到公开仓库——仓库只包含虚构示例数据。公开导出在单一中心策略层自动匿名化，
发布流程含敏感字段扫描。

## 开发

```bash
# 前端（server 构建时内嵌 clients/web/dist）
cd clients/web && npm ci && npm run build && cd ..

# Rust 全量测试 + clippy
cargo test --locked --workspace
cargo clippy --locked --all-targets --workspace -- -D warnings
```

技术栈：Rust 1.88（MSRV）· 9 个分层 crate · Tauri 2 · React 19 ·
698 项 Rust 测试 + 167 项前端测试 + 浏览器 E2E + fuzz + 性能门禁。
架构见 [docs/architecture.md](docs/architecture.md)。

## 许可

Apache-2.0，见 [LICENSE](LICENSE) 与 [NOTICE](NOTICE)。
