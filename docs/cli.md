# CLI 命令参考

运行 `seattrellis_cli --help` 查看当前安装版本的完整参数；`seattrellis_cli doctor`
检查环境（二进制/版本/临时目录）。

## 命令总览

| 命令 | 用途 |
|------|------|
| `doctor` | 检查环境（二进制名、版本、core API 版本、临时目录可写性） |
| `validate` | 校验 solve-request JSON（`CoreSolveRequest`），不运行求解 |
| `precheck` | 报告候选座位域和不可行原因 |
| `audit` | 审计已求解方案：hard 规则状态 + soft 评分明细 |
| `score` | 对固定 assignment 输出 PlanScore 明细 |
| `candidates` | 生成多样化的候选方案集 |
| `history-report` | 汇总历史座位类别 |
| `pair-report` | 汇总同桌和邻座历史 |
| `repair` | 保留锁定座位的前提下重新求解 snapshot |
| `edit` | 对 snapshot 或候选集执行人工调整命令 |
| `project-init` | 在已有 students/layout/rules 的目录创建 project 文件 |
| `project-list` | 列出根目录下的最近项目 |
| `project-info` | 显示 project 工作区摘要 |
| `project-validate` | 校验 project 及其文件 |
| `project-solve` | 求解 project 工作区 |
| `project-export` | 导出已保存的 project 方案（不会重新求解） |
| `project-rotate` | 为 project 生成未来排座时段 |
| `project-edit` | 对 project 座位产物执行人工调整 |
| `project-repair` | 保留锚点重新求解 project 产物 |
| `project-privacy` | 扫描 project 中的敏感字段 |
| `project-pack` | 把 project 备份为 `.seattrellis.zip` |
| `project-restore` | 从 bundle 恢复 project |
| `schema-list` | 列出 v2 产物注册表（kind + version + 是否可迁移） |
| `schema-export` | 导出某类产物的 JSON Schema |
| `schema-migrate` | 校验并重写带版本号的 JSON 产物 |
| `solve` | 求解排座问题并打印结果摘要 |
| `export` | 把已求解方案渲染为 SVG/HTML/PNG/PDF/XLSX/DOCX/PPTX |
| `help` | 显示帮助 |

`--version` / `-V` 显示版本。所有命令都支持内联 `--help`。

## 退出状态（冻结表）

- `0`：成功（`Solved`）；
- `2`：无效输入或参数（`InvalidInput`）；
- `3`：确认不可行（`ProvenInfeasible`）；
- `4`：超时（`Timeout`）；
- `5`：未知（`Unknown`，启发式耗尽等）；
- `70`：内部错误（`InternalError`）；
- `130`：用户取消（`Cancelled`）。

启发式耗尽只能报告 `Unknown`（5），绝不能伪装成 `ProvenInfeasible`（3）；有合法
方案时即使超时也报告 `Solved`（0）。`export` 失败按内部错误（70）处理。

## 求解

```bash
seattrellis_cli solve --problem problem.json [--seed <n>] [--time-limit <sec>] [--output <result.json>]
```

`--seed` 覆盖问题文件中的 seed；`--time-limit` 是墙钟预算，预算耗尽且没有完整
状态空间扫描时报告 `Timeout`。结果摘要打印到 stdout，`--output` 同时写出完整
`CoreSolveResponse` JSON。相同输入和 seed 会得到稳定结果。

## 校验与检查

```bash
seattrellis_cli validate --problem problem.json [--preset <name>] [--history <snapshot.json>]... [--history-dir <dir>] [--strict]
seattrellis_cli precheck --problem problem.json
seattrellis_cli audit --problem problem.json --solution result.json
seattrellis_cli score --problem problem.json --assignment <json> [--latest-snapshot <file>] [--diversity <f>]
```

- `validate` 只检查输入与明显冲突，不生成座位表；`--preset` 附带场景数据缺失
  warning（如 `daily` 缺少 history/score/height/vision）；`--strict` 把 warning
  当作失败。`validate` 只评判输入，失败一律退出码 2。
- `precheck` 报告每名学生的候选座位域与不可行原因。
- `audit` 输出 hard 规则状态、soft 评分明细和主要贡献项。
- `score` 的 `--assignment` 是内联 `[[student, seat], ...]` 索引对 JSON。

## 候选方案

```bash
seattrellis_cli candidates --problem problem.json [--count <n>] [--latest-snapshot <file>]
```

`--count` 为 1–20（默认 5）。每个候选必须满足全部 hard 约束，并带独立 snapshot、
总分和评分明细；推荐方案是加权总分最高的 hard-valid 候选。候选空间不足时保留已
找到的方案并给出 warning。

## 历史报告

```bash
seattrellis_cli history-report --problem problem.json [--history <snapshot.json>]... [--history-dir <dir>] [--output <file>]
seattrellis_cli pair-report --problem problem.json [--history <snapshot.json>]... [--history-dir <dir>] [--top <n>] [--within-distance <n>]
```

`--history` 可重复传入；`--history-dir` 扫描目录中的 `*.snapshot.json` 文件并加入
`--history`。`history-report --output` 写出 JSON 报告；`pair-report` 的 `--top`
限制高频学生对展示数量（默认 10），`--within-distance` 是 Chebyshev 距离阈值
（默认 2）。

## 人工调整

`edit` 对普通 snapshot 或 candidate set 中的某个候选执行按顺序排列的人工操作，
并写出新的草稿 snapshot：

```bash
seattrellis_cli edit \
  --snapshot outputs/neighbor-aware.snapshot.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R4C3 \
  --output outputs/neighbor-aware-edited.snapshot.json
```

输入 candidate set 时默认编辑 recommended candidate，也可以用 `--candidate` 指定。

支持的 operation 格式：

- `swap:STU001:STU002`
- `move:STU003:R2C2`
- `batch-move:STU001=R1C2,STU002=R1C1`
- `seat:STU003:R2C2`
- `unseat:STU004`
- `lock-student:STU001`
- `unlock-student:STU001`
- `lock-seat:R1C1`
- `unlock-seat:R1C1`

多次 `--operation` 按命令行顺序执行。默认即使调整后违反 hard 约束也会写出草稿
并列出违反项；加 `--strict` 后违反 hard 约束则命令失败且不写出 snapshot。锁定
状态记录在 `metadata.lock_state`，后续 `repair` 默认继承。

可复用、可审计的调整记录用 `--operations-file` 读取 JSON 文件（操作对象数组，
或包含 `operations` 数组的对象）；文件操作先执行，再执行命令行中的 `--operation`：

```json
{
  "operations": [
    {
      "kind": "swap_students",
      "payload": {"first_student": "STU001", "second_student": "STU002"}
    },
    {
      "kind": "lock_seat",
      "payload": {"seat_id": "R4C3"}
    }
  ]
}
```

`batch_move` 是单条原子操作：学生和目标座位不得重复；目标座位若已占用，其占用者
也必须包含在同一批次中。任一映射未知、锁定或冲突时整个批次失败，不留下部分修改。

## 锁定后重排与局部修复

`repair` 把编辑草稿交回求解器处理，输出 snapshot 在 `metadata.repair` 中记录
锁定、可变学生、实际变化学生和历史数量：

```bash
seattrellis_cli repair \
  --problem problem.json \
  --snapshot outputs/neighbor-aware-edited.snapshot.json \
  --affected STU001 \
  --affected STU002 \
  --lock-seat R4C3 \
  --output outputs/neighbor-aware-repaired.snapshot.json
```

提供 `--affected` 时，程序自动加入与其存在 hard rule 或座位相邻关系的一阶学生，
其余当前已入座学生会固定在草稿位置。未提供时所有未锁定学生都可重新安排。
`--lock-student` 保留该学生当前座位；`--lock-seat` 保留当前座位上的学生，空座则
临时预留为空座。默认继承 `metadata.lock_state` 中的锁；`--ignore-saved-locks`
可忽略它们。`--history` 与 `--history-dir` 可传入历史方案，保证公平轮换和近期
邻座规则在局部修复时仍生效。

## Project 工作流

```bash
seattrellis_cli project-init --dir <directory>
seattrellis_cli project-list [--root <dir>] [--limit <n>]
seattrellis_cli project-info|project-validate|project-solve|project-export --project <project.json>
seattrellis_cli project-rotate --project <project.json> [--periods <n>]
seattrellis_cli project-edit --project <project.json> [--snapshot <file>] --operation <op>...
seattrellis_cli project-repair --project <project.json> [--snapshot <file>] [--affected <key>]...
seattrellis_cli project-privacy --project <project.json> [--no-include-outputs]
seattrellis_cli project-pack --project <project.json> --output <bundle.zip> [--force]
seattrellis_cli project-restore --bundle <bundle.zip> --output-dir <dir> [--force]
```

`project-init` 在已经包含 `students.csv` / `layout.json` / `rules.json` 的目录中
创建 `seattrellis.project.json`。`project-solve` 支持 `--candidates <n>` 与
`--report <file>`（方案比较报告）；`project-export` 渲染的是 `project-solve
--output` 保存的方案，**导出永远不会重新求解**，并支持
`svg|html|print-html|png|pdf|xlsx|docx|pptx` 格式（默认使用 project 的
`default_export_format`）。`project-rotate` 的 `--periods` 为 1–20（默认 4）。
`project-validate --strict` 把 warning 当作失败。

## Schema 工具

```bash
seattrellis_cli schema-list
seattrellis_cli schema-export --kind <kind> [--output <file>]
seattrellis_cli schema-migrate --input <file> [--output <file> | --in-place] [--dry-run]
```

`schema-list` 列出 12 类 v2 产物（studentroster、classroomlayout、ruleset、
seatingsnapshot、candidateset、plancomparison、historyarchive、rotationplan、
editingoperationlog、project、projectbundlemanifest、exportpreset），版本均为
`v2`。`schema-export --kind` 写出对应 JSON Schema。`schema-migrate` 校验并重写
带版本号的 JSON 产物；`--dry-run` 只验证不写盘，`--in-place` 覆盖原文件前先创建
`.bak` 备份（重复运行时依次使用 `.bak.1`、`.bak.2` 等）。v1 时代的 snapshot、
candidate set、project 等文件会迁移到 v2 版本。

## 导出

```bash
seattrellis_cli export --problem problem.json --solution result.json \
  --format <svg|html|png|pdf|xlsx|docx|pptx> --output <file> [--template <public|teacher>]
```

`--solution` 是 `solve --output` 写出的结果 JSON；导出前会用独立 validator 复核
方案，绝不会导出无效方案。所有格式都经过共享的隐私过滤层：默认隐藏成绩、备注、
特殊需求、身高和视力字段；`--template public` 额外匿名化姓名。`print-html` 格式
通过 `project-export` 使用（渲染已保存方案）。

## 相关文档

命令示例见[快速开始](quickstart.zh.md)，退出状态与求解状态语义见
[版本策略](versioning.md)。
