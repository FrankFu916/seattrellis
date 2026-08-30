# 多候选方案生成与优选机制

[English](candidates.md) · [简体中文](candidates.md)

在班级排座实践中，往往不存在唯一的“完美解”，而是存在若干各具特色、各有侧重的优质方案。**席序（SeatTrellis）** 提供了一键生成多套备选方案的机制，方便老师同屏对比、权衡挑选。

---

## 🔀 1. 生成多套方案

通过命令行生成指定数量的候选方案：

```bash
seattrellis candidates \
  --problem problem.json \
  --count 5 \
  --latest-snapshot history/last_term.snapshot.json \
  > outputs/candidates.json
```

- `--count <n>`：生成的候选方案数量（支持 1 ~ 20 个，默认为 5 个）；
- `--latest-snapshot <path>`：提供最近一期的快照，用于评估各候选方案相比上一期的稳定性（`stability_score`）。

---

## 🏆 2. 推荐排序与遴选原则

系统基于严格的层次化规则从生成的候选集中选出**最佳推荐方案（Recommended Candidate）**：

1. **硬约束零容忍过滤**：剔除任何未通过硬约束校验的无效方案；
2. **加权总分降序排序**：对所有合法方案，计算各可用评分维度的加权总分；
3. **确定性打破平局（Tie-Breaking）**：若出现同分情况，以 `candidate_id` 的自然序稳定裁决，确保任意环境下的复现确定性。

---

## 📊 3. 多样性与防重复机制

- 算法在探索候选解空间时，会自动加入“差异性引导（Diversity Guidance）”，确保生成的各个候选方案之间具备足够的座次变动度，避免生成雷同结果；
- 若班级人数较少或约束极其严苛导致可行解空间非常有限，系统将仅输出实际找到的所有互不相同方案，并输出说明，**绝不通过复制重复方案来凑数**。

---

## 📖 相关文档

- [多维度评分机制](scoring.md)
- [排座规则手册](rules.zh.md)
- [CLI 命令行手册](cli.md)
