# 场景 Presets

Preset 是一份标准 `RuleSet` 基础配置，不是独立求解器。传入 `--rules` 时，用户 JSON 中显式出现的字段递归覆盖 preset。

| 名称 | 适用场景 | 主要依赖 |
|------|----------|----------|
| `random` | 快速随机排座 | 无 |
| `exam` | 考试或测验 | hard rules 可选 |
| `daily` | 日常综合排座 | history、score、height、vision 可选 |
| `fair-rotation` | 前后排公平轮换 | history |
| `neighbor-aware` | 减少重复同桌/邻座 | history |
| `balanced` | 成绩异质搭配 | score |
| `height-aware` | 身高偏好 | height |
| `vision-friendly` | 视力或靠前需求 | vision/needs |

缺少依赖字段时只停用受影响的 soft rule，并生成 warning；hard rules 不会被自动放松。

