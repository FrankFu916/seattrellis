# 同桌与邻座历史

`pair-report` 汇总任意两名学生在历史中的同桌、横向、纵向、斜向、任意相邻和指定距离内出现次数。

```bash
seattrellis pair-report \
  --students examples/students.csv \
  --layout examples/classroom.json \
  --history-dir examples/history \
  --top 10
```

`avoid_recent_neighbors` soft rule 可使用这些记录降低近期重复关系。`cooling`
字段目前仍是 model-only：会被解析和保留，但不会影响求解或评分。历史规则不会
放松 fixed seats、adjacency 或 minimum-distance hard rules。
