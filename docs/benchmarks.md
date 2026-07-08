# 性能基准

SeatTrellis 的大班级性能用固定合成数据集跟踪。数据集名为
`synthetic-classroom`，当前版本为 `synthetic-v1`。所有学生、座位和指标均为
虚构数据，不包含真实班级信息。

当前推荐入口：

```bash
python scripts/benchmark_solver.py \
  --sizes 40,50,60 \
  --backends fallback,ortools \
  --candidates 1 \
  --time-limit 10 \
  --output outputs/benchmark-solver.json
```

基准报告记录：

- dataset name、dataset version 和 case id；
- 班级人数和布局尺寸；
- backend request：`fallback`、`ortools`、`native` 或 `auto`；
- candidate 数量；
- 每次求解 time limit；
- 是否成功；
- 实际耗时；
- 生成的候选数；
- 推荐 candidate；
- 实际 solver backend、effective backend 和 solver status；
- SeatTrellis、Python 与平台版本；
- 失败时的错误类型和错误信息。

当前默认用例固定为：

| 人数 | 布局 | case id |
|---:|---|---|
| 40 | 5×8 | `synthetic-v1-40-students-5x8` |
| 50 | 5×10 | `synthetic-v1-50-students-5x10` |
| 60 | 6×10 | `synthetic-v1-60-students-6x10` |

如需修改合成数据逻辑，应创建新的 dataset version，而不是直接改变
`synthetic-v1`，这样历史性能报告仍可比较。

普通 CI 不应以绝对秒数失败。建议在 nightly 或发布前流程中比较相对回退比例，
并把 40/50/60 人、1/5/20 候选作为长期性能门槛。
