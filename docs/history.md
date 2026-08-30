# 历史位置追踪与公平轮换（Fair Rotation）

[English](history.md) · [简体中文](history.md)

为了防止个别学生在整个学期或整个学年长期固定在教室角落、后排或空调风口，**席序（SeatTrellis）** 建立了精细的历史位置画像与公平轮换机制。

---

## 📊 1. 历史位置报告（`history-report`）

运行 `history-report` 可以全面统计每位学生在历史各个时段所坐区域的分布情况：

```bash
seattrellis history-report \
  --problem problem.json \
  --history-dir examples/history \
  --output outputs/history-report.json
```

### 追踪的位置类别：
- `front`（前排）与 `back`（后排）
- `middle`（中间核心区）
- `side`（两侧靠墙排）与 `corner`（四角边缘）
- `near_window`（靠窗）、`near_door`（靠门）、`near_ac`（近空调风口）与 `near_platform`（靠讲台）

---

## 🔄 2. 轮换优化算法

在求解新一期座位时，`fair_rotation` 规则会自动分析最近若干期（`lookback`）的历史快照：
- 对近期连续坐过相同区域的学生增加换位激励；
- 对长期处于边缘位置的学生给予优先补偿；
- 若缺少历史快照，该规则优雅降级为 `not_available`，不会中断求解流程。

---

## 📖 相关文档

- [邻座历史与同桌回避](pair-history.md)
- [排座规则手册](rules.zh.md)
- [班级项目工作流](project.zh.md)
