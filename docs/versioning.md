# 版本策略与兼容性

## SemVer

SeatTrellis 遵循 [Semantic Versioning 2.0.0](https://semver.org/)。

- **主版本号 (MAJOR)**：不向后兼容的 API / 文件格式变更。
- **次版本号 (MINOR)**：向后兼容的新功能。
- **修订号 (PATCH)**：向后兼容的 Bug 修复。

当前稳定版本为 `1.0.1`。从 v1.0 起，公开 CLI、文件格式和 service API 的不兼容变更必须通过新的 MAJOR 版本发布。

## Schema Version

从 v0.6.0 开始，所有文件格式引入 `schema_version` 字段：

| 文件类型 | schema_version | 首次引入 |
|----------|---------------|---------|
| `SeatingSnapshot` | `"1.0"` | v0.1.0 |
| `CandidateSet` / `PlanComparisonReport` | `"0.2.2"` | v0.2.2 |
| `SeatTrellisProject` | `1` | v0.2.3 |
| `RuleSet` (JSON) | 无独立版本 | — |

schema_version 变更规则：
- **PATCH 升级**（如 `1.0` → `1.1`）：新增可选字段，旧读取器忽略新字段即可。
- **MINOR 升级**（如 `1.x` → `2.0`）：字段改名或语义变更；提供迁移路径或兼容读取。
- **MAJOR 升级**：格式彻底重写；旧文件可能无法读取。

## 命令行接口 (CLI)

CLI 命令名和参数以 `--help` 输出为准。以下承诺保持稳定：

- `seattrellis solve` / `validate` / `export` 命令名不变
- `--students`、`--layout`、`--rules`、`--preset`、`--output`、`--history-dir` 参数名不变
- exit code 0 = 成功，非 0 = 失败

`service.py` 与 `service_types.py` 中的公开入口是 CLI、Web 和未来桌面端共享的稳定边界。以下划线开头的函数仍属于内部实现。

## 弃用策略 (Deprecation Policy)

### 命令行

| 弃用项 | 引入版本 | 移除计划 | 说明 |
|--------|---------|---------|------|
| `seatplanner` 别名 | v0.1.0 | 不早于 v1.0 | 旧命令名保留 |

弃用流程：
1. **MINOR 版本 A**：文档标注 `(已弃用)`，运行时输出 warning（stderr）。
2. **MINOR 版本 A+1**：warning 升级为 `FutureWarning` 或更显眼的提示。
3. **下一个 MAJOR 版本**：移除。

### Python API

内部函数（`cli.py` 中以 `_` 开头的函数、`solver/` 内部实现）不承诺稳定性。公开函数（`cli.solve()`、`cli.export()` 等）的弃用遵循相同流程。

### 文件格式

- 读取旧 schema_version 的能力保留至少一个 MAJOR 版本周期。
- 无法读取时给出明确错误信息，包含迁移建议。

## 兼容性矩阵

| 组件 | 承诺 |
|------|------|
| Python 版本 | ≥ 3.11（跟随 Python 支持周期） |
| 操作系统 | macOS ≥ 13, Windows ≥ 10, Ubuntu ≥ 22.04 |
| Pydantic | 1.10+ 和 2.x 双轨（v1 兼容模式优先） |
| OR-Tools | 9.8–9.14 |
| Streamlit | 1.30+ |

## v1.0 承诺

达到 v1.0 后：
- CLI 命令名和主要参数永久冻结
- 文件格式 schema_version 永久冻结（后续只增不减）
- 公开 Python API 冻结
- `seatplanner` 别名可能移除（提前 2 个 MINOR 版本通知）
