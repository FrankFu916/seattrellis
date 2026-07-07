# 规则能力审计

更新日期：2026-07-06

本表以当前代码执行路径为准。只有模型、验证、求解、结果复核、测试和文档一致
时，规则才标记为“已实现”。仅能解析 JSON 不代表规则会影响排座结果。

| 规则 | 模型 | 验证 | 求解 | 评分/报告 | 状态 |
|---|---:|---:|---:|---:|---|
| `fixed_seats` | 是 | 是 | fallback / CP-SAT | hard constraint 复核 | 已实现 |
| `must_be_adjacent` | 是 | 是 | fallback / CP-SAT | hard constraint 复核 | 已实现 |
| `cannot_be_adjacent` | 是 | 是 | fallback / CP-SAT | hard constraint 复核 | 已实现 |
| `min_distance` | 是 | 是 | fallback / CP-SAT | hard constraint 复核 | 已实现 |
| `vision_front` | 是 | 输入提示 | fallback / CP-SAT | 独立评分 | 已实现 |
| `height_back` | 是 | 输入提示 | fallback / CP-SAT | 独立评分 | 已实现 |
| `randomize` | 是 | 是 | fallback / CP-SAT | 不适用 | 已实现 |
| `score_balance` | 是 | 输入提示 | fallback / CP-SAT | 独立评分 | 已实现，名称待澄清 |
| `fair_rotation` | 是 | 历史提示 | fallback / CP-SAT | 独立评分 | 已实现 |
| `avoid_recent_neighbors` | 是 | 历史提示 | fallback / CP-SAT | 独立评分 | 已实现 |
| `groups` | 是 | 否 | 否 | 否 | 仅模型 |
| `cooling` | 是 | 否 | 否 | 否 | 仅模型 |

## 已知语义问题

### `score_balance`

当前目标鼓励相邻学生之间出现较大的成绩差，更准确的产品含义是
“异质同伴搭配（peer mixing）”，不是让每行或每组平均成绩接近。v1.x 保留
字段以维持兼容，文档和 UI 应改用准确描述；未来可提供 `peer-mixing` preset
别名。

### `groups` 与 `cooling`

这两个字段已在模型、历史 changelog 和部分文档中出现，但当前求解路径没有读取
它们。进入新成绩分组和多期轮换功能前，必须选择并完成其中一种处理：

1. 实现验证、fallback、CP-SAT、结果复核和测试；
2. 明确标记 experimental/model-only，并在输入校验时发出 warning；
3. 按弃用策略迁移到新的版本化 RuleSet。

在处理完成前，不得把它们计入对外功能数量。

## 新规则准入清单

新增规则必须同时具备：

- 明确、可组合的领域语义；
- 缺少输入数据时的降级策略；
- fallback 和 OR-Tools 行为，或明确的 backend capability；
- 求解后独立 hard-constraint 复核；
- 评分解释和中英文文案（适用时）；
- 单元、属性和跨 backend 契约测试；
- schema 兼容与迁移说明。
