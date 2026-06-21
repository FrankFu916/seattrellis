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
    "score_balance": {"enabled": false, "weight": 1}
  }
}
```

| 规则 | 说明 |
| --- | --- |
| `vision_front` | 有视力需求的学生尽量靠前 |
| `height_back` | 身高较高的学生尽量靠后 |
| `randomize` | 使用 seed 生成可复现扰动 |
| `score_balance` | 相邻学生分数差异偏好 |

`seed` 控制可复现性。相同输入和 seed 应生成稳定结果。
