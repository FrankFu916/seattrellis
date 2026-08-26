# 输入格式

SeatTrellis 使用学生名单、教室布局和规则文件，也可以用一个本地 project 文件保存这些文件的相对路径和常用默认值。示例文件都在 `examples/`，只包含虚构数据。

v2 中这些文件通过 project 工作流（`project-*` 命令、工作台的班级项目面板）消费；
独立 CLI 命令则把学生、座位和规则内联到一份 problem JSON（`CoreSolveRequest`）
中求解，见[快速开始](quickstart.zh.md)。v1 时代的文件格式继续可读并自动迁移。

## 学生名单

CSV 和 Excel `.xlsx` / `.xlsm` 都由本地 Rust 导入器原生支持，无需安装任何 extra：

```bash
seattrellis_cli project-init --dir my-class   # 在已有 students.csv 的目录创建 project
seattrellis_cli project-validate --project my-class/seattrellis.project.json
```

旧版 `.xls` 请先另存为 `.xlsx` 或 CSV。

### Excel（.xlsx / .xlsm）导入边界

Excel 导入读取工作簿的**第一个工作表**，并遵守以下边界：

- 支持的单元格取值：共享字符串、内联字符串、公式缓存值（`str`）、数字与布尔
  （带缓存结果）；**没有缓存值的公式单元格会报错**；
- 文本单元格按文本读取，`001` 这类前导零编号不会丢失；
- 上限：文件不超过 20 MiB（解压后的 XML 部件同样受限）、数据行不超过
  10,000、列数不超过 256；超限或加密工作簿会明确报错。

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

可以先运行轻量预检（通过 project 工作流，或把数据内联进 problem JSON 后运行
`seattrellis_cli validate --problem`）：

```bash
seattrellis_cli project-validate --project my-class/seattrellis.project.json --strict
```

`project-validate` 只检查输入和明显冲突，不生成座位表。加 `--strict` 时，warning 也会导致命令失败。

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

## Project workspace JSON

project 文件是本地文件型工作流的配置入口，推荐命名为 `seattrellis.project.json` 或 `project.seattrellis.json`：

```json
{
  "kind": "seattrellis_project",
  "schema_version": 1,
  "name": "Demo Class",
  "students": "students.csv",
  "layout": "classroom.json",
  "rules": "rules_multi_candidate.json",
  "history_dir": "history",
  "outputs_dir": "outputs",
  "default_candidates": 5,
  "default_candidate": "recommended",
  "default_export_format": "html"
}
```

`students`、`layout`、`rules` 必填；`history_dir` 可省略；其余字段有默认值。所有路径必须是相对路径，并相对于 project 文件所在目录解析，而不是相对于安装目录。`project-solve` 会在需要时创建 `outputs_dir`，但不会自动创建或伪造学生、layout、rules、history 输入。

```bash
seattrellis_cli project-info --project examples/project.seattrellis.json
seattrellis_cli project-validate --project examples/project.seattrellis.json
seattrellis_cli project-solve --project examples/project.seattrellis.json
seattrellis_cli project-export --project examples/project.seattrellis.json
```

project 文件只保存路径和默认配置，不保存学生名单、成绩、备注、座位偏好或 snapshot 内容。真实输入和输出仍应放在 `.gitignore` 覆盖的私有目录中；不要因为 project 文件本身可分享，就误把它引用的真实数据一并提交。

## 历史 snapshot

`history-report`、`pair-report` 和 `validate --history` / `--history-dir` 读取
SeatTrellis JSON snapshot（v1 时代的 snapshot 由迁移路径自动处理）。历史分析只
依赖 JSON snapshot，不需要 Excel、PNG、Streamlit、SQLite 或数据库。

```bash
seattrellis_cli history-report --problem problem.json --history-dir examples/history
seattrellis_cli pair-report --problem problem.json --history-dir examples/history
seattrellis_cli validate --problem problem.json --history-dir examples/history --preset daily
```

历史 snapshot 会用当前学生名单和当前 layout 解释：

- 多个 snapshot 按传入顺序或目录文件名排序组成历史序列；
- 某个历史 snapshot 缺少当前学生时，该学生在该周次被跳过并产生 warning；
- 历史 snapshot 引用当前 layout 中不存在的座位时，该记录标记为 `unknown` 并产生 warning；
- 历史 snapshot 引用当前 layout 中 `enabled=false` 的座位时，该记录保留为历史座位，但不参与位置类别统计；
- pair history 使用当前 layout 的 `row` / `col`、adjacency graph 和 custom edges 判断 `desk_mate`、`horizontal`、`vertical`、`diagonal`、`adjacent_any`、`within_distance`；
- pair history 中引用 `enabled=false` 座位时，新排座不会使用该座位，但历史关系会尽量按 row/col 坐标统计并记录 warning；
- `within_distance` 使用 Chebyshev 距离，默认阈值为 `2`；
- v0.1.0 / v0.1.1 / v0.1.2 / v0.2.0 / v0.2.1 snapshot 仍可读取；v0.2.2 不修改普通 snapshot schema。

`examples/history/` 只包含虚构历史数据。真实历史座位记录应脱敏并保存在忽略目录中，不要提交到公开仓库。

## Candidate set JSON

v2 的多方案生成使用 `candidates` 命令，输出 `api_version: 2` 的候选报告（每个
候选带 `candidate_id`、assignment、plan score 明细和 hard-constraint 摘要），
推荐方案是加权总分最高的 hard-valid 候选：

```bash
seattrellis_cli candidates --problem problem.json --count 5 > outputs/candidates.json
```

v1 时代的 candidate set（`kind: "candidate_set"`，`schema_version: "0.2.2"`，
每个候选内嵌完整 snapshot）继续可读；project 工作流（`project-export
--candidate <id>`）对这类产物按 `candidate_id` 选择方案后导出，默认使用
`recommended_candidate_id`。如果 ID 不存在，CLI 会列出可用 ID 并返回友好错误。

`project-solve --report` 写出的 `kind: "plan_comparison_report"` 是比较报告，
不是可导出的座位 snapshot。真实 candidate set、比较报告和导出文件都应放在已
忽略的 `outputs/` 等私有目录，不要提交到公开仓库。

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

规则 JSON（`RuleSet`）内联在 problem JSON 的 `rules` 字段中，或由 project 的
`rules` 路径引用。v2 的 `validate --preset <name>` 只做场景数据缺失检查
（history/score/height/vision warning），不合并 preset 规则。完整规则与预设
说明见 [rules.zh.md](rules.zh.md)。

注意：problem JSON 的原生求解路径只消费**顶层索引对形式**的 hard 约束——
`fixed_seats` / `must_be_adjacent` / `cannot_be_adjacent` / `min_distance`
（学生用列表下标引用，见[快速开始](quickstart.zh.md)的 problem.json 示例）。
`rules.hard` 中的字符串引用形式不会被原生路径消费；非空的 `rules.hard`
会在 solve 请求中被明确拒绝并给出上述指引。字符串形式由工作台/服务端在生成
problem 之前解析合并为顶层索引对。
