# SeatTrellis · 席序

SeatTrellis 是一个隐私优先、本地运行的课堂排座工具。v2 是纯 Rust 实现：桌面
应用（Tauri）、loopback 本地服务 + React 工作台，以及 `seattrellis_cli`
命令行工具，都不需要 Python 或其他运行时。它将学生名单、教室布局、规则和历史
座位记录组合成可复现的单方案或多候选方案。

## 从这里开始

- [快速开始](quickstart.zh.md)：安装（桌面应用 / `cargo install seattrellis_cli`）、
  校验、求解和导出；
- Web 端：[中文](web.zh.md) / [English](web.en.md)：分步向导、候选比较和下载；
- [CLI 命令参考](cli.md)：28 个子命令与退出状态；
- [输入格式](input-format.zh.md)：学生、layout 和产物格式；
- [规则](rules.zh.md)：hard constraints、soft preferences 和 presets；
- [隐私](privacy.md)：真实班级数据的本地处理边界。

v1（Python）版本冻结在 1.9.0，仅作为遗留包（`pip install seattrellis==1.9.0`）。

## 核心原则

1. Hard constraints 永远优先于 soft scoring。
2. seed 固定伪随机序列；未被墙钟时间提前终止的求解应生成稳定结果。
3. 不可计算的评分维度标记为 `not_available`，不虚构分数。
4. 默认不采集遥测、不上传学生数据。
5. `examples/` 中只允许虚构数据。
6. 求解状态与 CLI 退出码使用冻结语义：启发式耗尽只能是 `Unknown`，
   绝不能伪装成 `ProvenInfeasible`。
