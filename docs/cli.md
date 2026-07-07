# CLI 命令参考

运行 `seattrellis --help` 查看当前安装版本的完整参数。

| 命令 | 用途 |
|------|------|
| `init-demo` | 创建虚构示例文件 |
| `presets list/show/export` | 查看或导出内置规则 preset |
| `validate` | 校验输入、字段和明显规则冲突 |
| `solve` | 生成 snapshot 或 candidate set |
| `export` | 导出选定方案 |
| `history-report` | 汇总学生座位类别历史 |
| `pair-report` | 汇总同桌和邻座历史 |
| `project-init/info/validate/solve/export` | 管理本地 project 工作流 |
| `doctor` | 检查 Python、optional extras 和示例文件 |

## 退出状态

- `0`：命令成功；
- 非 `0`：输入、依赖、校验、求解或导出失败。

命令默认使用 deterministic fallback solver。`solve` 和 `project-solve` 支持
`--backend auto|fallback|ortools`：

- `auto`：保持兼容行为；默认使用 fallback，若设置旧环境变量
  `SEATTRELLIS_USE_ORTOOLS=1` 或 `SEATTRELLIS_BACKEND=ortools` 则使用 OR-Tools；
- `fallback`：显式使用内置启发式求解器；
- `ortools`：显式使用 OR-Tools，不需要再设置旧环境变量。

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
  --output outputs/benchmark-solver.json
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

命令示例见[快速开始](quickstart.zh.md)。
