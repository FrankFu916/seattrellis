# 公平轮换历史

`history-report` 读取历史 snapshot，统计每名学生使用前排、后排、侧边、角落、靠窗、靠门、讲台侧和空调侧等类别的次数。

```bash
seattrellis history-report \
  --students examples/students.csv \
  --layout examples/classroom.json \
  --history-dir examples/history
```

`fair_rotation` soft rule 使用最近 `lookback` 次历史减少重复类别。历史缺失不会导致求解失败，只会使该评分维度不可用。

历史 snapshot 应与当前学生稳定 ID 和 layout seat ID 保持一致。

