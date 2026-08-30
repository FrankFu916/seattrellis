# 邻座关系分析与同桌回避（Pair History）

[English](pair-history.md) · [简体中文](pair-history.md)

为了促进班级同学间的广泛交流，避免形成固化小团体，**席序（SeatTrellis）** 提供了跨周期的搭档历史统计与同桌回避能力。

---

## 👥 1. 邻座关系报告（`pair-report`）

通过 `pair-report` 命令可以直观排查班级中重复同桌或相邻频次最高的学生对：

```bash
seattrellis pair-report \
  --problem problem.json \
  --history-dir examples/history \
  --top 10 \
  --within-distance 2
```

### 分析的关系维度：
- **`desk_mate`**：同排且紧邻的水平同桌；
- **`horizontal`** / **`vertical`** / **`diagonal`**：横向、纵向前后排、斜对角相邻；
- **`adjacent_any`**：任意连通相邻；
- **`within_distance`**：指定几何距离范围内的邻近。

---

## ❄️ 2. 智能回避与严格冷却（Cooling）

- **`avoid_recent_neighbors`**：基于权重惩罚近期反复同桌的组合；
- **`cooling`（严格冷却期）**：强制在设定的轮换期数（`cooling_period`）内禁止某两名学生再次搭档；
- 无论回避权重多大，**均绝不违反显式的固定座位与硬约束**。

---

## 📖 相关文档

- [历史位置追踪](history.md)
- [排座规则手册](rules.zh.md)
- [输入数据格式规范](input-format.zh.md)
