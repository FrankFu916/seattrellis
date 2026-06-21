# 输入格式

SeatTrellis 使用三个输入文件：学生名单、教室布局和规则文件。示例文件都在 `examples/`，只包含虚构数据。

## 学生名单

基础安装支持 CSV。安装 `excel` extra 后支持 Excel `.xlsx` / `.xlsm`：

```bash
python -m pip install -e ".[excel]"
```

旧版 `.xls` 请先另存为 `.xlsx` 或 CSV。

至少需要提供 `student_id` 或 `name` 之一。其他字段都是可选字段：

| 字段 | 说明 |
| --- | --- |
| `student_id` | 学生稳定编号，可选但推荐 |
| `name` | 学生显示姓名 |
| `gender` | 性别或其他分组信息 |
| `height_cm` | 身高，必须是正数 |
| `score` | 成绩或综合分，必须是有限数字 |
| `vision` | 视力信息，例如 `poor`、`0.8` |
| `tags` | 标签，可用逗号、分号、顿号或竖线分隔 |
| `needs` | 特殊需求，可用同样分隔符 |
| `notes` | 备注 |

导入器会检查：

- 学生表不能为空；
- 表头必须包含 `student_id` 或 `name` 中至少一列；
- 每行至少有 `student_id` 或 `name`；
- 如果存在 `name` 列，非空学生行中的 `name` 不能为空；
- `student_id` 不能重复；
- `height_cm`、`score` 不能是非法数值，错误会尽量指出列名和行号；
- 未识别列会保存在学生的 `attributes` 中。
- 没有 `student_id` 的学生会使用 `name` 作为稳定内部 ID，并在 `validate` 中给出 warning。

可以先运行轻量预检：

```bash
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

`validate` 只检查输入和明显冲突，不生成座位表。加 `--strict` 时，warning 也会导致命令失败。

## 教室布局 JSON

布局由 seat nodes 组成，不要求是完整矩阵。

```json
{
  "layout_id": "fictional-room",
  "name": "Fictional Classroom",
  "seats": [
    {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
    {"seat_id": "R1C2", "row": 1, "col": 2, "enabled": false, "zone": "aisle"}
  ],
  "adjacency": {
    "include_horizontal": true,
    "include_vertical": false,
    "include_diagonal": false,
    "custom_edges": []
  }
}
```

座位字段：

| 字段 | 说明 |
| --- | --- |
| `seat_id` | 必填，座位唯一 ID |
| `row` / `col` | 必填，正整数 |
| `x` / `y` | 可选坐标，默认使用 `col` / `row` |
| `enabled` | 可选，`false` 表示不可用座位 |
| `zone` | 可选，区域标签 |
| `near_window` / `near_door` / `near_platform` / `near_ac` | 可选布尔字段 |
| `tags` | 可选标签列表 |
| `attributes` | 可选扩展属性 |

布局校验会检查空 `seat_id`、重复 `seat_id`、`row` / `col` 类型、空布局、没有可用座位、以及 `custom_edges` 引用不存在或不可用座位。跨文件预检还会检查学生人数是否超过可用座位数，以及规则是否把学生固定到 `enabled=false` 的座位。

错误示例可参考 `examples/invalid/duplicate_student_id.csv`、`examples/invalid/duplicate_seat_id.json` 和 `examples/invalid/not_enough_seats.json`。

## 规则 JSON

规则说明见 [rules.zh.md](rules.zh.md)。
