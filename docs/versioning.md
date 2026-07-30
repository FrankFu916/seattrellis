# 版本策略与兼容性

## SemVer

SeatTrellis 遵循 [Semantic Versioning 2.0.0](https://semver.org/)。

- **主版本号 (MAJOR)**：不向后兼容的 API / 文件格式变更。
- **次版本号 (MINOR)**：向后兼容的新功能。
- **修订号 (PATCH)**：向后兼容的 Bug 修复。

当前稳定版本为 `1.4.0`，包版本以 `pyproject.toml` 为准。从 v1.0 起，公开
CLI、文件格式和 service API 的不兼容变更必须通过新的 MAJOR 版本发布。

## Schema Version

需要长期保存或交换的产物带有 `schema_version`：

| 文件类型 | schema_version | 首次引入 |
|----------|---------------|---------|
| `SeatingSnapshot` | `"1.0"` | v0.1.0 |
| `CandidateSet` / `PlanComparisonReport` | `"0.2.2"` | v0.2.2 |
| `SeatTrellisProject` | `1` | v0.2.3 |
| `EditorCommandEnvelope` / `EditorStateEnvelope` | `protocol_version: "1.0"` | v1.4.0 |
| `RuleSet` (JSON) | `1` | v1.4.0 |

当前读取器只接受表中的版本。新增可选字段时可以保留版本号；字段改名、类型
变化或语义变化需要新的 schema 版本，并应同时提供迁移说明。

公开 JSON Schema 文件位于 `schemas/`。重新生成：

```bash
seattrellis schema export --output-dir schemas
```

文档构建会把这些文件复制到站点的 `/schemas/` 路径，使每份 Schema 的 `$id`
可以直接访问。

当前 `schema migrate` 命令对现行版本执行验证与规范化写回，为未来旧版本迁移保留
稳定入口：

```bash
seattrellis schema migrate --input input.json --dry-run
seattrellis schema migrate --input input.json --output output.json
seattrellis schema migrate --input input.json --in-place
```

`--dry-run` 只验证和报告目标版本，不创建文件。覆盖已有目标或使用 `--in-place`
时默认先创建同目录备份（`.bak`、`.bak.1` 等）；只有调用方已经另行备份时才使用
`--no-backup`。旧版未带 `schema_version` 的 RuleSet 继续按版本 1 读取，重新导出或
迁移后会显式写入版本。

Editor command/state 是短期传输契约，不是长期保存的 snapshot 或 project 产物，
因此不由 `schema migrate` 处理。客户端必须显式发送 `protocol_version`；不支持的
版本会在执行命令前被拒绝。

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
| `seatplanner` 别名 | v0.1.0 | 下一个 MAJOR 版本前不会移除 | 旧命令名保留 |

弃用流程：
1. **MINOR 版本 A**：文档标注 `(已弃用)`，运行时输出 warning（stderr）。
2. **MINOR 版本 A+1**：warning 升级为 `FutureWarning` 或更显眼的提示。
3. **下一个 MAJOR 版本**：移除。

### Python API

以下划线开头的函数和 `solver/` 内部实现不承诺稳定性。`service.py` 中的公开
函数如果需要弃用，会先在文档和运行时提示中说明替代接口。

### 文件格式

- 读取旧 schema_version 的能力保留至少一个 MAJOR 版本周期。
- 无法读取时给出明确错误信息，包含迁移建议。

## 兼容性矩阵

| 组件 | 承诺 |
|------|------|
| Python 版本 | 3.11–3.14（v1.x 最低版本保持 3.11） |
| 操作系统 | 目标范围为 macOS ≥ 13、Windows ≥ 10、Ubuntu ≥ 22.04；CI 验证 GitHub 当前 runner |
| Pydantic | 1.10.26+ 和 2.x 双轨，统一使用 v1 兼容 API |
| Typer | 0.26.7–0.27.x |
| OR-Tools | 9.15.x |
| Streamlit | 1.50+ 兼容；浏览器主验收使用 1.60 |
| Pillow | 11.3–12.x |
| WeasyPrint | 69.x |
| Rust 原生扩展 | Rust ≥ 1.83；Python 3.11–3.14 |

## v1.x 兼容范围

- CLI 命令名和主要参数保持兼容；
- 现有 schema 在 v1.x 期间继续可读；
- 公开 service 函数不做无提示的破坏性改动；
- 删除兼容别名前至少提前两个 MINOR 版本通知。
