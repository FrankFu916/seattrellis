# 席序 SeatTrellis

[![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)
[![Rust](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/FrankFu916/seattrellis?include_prereleases&label=release)](https://github.com/FrankFu916/seattrellis/releases)

**简体中文 | [English](README.en.md)**

席序 SeatTrellis 是一款**本地优先的课堂排座工具**：导入名单、生成座位表、手工微调、导出打印或分享。所有数据都在本机处理——无账号、无云同步、学生数据不出电脑。

- 生成可复现的座位安排（单方案，或带可解释评分的一组候选方案）
- 满足硬约束（固定座位、相邻关系、最小距离、分组），同时优化软偏好（身高、视力、成绩均衡、公平轮换、近期邻座回避）
- 交互式编辑器：拖拽、交换、锁定、撤销、修复
- 多学期轮换计划（公平性与邻座重复摘要）
- 导出 SVG、HTML、打印版 HTML、PNG、PDF、XLSX、DOCX、PPTX
- 完全离线；公开导出自动匿名化

![Demo seating chart](docs/assets/demo-seating.png)

## 安装

### 桌面版（推荐）

从 [Releases](https://github.com/FrankFu916/seattrellis/releases) 下载对应平台的安装包：

- **Windows**：MSI 或 NSIS 安装包（x64）
- **macOS**：DMG 或 app 归档（Apple Silicon）
- **Linux**：DEB 包

安装前请对照 `SHA256SUMS` 校验文件完整性。

### CLI

```bash
cargo install seattrellis_cli
# 或使用 Releases 中的预编译二进制
```

### v1（Python）线

v1 线已冻结于 **1.9.0**，由 `v1.x-maintenance` 分支维护。需要旧版包时：

```bash
pip install seattrellis==1.9.0
```

## 快速开始（CLI）

```bash
seattrellis_cli validate --problem problem.json
seattrellis_cli solve --problem problem.json --output plan.json
seattrellis_cli export --problem problem.json --solution plan.json --format png --output plan.png
```

完整场景见 [快速开始指南](docs/quickstart.zh.md)，格式参考见 [输入格式](docs/input-format.zh.md)，命令参考见 [CLI 参考](docs/cli.md)。

## 从 v1 迁移

v1 的项目文件与工件可在 v2 工作台或通过 `seattrellis_cli schema-migrate` 自动迁移，每次迁移前自动备份。

## 文档

- [快速开始](docs/quickstart.zh.md) · [输入格式](docs/input-format.zh.md) · [规则](docs/rules.zh.md) · [导出](docs/export.zh.md) · [隐私](docs/privacy.md)
- [开发指南](docs/development.md) · [发布](docs/publishing.md)

## 隐私

席序完全在本机处理数据。请勿将真实学生名单、学号、成绩、班级、学校或排座历史提交到公开仓库——仓库只包含合成示例数据。

## 许可

Apache-2.0，见 [LICENSE](LICENSE) 与 [NOTICE](NOTICE)。
