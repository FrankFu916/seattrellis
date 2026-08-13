# 性能基准

SeatTrellis 的大班级性能用固定合成数据集跟踪。数据集名为
`synthetic-classroom`，当前版本为 `synthetic-v1`。所有学生、座位和指标均为
虚构数据，不包含真实班级信息。

## 性能回归门槛（CI 常跑）

`solver-baseline.json`（`benchmarks/`）记录了 40/50/60/80 人 planted-feasible
实例的 release 模式墙钟基线。CI 的 release job 运行：

```bash
cargo build --release --locked -p seattrellis_cli
python3 scripts/bench_solver.py --check
```

门槛是基线 ×1.10（容忍 CI 硬件噪声）+ 绝对交互上限（40 人 1.5s、50 人 2.5s、
60 人 3.5s、80 人 6s）。真实算法回退会大幅突破这两个界限，普通 CI 噪声不会。
需要重新校准基线时运行 `python3 scripts/bench_solver.py --record`，并把新的
`benchmarks/solver-baseline.json` 提交入库。

## 求解质量：与 OR-Tools oracle 的 regret

`scripts/measure_rust_quality.py` 在相同编译问题上比较 Rust 求解器与 Python
OR-Tools oracle 的归一化 regret：

```text
regret = (rust_total_cost - ortools_total_cost) / |ortools_total_cost|
```

门槛：标准基准集上 median regret ≤ 5%、P95 ≤ 15%。regret 为正表示 Rust 方案
更贵（更差），为负表示更优。两侧求解同一个编译问题：OR-Tools 走 Python
backend，Rust 侧用 release CLI 求解由同一编译问题构造的 `CoreSolveRequest`。

## Oracle 差分

固定 corpus 的 Python↔Rust 差分（`scripts/rust_python_diff.py --fixtures`）
覆盖 41 个 case（34 合法 + 7 invalid），使用确定性预算 `--time-limit 1800`
（墙钟截止的 solve 不稳定，短预算会产出不可重放的 golden；对截止命中的运行
显式 SKIP）。任何 Python error 都不能记为 INFEASIBLE，mismatch 必须非零退出。

## 数据集

固定长期矩阵是 40/50/60 人、`light`/`dense` 两种约束 profile，以及 1/5/20
个候选；性能回归门槛额外覆盖 80 人。`light` 使用选定 preset，不增加硬约束；
`dense` 在相同虚构数据上增加确定性的 fixed-seat、cannot-adjacent 和
graph-distance 规则。两者都不会读取真实学生数据。

| 人数 | 布局 | case id |
|---:|---|---|
| 40 | 5×8 | `synthetic-v1-40-students-5x8` |
| 50 | 5×10 | `synthetic-v1-50-students-5x10` |
| 60 | 6×10 | `synthetic-v1-60-students-6x10` |

如需修改合成数据逻辑，应创建新的 dataset version，而不是直接改变
`synthetic-v1`，这样历史性能报告仍可比较。

## 报告与归档

release job 归档基准 JSON/Markdown 报告并写入 workflow summary；普通 PR CI
不运行完整矩阵。回退判断比较同类 runner 上的相对变化，并单独观察可行率、
候选产出率和候选多样性，不按绝对秒数失败。

v1.4 时代的首份实测结果与预算校准见
[`benchmark-baseline-v1.4.md`](benchmark-baseline-v1.4.md)（历史记录；v2 的
Rust 求解器质量与性能以本节描述的自动化门槛为准）。
