# 规则说明

[English](rules.md) / [简体中文](rules.zh.md)

SeatTrellis v2.0.0 的规则文件是 JSON，分为 `hard` 和 `soft`。

## hard 规则

hard 规则必须满足，否则求解失败。

```json
{
  "seed": 42,
  "hard": {
    "fixed_seats": [{"student": "STU001", "seat_id": "R1C1"}],
    "must_be_adjacent": [{"students": ["STU002", "STU003"]}],
    "cannot_be_adjacent": [{"students": ["STU004", "STU005"]}],
    "min_distance": [{"students": ["STU006", "STU007"], "distance": 2, "metric": "euclidean"}]
  }
}
```

| 规则 | 说明 |
| --- | --- |
| `fixed_seats` | 固定某个学生到某个可用座位 |
| `must_be_adjacent` | 两个学生必须相邻 |
| `cannot_be_adjacent` | 两个学生不能相邻 |
| `min_distance` | 两个学生之间至少保持指定距离 |

`student` 可引用 `student_id` 或 `name`。`seat_id` 必须存在且座位必须 `enabled=true`。

当前会检查：

- 规则引用不存在学生；
- 规则引用不存在或不可用座位；
- 同一学生被固定到多个座位；
- 同一座位被固定给多个学生；
- 同一学生对同时出现在 `must_be_adjacent` 和 `cannot_be_adjacent`；
- `min_distance` 与 `must_be_adjacent` 对同一学生对明显冲突；
- 固定座位已经明显违反相邻或距离规则；
- 未识别规则字段会作为错误报告。

预检命令：

```bash
seattrellis validate --problem problem.json
seattrellis project-validate --project my-class/seattrellis.project.json --strict
```

如果求解器没有找到可行解，CLI 会输出学生人数、可用座位数、hard 规则数量，并提示可能需要检查固定座位、禁止相邻、最小距离和 disabled 座位。

## soft 规则

soft 规则是偏好，不保证一定满足。每条规则包含 `enabled` 和非负整数 `weight`
（取值范围 0–1,000,000）。负数权重会报错，超过 1,000,000 的权重也会报错，
未识别的 soft rule 名称同样会报错。

```json
{
  "soft": {
    "vision_front": {"enabled": true, "weight": 20},
    "height_back": {"enabled": true, "weight": 1},
    "randomize": {"enabled": true, "weight": 1},
    "score_balance": {"enabled": false, "weight": 1},
    "fair_rotation": {
      "enabled": true,
      "weight": 10,
      "avoid_repeating_categories": ["front", "back", "side", "corner", "near_window", "near_door", "near_ac"],
      "lookback": 4
    },
    "avoid_recent_neighbors": {
      "enabled": true,
      "weight": 10,
      "lookback": 4,
      "relation_types": ["desk_mate", "adjacent_any"],
      "max_recent_count": 1,
      "within_distance": 2
    }
  }
}
```

| 规则 | 说明 |
| --- | --- |
| `vision_front` | 有视力需求的学生尽量靠前 |
| `height_back` | 身高较高的学生尽量靠后 |
| `randomize` | 使用 seed 生成可复现扰动 |
| `score_balance` | 相邻学生分数差异偏好 |
| `fair_rotation` | 基于历史 snapshot 的座位类别轮换偏好 |
| `avoid_recent_neighbors` | 基于历史 snapshot 的同桌/相邻关系回避偏好 |

`seed` 固定伪随机序列。fallback 完成固定尝试预算时，相同输入和 seed 会生成稳定
结果；如果墙钟时间限制提前停止求解，不同机器可能完成不同数量的尝试，此时结果
不保证完全一致。snapshot 的 `metrics.stopped_by_time_limit` 会记录这一情况。

## 场景 preset

preset 是标准 `RuleSet` 基础配置，不是新的规则格式。v2 中把对应 preset 的规则
JSON 内联到 problem 的 `rules` 字段（或由 project 引用）即应用该场景；
`validate --preset <name>` 会检查问题缺少哪些该场景的首选数据并给出 warning。
当前内置场景：

| Preset | 重点 |
| --- | --- |
| `random` | 只启用可复现随机扰动 |
| `exam` | 更强的可复现随机扰动；考试所需的间距、固定座位等仍由显式 hard rules 提供 |
| `daily` | 综合视力、身高、成绩混合、公平轮换和近期邻座回避 |
| `fair-rotation` | 优先历史座位类别轮换 |
| `neighbor-aware` | 优先减少近期重复同桌/相邻 |
| `balanced` | 优先相邻成绩层次混合 |
| `height-aware` | 优先高个靠后 |
| `vision-friendly` | 优先有视力/靠前需求的学生 |

```bash
seattrellis validate --problem problem.json --preset daily --history-dir examples/history
```

preset 与用户 rules 的叠加发生在工作台/服务端：先生成 preset 的完整标准 rules，
再递归应用用户 JSON 中明确提供的字段（合并后的完整 `RuleSet` 可在求解前预览并
下载）。例如下面的 overlay 会保留 `daily` 的其他 soft rules，但关闭随机扰动并
添加固定座位：

```json
{
  "hard": {
    "fixed_seats": [{"student": "STU001", "seat_id": "R1C1"}]
  },
  "soft": {
    "randomize": {"enabled": false, "weight": 0}
  }
}
```

CLI 不合并 preset：它接收的 problem JSON 或 project 中应包含最终生效的完整
`RuleSet`，`--preset` 只用于数据缺失检查。preset 不会生成或放宽 hard rules；
hard rules 继续由预检、求解器和候选复核执行。缺少历史、成绩、身高或视力/靠前
标记时，`validate` 会给出 warning，相关 soft rule 保持可解释地降级，对应评分
显示 `not_available`。`validate --strict` 会把这些 warning 当作失败。

## fair_rotation

`fair_rotation` 是 soft rule，不会覆盖 hard rules。固定座位、必须相邻、禁止相邻、最小距离等 hard rules 仍然优先；如果没有传入历史 snapshot，`fair_rotation` 会自动无效并在求解结果 metrics 中提示无历史可用，不会报错。`weight=0` 时不影响求解。

字段：

| 字段 | 说明 |
| --- | --- |
| `enabled` | 是否启用公平轮换 |
| `weight` | 非负整数权重，越大越倾向避免重复历史类别 |
| `avoid_repeating_categories` | 要避免连续重复的座位类别 |
| `lookback` | 计算近期重复时查看最近多少个历史 snapshot，`0` 表示不使用近期重复惩罚 |

当前支持的类别包括 `front`、`back`、`middle`、`side`、`corner`、`near_window`、`near_door`、`near_platform`、`near_ac`。默认避免重复 `front`、`back`、`side`、`corner`、`near_window`、`near_door`、`near_ac`。

求解器会把 fair rotation 转换为单个"学生-座位"的启发式 cost：近期多次坐过同类
位置会增加 cost，长期次数较少的学生会得到轻微补偿。该方法基于座位类别和历史
次数，不保证绝对公平。

## avoid_recent_neighbors

`avoid_recent_neighbors` 是 soft rule，用历史 snapshot 统计学生两两之间近期是否反复同桌或相邻，并在下一次排座时给重复关系增加 cost。它不会覆盖 hard rules：如果 `must_be_adjacent` 要求两名学生相邻，或者固定座位导致两名学生相邻，hard rule 仍然优先；如果 `cannot_be_adjacent` 或 `min_distance` 禁止某种关系，历史回避也不能让它失效。没有传入历史 snapshot 时，该规则自动无效并在 metrics 中提示无 pair history，不会报错。`weight=0` 时不影响求解。

示例：

```json
{
  "soft": {
    "avoid_recent_neighbors": {
      "enabled": true,
      "weight": 10,
      "lookback": 4,
      "relation_types": ["desk_mate", "adjacent_any"],
      "max_recent_count": 1,
      "within_distance": 2
    }
  }
}
```

字段：

| 字段 | 说明 |
| --- | --- |
| `enabled` | 是否启用近期同桌/相邻回避 |
| `weight` | 非负整数权重，越大越倾向避免重复关系 |
| `lookback` | 只统计最近多少个历史 snapshot，`0` 表示不使用近期关系惩罚 |
| `relation_types` | 要回避的关系类型列表 |
| `max_recent_count` | 最近历史中超过该次数后开始惩罚，例如 `1` 表示第 2 次及以后会增加 cost |
| `within_distance` | `within_distance` 关系使用的 Chebyshev 距离阈值，默认 `2` |

关系类型：

| 类型 | 定义 |
| --- | --- |
| `desk_mate` | 默认等同于同一 row 且 col 差值为 1 的横向相邻，预留未来支持自定义同桌组 |
| `horizontal` | 同一 row，col 差值为 1 |
| `vertical` | 同一 col，row 差值为 1 |
| `diagonal` | row 差值为 1 且 col 差值为 1 |
| `adjacent_any` | 横向、纵向、斜向或当前 layout adjacency graph / custom edge 中定义的相邻 |
| `within_distance` | row/col 的 Chebyshev 距离小于等于 `within_distance` |

关系历史按当前学生名单和当前 layout 解释，不假设座位形成完整矩阵。历史 snapshot 中缺少当前学生会跳过并记录 warning；引用当前 layout 中不存在的座位会跳过相关 pair 并记录 warning；引用 `enabled=false` 座位时，新排座不会使用该座位，但历史关系会尽量按 row/col 坐标统计并记录 warning。

求解器支持该规则。当前实现是启发式评分：它会倾向避免近期重复同桌/相邻，但不保证绝对最优。

## cooling

`cooling` 是更严格的近期关系回避目标。在 `cooling_period` 个历史 snapshot
内再次出现所选关系时增加惩罚，相当于 `max_recent_count=0` 的
`avoid_recent_neighbors`。它不会覆盖 fixed seats、相邻和最小距离等 hard rules；
没有历史记录时目标会在公平性摘要中标记为未生效。若同时启用
`avoid_recent_neighbors`，两者会合并为更严格的历史窗口和关系集合，权重相加。

```json
{
  "soft": {
    "cooling": {
      "enabled": true,
      "weight": 12,
      "cooling_period": 3,
      "relation_types": ["desk_mate", "adjacent_any"],
      "within_distance": 2
    }
  }
}
```

| 字段 | 说明 |
| --- | --- |
| `cooling_period` | 需要避开的最近历史期数，必须为正整数 |
| `relation_types` | `desk_mate`、`adjacent_any` 等关系类型 |
| `within_distance` | `within_distance` 关系使用的 Chebyshev 距离阈值 |

求解器使用相同的 pair history 成本；结果评分、候选比较和公平性
摘要也会复用同一规则，因此不会出现“求解时生效、报告时消失”的情况。

## 多方案生成与评分

多方案模式不会新增或放宽规则。`candidates --count N` 会先按正常流程应用全部
hard constraints 和 soft costs，再通过不同 seed 与"禁止完整重复上一候选
assignment"的约束继续求解。hard constraints 始终绝对优先；候选生成与推荐排序
都是启发式，不保证找到全部可行方案或全局最优方案。

```bash
seattrellis candidates \
  --problem problem.json \
  --count 5 \
  > outputs/candidates.json
```

（seed 来自 problem JSON；`--latest-snapshot` 可为每个候选的 stability 维度提供
最近历史。project 工作流使用 `project-solve --candidates N --report
outputs/plan-report.json` 生成方案比较报告。）

评分维度均为 0–100，高分表示更符合该维度。总分只使用 `status: "available"` 的维度，并按对应 soft-rule weight 加权；`diversity_score` 使用 `randomize.weight`，`stability_score` 使用固定比较权重 1。hard constraint 校验失败的方案不会进入 candidate set。

| 维度 | 含义 | 不可用条件示例 |
| --- | --- | --- |
| `fair_rotation_score` | 当前方案的历史座位类别轮换代价，代价越低分越高 | 未启用规则、无历史 |
| `avoid_recent_neighbors_score` | 当前同桌/相邻组合的近期重复代价，代价越低分越高 | 未启用规则、无 pair history |
| `score_balance_score` | 相邻座位之间不同成绩层次的混合程度 | 未启用规则、成绩数据不足 |
| `height_preference_score` | 身高排序与前后排位置的匹配程度 | 未启用规则、身高或 row 不足 |
| `vision_preference_score` | 有视力需求学生靠前的程度 | 未启用规则、无相应学生 |
| `diversity_score` | 与其他候选相比，有多少学生更换了座位 | 只有一个候选 |
| `stability_score` | 相比最近历史 snapshot 保持原座位的比例 | 无历史 snapshot |
| `hard_constraint_summary` | assignment 完整性与全部 hard rules 的复核 | 始终执行 |

`not_available` 表示当前输入无法诚实计算该维度，不等于 0 分。recommended candidate 是 hard-valid 候选中加权总分最高者；同分时按 `candidate_id` 排序，保证选择稳定。
