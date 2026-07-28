# 性能基准

SeatTrellis 的大班级性能用固定合成数据集跟踪。数据集名为
`synthetic-classroom`，当前版本为 `synthetic-v1`。所有学生、座位和指标均为
虚构数据，不包含真实班级信息。

fallback 的推荐入口：

```bash
python scripts/benchmark_solver.py \
  --sizes 40,50,60 \
  --backends fallback \
  --constraint-profiles light,dense \
  --candidate-counts 1,5,20 \
  --time-limit 0.25 \
  --max-attempts 24 \
  --output outputs/benchmark-solver.json \
  --markdown-output outputs/benchmark-solver.md
```

OR-Tools 需要更长的模型搜索窗口，使用相同矩阵时把 `--backends` 改为
`ortools`，并把 `--time-limit` 设为 `5`。每周 workflow 按人数、profile 和
backend 拆成 12 个并行 job，fallback 使用 0.25 秒，OR-Tools 使用 5 秒；这样
报告比较的是各后端在固定、明确预算内的产出，而不是让较慢 job 阻塞整组结果。

旧入口保持兼容：`--candidates 1` 仍运行单一候选数，未传
`--constraint-profiles`（别名 `--profiles`）时仍使用 `light`。传入
`--candidate-counts` 时，它覆盖旧的单值 `--candidates`。`--time-limit` 是每次
solve attempt 的限制；`--max-attempts` 可限制一个 case 的总尝试数。

固定长期矩阵是 40/50/60 人、`light`/`dense` 两种约束 profile，以及 1/5/20
个候选。`light` 使用选定 preset，不增加硬约束；`dense` 在相同虚构数据上增加
确定性的 fixed-seat、cannot-adjacent 和 graph-distance 规则。两者都不会读取真实
学生数据。

基准报告记录：

- dataset name、dataset version 和 case id；
- 班级人数和布局尺寸；
- backend request：`fallback`、`ortools`、`native` 或 `auto`；
- candidate 数量；
- constraint profile 和唯一 run id；
- 每次求解 time limit；
- 是否成功；
- 实际耗时；
- `parse`、`compile`、`solve`、`score`、`serialization` 五阶段耗时；
- case feasibility、候选生成率和 solve attempt 数；
- 候选之间换座学生比例的 pairwise mean/min/max diversity；单候选时为不可用；
- 生成的候选数；
- 推荐 candidate；
- 实际 solver backend、effective backend 和 solver status；
- SeatTrellis、Python 与平台版本；
- 按 backend 和班级人数汇总的成功率、耗时和最快 backend；
- 失败时的错误类型和错误信息。

JSON 文件适合机器比较和长期归档。Markdown 文件适合放入 release notes、PR
说明或人工验收记录。`score` 阶段包含快照构造、评分和硬约束复核；
`serialization` 只测 CandidateSet 的内存 JSON 编码，不包含磁盘写入。

当前默认用例固定为：

| 人数 | 布局 | case id |
|---:|---|---|
| 40 | 5×8 | `synthetic-v1-40-students-5x8` |
| 50 | 5×10 | `synthetic-v1-50-students-5x10` |
| 60 | 6×10 | `synthetic-v1-60-students-6x10` |

如需修改合成数据逻辑，应创建新的 dataset version，而不是直接改变
`synthetic-v1`，这样历史性能报告仍可比较。

`.github/workflows/benchmarks.yml` 提供每周定时和手动运行，上传 JSON/Markdown
artifact 并把 Markdown 写入 workflow summary。该流程只报告结果，不按绝对秒数
失败；后续性能回退判断应比较同环境下的相对变化。普通 PR CI 不运行完整矩阵。

v1.4 的首份实测结果、预算校准和 native 后端决策见
[`benchmark-baseline-v1.4.md`](benchmark-baseline-v1.4.md)。
