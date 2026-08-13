# 候选方案

`candidates --count N` 使用确定性的 seed 序列生成最多 N 个不同方案。每个候选
都必须满足 hard constraints，并包含独立 assignment、总分和评分明细。

```bash
seattrellis_cli candidates --problem problem.json --count 5 > outputs/candidates.json
```

## 推荐规则

1. 排除违反 hard constraints 的候选；
2. 按可用评分维度的加权总分降序排列；
3. 同分时按 `candidate_id` 稳定排序。

候选空间不足时会返回已经找到的不同方案并记录 warning，不会复制方案凑数。

v2 的候选报告为 `api_version: 2` 格式；v1 时代的 candidate set（每个候选内嵌
snapshot）继续可读，project 工作流按 `candidate_id` 选择后导出。
