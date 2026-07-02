# 同桌与邻座历史

`pair-report` 汇总任意两名学生在历史中的同桌、横向、纵向、斜向、任意相邻和指定距离内出现次数。

```bash
seattrellis pair-report \
  --students examples/students.csv \
  --layout examples/classroom.json \
  --history-dir examples/history \
  --top 10
```

`avoid_recent_neighbors` 与 `cooling` soft rules 可使用这些记录降低近期重复关系。它们不会放松 fixed seats、adjacency 或 minimum-distance hard rules。

