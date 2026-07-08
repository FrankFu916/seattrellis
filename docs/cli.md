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
| `project-init/info/validate/solve/export` | 管理本地 project 工作流 |
| `schema list/export/migrate` | 查看 JSON Schema、导出 schema 文件、规范化版本化 JSON |
| `doctor` | 检查 Python、optional extras 和示例文件 |

## 退出状态

- `0`：命令成功；
- 非 `0`：输入、依赖、校验、求解或导出失败。

命令默认使用 deterministic fallback solver。`solve` 和 `project-solve` 支持
`--backend auto|fallback|ortools|native`：

- `auto`：保持兼容行为；默认使用 fallback，若设置旧环境变量
  `SEATTRELLIS_USE_ORTOOLS=1` 或 `SEATTRELLIS_BACKEND=ortools` 则使用 OR-Tools；
- `fallback`：显式使用内置启发式求解器；
- `ortools`：显式使用 OR-Tools，不需要再设置旧环境变量。
- `native`：实验 Rust core 后端。当前仍使用 Python fallback 搜索，但要求本地
  Rust 扩展可用，并用 native core 做底层结构校验。

`doctor` 会显示当前 backend 默认解析结果。OR-Tools 超时或返回 `UNKNOWN`
时会提示“未在时间限制内找到方案”，不会再误报为确认无解。

## 性能基准

发布前可运行固定虚构数据集基准：

```bash
python scripts/benchmark_solver.py \
  --sizes 40,50,60 \
  --backends fallback,ortools \
  --candidates 1 \
  --time-limit 10 \
  --output outputs/benchmark-solver.json \
  --markdown-output outputs/benchmark-solver.md
```

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
页面和模板参数当前适用于 `print-html`、`pdf` 和 `docx`。其他格式收到非默认
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
- `seat:STU003:R2C2`
- `unseat:STU004`
- `lock-student:STU001`
- `unlock-student:STU001`
- `lock-seat:R1C1`
- `unlock-seat:R1C1`

多次 `--operation` 会按命令行顺序执行。默认情况下，即使调整后 hard
constraints 不满足，也会写出草稿并在终端列出违反项；加 `--strict` 后，
若 hard constraints 不满足则命令失败且不写出 snapshot。锁定状态目前只用于本次
命令序列；本次操作摘要会记录在 `metadata.manual_edit`，但锁定状态还不是后续
命令自动继承的正式 schema 字段。

## Schema 工具

公开 JSON Schema 文件位于仓库的 `schemas/` 目录。需要重新生成时运行：

```bash
seattrellis schema export --output-dir schemas
```

当前迁移命令支持对现行版本 snapshot、candidate set、plan comparison report 和
project JSON 做验证与规范化写回：

```bash
seattrellis schema migrate \
  --input examples/history/week1.snapshot.json \
  --output outputs/week1.migrated.snapshot.json
```

旧版本迁移会在未来 schema 版本变更时加入；未知版本仍会明确报错。
