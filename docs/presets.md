# 场景 Presets

Preset 是一份标准 `RuleSet` 基础配置，不是独立求解器。v2 中把对应 preset 的
规则 JSON 内联到 problem 的 `rules` 字段（或由 project 引用）即应用该场景；
`validate --preset <name>` 检查问题缺少哪些首选数据并给出 warning。

| 名称 | 适用场景 | 主要依赖 |
|------|----------|----------|
| `random` | 快速随机排座 | 无 |
| `exam` | 考试或测验 | hard rules 可选 |
| `daily` | 日常综合排座 | history、score、height、vision 可选 |
| `fair-rotation` | 前后排公平轮换 | history |
| `neighbor-aware` | 减少重复同桌/邻座 | history |
| `balanced` / `peer-mixing` | 成绩异质搭配 | score |
| `score-high-front` / `score-high-back` | 高分靠前/靠后 | score |
| `row-score-balanced` / `group-score-balanced` | 行/组内成绩均衡 | score |
| `mentor-pairing` | 师徒结对 | score |
| `height-aware` | 身高偏好 | height |
| `vision-friendly` | 视力或靠前需求 | vision/needs |

缺少依赖字段时只停用受影响的 soft rule，并生成 warning；hard rules 不会被
自动放松。
