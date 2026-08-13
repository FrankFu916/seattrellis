# 版本策略与兼容性

## SemVer

SeatTrellis 遵循 [Semantic Versioning 2.0.0](https://semver.org/)。

- **主版本号 (MAJOR)**：不向后兼容的 API / 文件格式变更。
- **次版本号 (MINOR)**：向后兼容的新功能。
- **修订号 (PATCH)**：向后兼容的 Bug 修复。

当前主线为 **v2（纯 Rust）**，crate 版本以 `Cargo.toml` 为准。v1（Python）行
冻结在 `1.9.0`（annotated tag `v1.9.0`，维护分支 `v1.x-maintenance`），只作为
遗留包存在，不再开发新功能。从 v2.0 起，公开 CLI、文件格式和 HTTP API 的不兼容
变更必须通过新的 MAJOR 版本发布。

## Schema Version

需要长期保存或交换的产物带有 `schema_version`。v2 产物注册表（`schema-list`）：

| 产物 kind | 版本 | 可迁移 |
|-----------|------|--------|
| `studentroster` | v2 | 否 |
| `classroomlayout` | v2 | 否 |
| `ruleset` | v2 | 否 |
| `seatingsnapshot` | v2 | 否 |
| `candidateset` | v2 | 否 |
| `plancomparison` | v2 | 否 |
| `historyarchive` | v2 | 否 |
| `rotationplan` | v2 | 否 |
| `editingoperationlog` | v2 | 否 |
| `project` | v2 | 否 |
| `projectbundlemanifest` | v2 | 否 |
| `exportpreset` | v2 | 否 |

v1 时代的文件（snapshot `"1.0"`、candidate set `"0.2.2"`、project `1`、ruleset
`1` 等）会被 v2 迁移路径读取并重写到 v2 版本，迁移前自动创建备份。

公开 JSON Schema 文件位于 `schemas/`。重新生成：

```bash
seattrellis_cli schema-export --kind seatingsnapshot --output seating-snapshot.v2.schema.json
```

迁移命令对旧版本文件执行验证与规范化写回：

```bash
seattrellis_cli schema-migrate --input v1-rules.json --dry-run
seattrellis_cli schema-migrate --input v1-rules.json --output v2-rules.json
seattrellis_cli schema-migrate --input v1-rules.json --in-place
```

`--dry-run` 只验证和报告目标版本，不创建文件。覆盖已有目标或使用 `--in-place`
时默认先创建同目录备份（`.bak`、`.bak.1` 等）。

Editor command/state 是短期传输契约，不是长期保存的产物，不由
`schema-migrate` 处理。协议版本当前为 `"1.0"`；客户端必须显式发送
`protocol_version`，不支持的版本会在执行命令前被拒绝。

## 命令行接口 (CLI)

CLI 命令名和参数以 `seattrellis_cli --help` 输出为准。以下承诺保持稳定：

- `seattrellis_cli solve` / `validate` / `export` 命令名不变；
- `--problem`、`--solution`、`--output`、`--seed`、`--time-limit` 等参数名不变；
- 退出码冻结表：`0` 成功、`2` 无效输入、`3` 确认不可行、`4` 超时、`5` 未知、
  `70` 内部错误、`130` 用户取消。

## 弃用策略 (Deprecation Policy)

### 命令行

| 弃用项 | 说明 |
|--------|------|
| v1 CLI 命令（`seattrellis` / `seatplanner` 别名、`init-demo`、`presets list` 等） | 仅存在于 v1 遗留包中；v2 使用 `seattrellis_cli` |

弃用流程：
1. **MINOR 版本 A**：文档标注 `(已弃用)`，运行时输出 warning（stderr）。
2. **MINOR 版本 A+1**：warning 升级为更显眼的提示。
3. **下一个 MAJOR 版本**：移除。

### 文件格式

- 读取旧 schema 版本的能力保留至少一个 MAJOR 版本周期；
- 无法读取时给出明确错误信息，包含迁移建议。

## 兼容性矩阵

| 组件 | 承诺 |
|------|------|
| Rust | MSRV 1.88；CI 验证 Linux、Windows、macOS |
| 操作系统 | 目标范围为 macOS ≥ 13、Windows ≥ 10、Ubuntu ≥ 22.04 |
| 桌面壳 | Tauri 2（`app/src-tauri`），toolchain 锁定 1.88.0 |
| 前端 | React 19 + TypeScript（`clients/web`），Node.js 仅用于构建 |
| 运行时依赖 | 无 Python、Node.js、OR-Tools、Streamlit；全部本机执行 |

## v1 兼容范围（遗留）

- v1 包（`seattrellis==1.9.0`）继续可从 PyPI 安装，由 `v1.x-maintenance` 分支维护；
- v1 文件格式（CSV 名单、layout/rules JSON、snapshot、candidate set、project）在
  v2 中继续可读并自动迁移；
- v2 不承诺 v1 CLI 命令名和 Python API 的兼容性。
