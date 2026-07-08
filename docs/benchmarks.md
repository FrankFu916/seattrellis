# 性能基准

SeatTrellis 的大班级性能用固定合成数据集跟踪。数据均为虚构学生，不包含真实
班级信息。

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

普通 CI 不应以绝对秒数失败。建议在 nightly 或发布前流程中比较相对回退比例，
并把 40/50/60 人、1/5/20 候选作为长期性能门槛。
