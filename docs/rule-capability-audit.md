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
| `groups` | 是 | 是 | fallback / CP-SAT | hard constraint 复核 | 已实现（组内成对相邻/分离） |
| `cooling` | 是 | warning | 否 | 否 | model-only |

## 已知语义问题

### `score_balance`

当前目标鼓励相邻学生之间出现较大的成绩差，更准确的产品含义是
“异质同伴搭配（peer mixing）”，不是让每行或每组平均成绩接近。v1.x 保留
字段以维持兼容，文档和 UI 应改用准确描述；未来可提供 `peer-mixing` preset
别名。

### `groups` 与 `cooling`

`groups` 的 `together` 和 `separate` 会在共享规则编译器中展开为组内每一对学生的
`must_be_adjacent` 或 `cannot_be_adjacent` 硬约束，因此验证、fallback、OR-Tools、
native 复核和人工调整使用同一语义。组内成员超过两人时，要求每对成员都满足相应
条件；如果同时开启 `together` 和 `separate`，正常冲突检查会报告矛盾。

`cooling` 仍在模型中保存，并由 `validate`/`solve` 输出 model-only warning；普通
模式允许继续运行，`validate --strict` 会把 warning 视为失败。

在真正实现验证、fallback、OR-Tools、结果复核和测试前，不得把它们计入对外
功能数量。

## 新规则准入清单

新增规则必须同时具备：

- 明确、可组合的领域语义；
- 缺少输入数据时的降级策略；
- fallback 和 OR-Tools 行为，或明确的 backend capability；
- 求解后独立 hard-constraint 复核；
- 评分解释和中英文文案（适用时）；
- 单元、属性和跨 backend 契约测试；
- schema 兼容与迁移说明。
