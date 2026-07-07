# ADR-0002：v1.x 保留 Python OR-Tools 后端

- 状态：接受
- 日期：2026-07-06

## 背景

OR-Tools 官方提供 Python 和 C++ API，但没有一等 Rust API。SeatTrellis 当前的
CP-SAT 模型已经通过 Python API 工作，近期目标是改善产品体验和领域边界，而
不是替换成熟求解器。

## 决策

v1.x 中继续使用 Python OR-Tools，并把它放在统一的 `SolverBackend` 协议后。
Rust 内核负责预计算、验证、评分和启发式求解，不重新实现 CP-SAT。

只有同时满足以下条件时才评估 C++ adapter：

- 桌面发行必须移除 Python 求解运行时；
- Python OR-Tools 的体积、启动或进程边界已被基准证明为主要问题；
- 团队能够持续维护 CMake、OR-Tools ABI 和三平台二进制。

## 后果

- v1.x 不引入 C++ 构建链；
- CP-SAT 与 fallback/Rust backend 必须共享规则编译语义和输出契约；
- C++ 若被引入，只能是薄 OR-Tools adapter，不承载另一套评分、历史或规则逻辑。
