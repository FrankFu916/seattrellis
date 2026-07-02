# 候选方案

`solve --candidates N` 会使用确定性的 seed 序列生成最多 N 个不同方案。每个候选都必须满足 hard constraints，并包含独立 snapshot、solver backend、总分和评分明细。

## 推荐规则

1. 排除违反 hard constraints 的候选；
2. 按可用评分维度的加权总分降序排列；
3. 同分时按 `candidate_id` 稳定排序。

候选空间不足时会返回已经找到的不同方案并记录 warning，不会复制方案凑数。

Candidate set 的当前 `schema_version` 为 `"0.2.2"`。普通 snapshot 继续使用 `"1.0"`。

