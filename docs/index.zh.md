---
slug: /zh/
title: SeatTrellis · 席序
---

# SeatTrellis · 席序

席序（SeatTrellis）是一个隐私优先、本地运行的课堂排座工具。v2 是纯 Rust 实现：
桌面应用（Tauri）、loopback 本地服务 + React 工作台，以及 `seattrellis_cli`
命令行工具，都不需要 Python 或其他运行时。

> English documentation: switch via the **Language** menu in the navbar, or see
> the [English docs home](/).

## 中文文档

- [快速开始](quickstart.zh.md)：安装（桌面应用 / `cargo install seattrellis_cli`）、校验、求解和导出；
- [Web 工作台](web.zh.md)：分步向导、候选比较和下载；
- [输入格式](input-format.zh.md)：学生、layout 和产物格式；
- [规则手册](rules.zh.md)：hard 约束、soft 偏好与预设；
- [导出说明](export.zh.md)：八种格式与教师/公示版；
- [中文字体策略](font-strategy.zh.md)；
- [Project 工作流](project.zh.md)；
- [隐私](privacy.md)：真实班级数据的本地处理边界。

v1（Python）版本冻结在 1.9.0，仅作为遗留维护线
（`pip install seattrellis==1.9.0`），v2 不依赖它。

## 核心原则

1. 硬约束永远优先于软偏好评分。
2. 固定 seed 时，未被墙钟截止提前终止的求解结果可精确复现。
3. 不可计算的评分维度标记为 `not_available`，不虚构分数。
4. 默认不采集遥测、不上传学生数据。
5. `examples/` 中只允许虚构数据。
6. 求解状态与 CLI 退出码使用冻结语义：启发式耗尽只能是 `Unknown`，
   绝不能伪装成 `ProvenInfeasible`。
