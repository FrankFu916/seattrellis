# ADR-0001：原生计算内核采用 Rust

- 状态：接受（已被 ADR-0004 supersede，2026-08-08）
- 日期：2026-07-06

> **superseded**：ADR-0004 将 v2.0.0 的方向改为 Rust-only 生产运行时；
> 本 ADR 中"不进行全量重写""Python 至少保留到 Rust backend 通过验收"的
> 表述不再适用。Python 以 oracle 角色保留，删除 gate 见 ADR-0004。

## 背景

SeatTrellis 的 CLI、文件处理、Web、报表和导出依赖 Python 生态，但图计算、
历史统计、规则验证、评分和启发式求解适合在稳定接口后下沉。项目需要兼顾
Windows、macOS、Linux 和未来桌面端，同时避免维护多套领域语义。

## 决策

需要原生实现的通用领域内核采用 Rust，并通过 PyO3 与 maturin 接入 Python。
迁移采用可选 backend 和差分测试，不进行全量重写。

首批候选模块为：

1. 邻接图与距离预计算；
2. 硬约束验证；
3. 历史关系统计与方案评分；
4. 通过性能门槛后再迁移启发式求解。

Python 实现至少保留到 Rust backend 通过三平台构建、行为一致性和性能验收。

## 后果

- 获得内存安全、跨平台复用和未来 Tauri/桌面端复用能力；
- 增加 native wheel、Rust 工具链和 Python/Rust 差分测试成本；
- Python 与 Rust 之间必须使用粗粒度、版本化 DTO，禁止逐座位频繁跨边界调用；
- 若 40–60 人基准没有至少两倍加速或明显交互收益，应停止继续迁移。

## 不采用

- 全量 Rust 重写：会无谓替换成熟的 Python 导出和 UI 生态；
- C++ 作为通用内核：其主要优势是 OR-Tools 原生接口，不足以抵消双内核维护风险。
