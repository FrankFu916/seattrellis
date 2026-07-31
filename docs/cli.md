# CLI 命令参考

运行 `seattrellis --help` 查看当前安装版本的完整参数。

| 命令 | 用途 |
|------|------|
| `init-demo` | 创建虚构示例文件 |
| `presets list/show/export` | 查看或导出内置规则 preset |
| `validate` | 校验输入、字段和明显规则冲突 |
| `solve` | 生成 snapshot 或 candidate set |
| `edit` | 对已有 snapshot 或候选方案执行人工调整命令 |
| `export` | 导出选定方案 |
| `history-report` | 汇总学生座位类别历史 |
| `pair-report` | 汇总同桌和邻座历史 |
| `project-init/info/validate/solve/edit/export` | 管理本地 project 工作流 |
| `schema list/export/migrate` | 查看 JSON Schema、导出 schema 文件、规范化版本化 JSON |
| `doctor` | 检查 Python、optional extras 和示例文件 |

## 退出状态

- `0`：命令成功；
- 非 `0`：输入、依赖、校验、求解或导出失败。

命令默认使用 seeded fallback solver。固定尝试预算完整执行时，相同输入和 seed
会得到稳定结果；若 `metrics.stopped_by_time_limit` 为 `true`，墙钟截止时间可能使
不同机器完成不同数量的尝试，此时不承诺输出完全相同。`solve` 和 `project-solve` 支持
`--backend auto|fallback|ortools|native`：

- `auto`：保持兼容行为；默认使用 fallback，若设置旧环境变量
  `SEATTRELLIS_USE_ORTOOLS=1` 或 `SEATTRELLIS_BACKEND=ortools` 则使用 OR-Tools；
- `fallback`：显式使用内置启发式求解器；
- `ortools`：显式使用 OR-Tools，不需要再设置旧环境变量。
- `native`：仅供源码试验的 Rust 校验器。求解仍由 Python fallback 完成，扩展
  不会随主包或任何 extra 安装；只有在同版本源码树中自行构建
  `seattrellis_native` 后才应显式选择。

`doctor` 会显示当前 backend 默认解析结果。OR-Tools 超时或返回 `UNKNOWN`
时会提示“未在时间限制内找到方案”，不会再误报为确认无解。

## 性能基准

发布前可运行固定虚构数据集基准：

```bash
python scripts/benchmark_solver.py \
  --sizes 40,50,60 \
  --backends fallback \
  --constraint-profiles light,dense \
  --candidate-counts 1,5,20 \
  --time-limit 0.25 \
  --max-attempts 24 \
  --output outputs/benchmark-solver.json \
  --markdown-output outputs/benchmark-solver.md
```

OR-Tools 基准使用相同参数，但设置 `--backends ortools --time-limit 5`。

详见[性能基准](benchmarks.md)。

## 导出模板与隐私

`export` 支持 `public`、`teacher` 和 `report` 模板，以及细粒度隐藏参数：

```bash
seattrellis export \
  --snapshot outputs/candidates.json \
  --candidate recommended \
  --format print-html \
  --template teacher \
  --hide-score \
  --hide-notes \
  --hide-special-needs \
  --hide-height \
  --hide-vision \
  --anonymize \
  --orientation landscape \
  --page-scale 0.8 \
  --locale en \
  --output outputs/private-print.html
```

`public` 的安全默认字段不能通过这些参数放宽；CLI 参数只能进一步隐藏信息。
模板和隐私参数适用于 `print-html`、`pdf`、`docx`、`svg` 和 `pptx`；A4 页面
方向、缩放和页边距只适用于 `print-html`、`pdf` 和 `docx`。其他格式收到非默认
配置时会明确报错，避免设置被静默忽略。

完整候选集比较报告可用 `--candidate-scope all` 导出，目前支持 `html` 和
`print-html`：

```bash
seattrellis export \
  --snapshot outputs/candidates.json \
  --candidate-scope all \
  --format html \
  --output outputs/candidate-comparison.html
```

命令示例见[快速开始](quickstart.zh.md)。

## 人工调整

`edit` 对普通 snapshot 或 candidate set 中的某个候选方案执行一组按顺序排列的
人工操作，并写出新的草稿 snapshot。它适合在可视化编辑器完成前验收
“自动生成 → 手动微调 → 重新导出”流程：

```bash
seattrellis edit \
  --snapshot outputs/neighbor-aware.snapshot.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R4C3 \
  --output outputs/neighbor-aware-edited.snapshot.json
```

输入 candidate set 时默认编辑 recommended candidate，也可以显式指定：

```bash
seattrellis edit \
  --snapshot outputs/candidates.json \
  --candidate candidate_02 \
  --operation swap:STU001:STU002 \
  --output outputs/candidate-02-edited.snapshot.json
```

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

多次 `--operation` 会按命令行顺序执行。默认情况下，即使调整后 hard
constraints 不满足，也会写出草稿并在终端列出违反项；加 `--strict` 后，
若 hard constraints 不满足则命令失败且不写出 snapshot。锁定状态目前只用于本次
命令序列；本次操作摘要会记录在 `metadata.manual_edit`，当前锁状态会记录在
`metadata.lock_state`。后续 `repair` 默认继承这份状态。

对于可复用、可审计的调整记录，也可以使用 `--operations-file`。文件可以是操作对象
数组，或包含 `operations` 数组的对象；文件中的操作总会先执行，随后才执行命令行中的
`--operation`：

```json
{
  "operations": [
    {
      "kind": "swap_students",
      "payload": {
        "first_student": "STU001",
        "second_student": "STU002"
      }
    },
    {
      "kind": "lock_seat",
      "payload": {"seat_id": "R4C3"}
    },
    {
      "kind": "batch_move",
      "payload": {
        "moves": [
          {"student_key": "STU003", "seat_id": "R2C2"},
          {"student_key": "STU004", "seat_id": "R2C1"}
        ]
      }
    }
  ]
}
```

`batch_move` 是单条原子操作：学生和目标座位不得重复；目标座位若已占用，其占用者
也必须包含在同一批次中。任一映射未知、锁定或冲突时，整个批次失败，不会留下部分
修改。内联格式适合简单 ID；ID 含逗号或等号时应使用 JSON 操作文件。

```bash
seattrellis edit \
  --snapshot outputs/candidates.json \
  --operations-file adjustments.json \
  --output outputs/edited.snapshot.json
```

Project 工作流可使用 `project-edit` 复用相同语义。未指定 `--snapshot` 时，它会在
project outputs 目录中查找最新 snapshot 或 candidate set；输入 candidate set 时默认
使用 project 的 `default_candidate`。`project-edit` 同样支持 `--operations-file`。

## 锁定后重排与局部修复

`repair` 把编辑草稿交回现有求解器处理。它不会把临时锁写入 rules 文件，而是在本次
求解中转为临时 fixed-seat 约束；输出 snapshot 会在 `metadata.repair` 中记录锁定、
可变学生、预留空座、实际变化学生、历史数量和有效 backend。

```bash
seattrellis repair \
  --snapshot outputs/neighbor-aware-edited.snapshot.json \
  --affected-student STU001 \
  --affected-student STU002 \
  --lock-seat R4C3 \
  --backend fallback \
  --output outputs/neighbor-aware-repaired.snapshot.json
```

提供一个或多个 `--affected-student` 时，程序会自动加入与其存在 hard rule 或当前
座位相邻关系的一阶学生，其他当前已入座学生会固定在草稿位置。因此只有有效范围和
当前未入座学生会被重新安排。未提供时，所有未锁定学生都可以重新排座。
`--lock-student` 保留该学生当前座位；`--lock-seat` 保留当前座位上的学生，
若该座位为空则临时预留为空座。默认会继承 `metadata.lock_state` 中的锁；使用
`--ignore-saved-locks` 可忽略它们。`--history` 与 `--history-dir` 可传入历史方案，
保证公平轮换和近期邻座规则在局部修复时仍生效。

`project-repair` 对 Project 工作流提供相同能力，未提供 `--snapshot` 时会使用最新
project artifact，并自动读取 project 配置的 `history_dir`。若锁定和原有 hard
fixed-seat 规则矛盾，命令会在写出任何文件前失败，而不会静默改变老师的规则。
求解因锁或局部范围不可行时，错误信息会列出有效范围，并给出需要解锁或扩大范围的
下一步操作。

## Schema 工具

公开 JSON Schema 文件位于仓库的 `schemas/` 目录。需要重新生成时运行：

```bash
seattrellis schema export --output-dir schemas
```

当前迁移命令支持对现行版本 ruleset、snapshot、candidate set、plan comparison
report 和 project JSON 做验证与规范化写回。可先只验证，不写盘：

```bash
seattrellis schema migrate \
  --input examples/rules.json \
  --dry-run
```

确认后再规范化写回：

```bash
seattrellis schema migrate \
  --input examples/history/week1.snapshot.json \
  --output outputs/week1.migrated.snapshot.json
```

覆盖已有文件时会先创建 `.bak` 备份；可重复运行时依次使用 `.bak.1`、`.bak.2`
等名称。旧版本迁移会在未来 schema 版本变更时加入；未知版本仍会给出可操作的
迁移提示。
