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

## 历史 snapshot

`solve --history`、`solve --history-dir` 和 `history-report` 读取 SeatTrellis JSON snapshot。历史分析只依赖 JSON snapshot，不需要 Excel、PNG、Streamlit、SQLite 或数据库。

```bash
seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --history-dir examples/history --output outputs/fair.snapshot.json
```

历史 snapshot 会用当前学生名单和当前 layout 解释：

- 多个 snapshot 按传入顺序或目录文件名排序组成历史序列；
- 某个历史 snapshot 缺少当前学生时，该学生在该周次被跳过并产生 warning；
- 历史 snapshot 引用当前 layout 中不存在的座位时，该记录标记为 `unknown` 并产生 warning；
- 历史 snapshot 引用当前 layout 中 `enabled=false` 的座位时，该记录保留为历史座位，但不参与位置类别统计；
- v0.1.0 / v0.1.1 / v0.1.2 snapshot 仍可读取；v0.2.0 snapshot 可能在 `metadata.fairness` 中加入公平性摘要。

`examples/history/` 只包含虚构历史数据。真实历史座位记录应脱敏并保存在忽略目录中，不要提交到公开仓库。

## 座位位置类别

位置类别用于 `history-report` 和 `fair_rotation`。当前规则如下：

- `row` 越小越靠前；
- 如果 `zone` 明确为 `front`、`middle` 或 `back`，优先使用该显式区域；否则按当前 layout 的可用座位 row 推断：最小 row 为 `front`，最大 row 为 `back`，其他 row 为 `middle`；只有一行时推断为 `middle`；
- `side` 使用当前 layout 中可用座位的最小 col 或最大 col；
- `corner` 使用可用座位的 row 边界和 col 边界交点；
- `near_window`、`near_door`、`near_platform`、`near_ac` 只由显式布尔字段决定，字段不存在时默认 `false`；
- 异形教室按实际 seat nodes 处理，缺失座位不会被补成矩阵座位；
- `enabled=false` 的座位不参与分配统计和类别边界计算。

## 规则 JSON

规则说明见 [rules.zh.md](rules.zh.md)。
