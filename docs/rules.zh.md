# 规则说明

规则文件是 JSON，分为 `hard` 和 `soft`。

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
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

如果求解器没有找到可行解，CLI 会输出学生人数、可用座位数、hard 规则数量，并提示可能需要检查固定座位、禁止相邻、最小距离和 disabled 座位。

## soft 规则

soft 规则是偏好，不保证一定满足。每条规则包含 `enabled` 和非负整数 `weight`。负数权重会报错，未识别的 soft rule 名称也会报错。

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

`seed` 控制可复现性。相同输入和 seed 应生成稳定结果。

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

fallback solver 和 OR-Tools solver 都会把 fair rotation 转换为单个“学生-座位”的启发式 cost：近期多次坐过同类位置会增加 cost，长期次数较少的学生会得到轻微补偿。该方法基于座位类别和历史次数，不保证绝对公平。

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

fallback solver 和 OR-Tools solver 都支持该规则。当前实现是启发式评分：它会倾向避免近期重复同桌/相邻，但不保证绝对最优。
