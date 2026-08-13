# v1 → v2 迁移指南（SeatTrellis v2.0.0）

> 本文档是 M6 §9.1 的交付物之一（v1 → v2 migration
> documentation）。以当前 Rust 实现为准：
> `crates/seattrellis-schema/src/migration.rs`、
> `crates/seattrellis-io/src/migration.rs` 与 Rust CLI `schema-migrate`。

## 1. 先区分两条不同的迁移路径

当前 Rust 有两条用途不同的路径，不能把它们视为同一个
“通用 v1 → v2 转换器”。

1. **typed v1 → v2 迁移**：由 `seattrellis-schema` 执行，把严格解析的
   v1 JSON 转为 `ArtifactEnvelope` schema version 2。当前仅支持
   `student_roster` 和 `classroom_layout`。Rust CLI `schema-migrate`
   调用的就是这条路径。
2. **项目/工件规范化**：由 `seattrellis-io` 执行，识别现有长期 JSON
   工件，补齐或修正当前 v1 线的标准字段，其余字段保留。
   它**不会**把 project/ruleset/snapshot/rotation 包装成 v2
   `ArtifactEnvelope`。工作台的 project migration API 走这条路径。

### 1.1 当前支持矩阵

| 工件 | typed CLI v1 → v2 | IO 规范化 | 当前结果 |
|---|---:|---:|---|
| `student_roster` JSON | 支持 | 不支持 | v2 envelope，`schema_version: 2` |
| `classroom_layout` JSON | 支持 | 不支持 | v2 envelope，`schema_version: 2` |
| `seattrellis_project` | 不支持 | 支持 | 保持 project v1 契约，`schema_version: 1` |
| ruleset | 不支持 | 支持 | 保持 ruleset v1 契约，`schema_version: 1` |
| seating snapshot | 不支持 | 支持 | 保持 snapshot 契约，`schema_version: "1.0"` |
| `candidate_set` | 不支持 | 支持 | 保持当前契约，`schema_version: "0.2.2"` |
| `plan_comparison_report` | 不支持 | 支持 | 保持当前契约，`schema_version: "0.2.2"` |
| `rotation_plan` | 不支持 | 支持 | 镜像 v1 契约，`schema_version: "1.0"` |
| `history_archive` / `editing_operation_log` / `export_preset` | 不支持 | 不支持 | 不得猜测转换，保留原文件 |

`students.csv` 不是 `schema-migrate` 的 JSON 输入。v1/v2 的 roster
CSV 由名单导入路径直接读取，无需先转成 envelope。

## 2. 工作台与 local API：项目/工件规范化

### 2.1 单文件

- **预览**：`POST /api/v1/projects/migration/preview` 只识别工件、
  计算字段变更和 project reference checks，不写盘。
- **默认应用（`in_place=false`）**：在源文件旁创建
  `<stem>.migrated.json`；已占用时依次使用
  `<stem>.migrated-2.json`、`-3` 等。源文件不变，也不为源文件
  创建 backup。新输出写后若无法重新识别，会删除该新文件。
- **原位应用（`in_place=true`）**：先在源文件旁创建
  `<filename>.bak`；已占用时使用 `.bak.1`、`.bak.2` 等。
  随后以临时文件写入、flush 并 rename。写后重新识别失败时，
  使用该 backup 恢复源文件。
- **恢复**：`POST /api/v1/projects/migration/restore` 只接受文件名
  以 `<destination filename>.bak` 开头的 backup。覆盖当前目标前，
  会另存 `<filename>.pre-restore.bak`（或带数字后缀）安全副本。
  当前实现校验 JSON、backup 文件名归属和恢复后的工件种类；
  不校验独立的 backup 指纹。

project 规范化会补齐 `kind: "seattrellis_project"`、
`schema_version: 1`、缺失/空白时的默认 `name`、`outputs_dir` 以及
`default_candidates/default_candidate/default_export_format`。其他工件只补齐
支持矩阵中的当前 `schema_version`。成功识别后，overlay merge
保留未被标准字段覆盖的数据；但 ruleset 的种类识别本身只允许
`schema_version/seed/hard/soft/groups` 顶层字段。

### 2.2 当前工作台边界

工作台客户端虽会在请求中发送可选 `artifact_path`，但当前 Rust
server 的 preview/apply handler 只使用 `project_path`。因此：

- 工作台中的可靠目标是当前已选项目主文件；
- 不要依赖 `artifact_path` 把 preview/apply 切换到另一个工件；
- local API 会把 `project_path` 字段指向的文件传给 IO 规范化器。

project 主文件的 reference checks 覆盖 `students`、`layout`、`rules`、
`history_dir` 和 `outputs_dir`；其他工件不产生这些检查。

`.seattrellis.zip` 的 `project-pack` / `project-restore` 是项目包打包与
恢复，不会调用 typed v1 → v2 schema migration，不应当作格式迁移入口。

## 3. 批量规范化的事务语义

- 请求必须包含 **2–20** 个非空、不重复的路径。
- preview 会先处理全部文件。project reference 缺失或类型错误时
  `ready=false`；还会报告多个 project 共用的引用路径。
- apply 会再次 preview；`ready=false` 时拒绝写入。所有目标先计算、
  再放入**同一个多文件事务**。任一 staging、校验或提交失败，
  整批不留部分更改；不是“每个项目独立提交”。
- `in_place=false` 为每个源文件创建未占用的
  `.migrated*.json` 兄弟文件，源文件不变。
- `in_place=true` 由 transaction 层为已存在目标保留唯一
  `.<filename>.seattrellis-backup-<transaction-id>-<step>.bak` 兄弟 backup。
  但当前 batch response 的每个 `backup_path` 为 `null`，且 migration
  restore endpoint 只接受第 2.1 节的 `.bak[.N]` 命名。因此批量事务
  能在**失败时**整体回滚，但不提供通过当前 migration restore API
  进行的逐文件成功后恢复。需要可见 `backup_path` 时，使用单文件
  原位应用。
- batch journal 位于系统临时目录；下一次 batch 会在新事务前
  尝试恢复遗留事务。

## 4. Rust CLI `schema-migrate`：仅 typed roster/layout

### 4.1 输入形状

CLI 读取 JSON，并要求顶层有 `kind`。v1 payload 应放在 `data`
中，例如 roster：

```json
{
  "kind": "student_roster",
  "schema_version": 1,
  "data": {
    "students": [
      {"student_id": "1", "name": "张三", "tags": [], "needs": []}
    ]
  }
}
```

layout 使用 `kind: "classroom_layout"`，`data` 内放 v1 layout 对象。
v1 reader 使用 `deny_unknown_fields`；无法解释的 v1 字段会直接拒绝，
不会静默丢弃。

### 4.2 命令

```bash
# 只运行 typed migration 并报告结果；不写盘
seattrellis_cli schema-migrate --input v1-roster.json --dry-run

# 写到另一个文件；源文件不变
seattrellis_cli schema-migrate \
  --input v1-roster.json --output roster.v2.json

# 原位替换输入
seattrellis_cli schema-migrate --input v1-roster.json --in-place
```

CLI 语义：

- `--in-place` 与 `--dry-run` 同时使用会拒绝。
- 非 `--in-place`/非 `--dry-run` 必须提供 `--output`。
- CLI 写入使用 journaled atomic write。只有目标已存在时才保留
  transaction-unique sibling backup：`--output` 不会备份另一个
  路径上的源文件；`--in-place` 会因覆盖输入而保留旧内容。
- 输入的整数 `schema_version` 已为 `2` 时，当前 CLI 将文档原样
  写出；这是 pass-through，不是 typed 重新校验。
- `project`、`ruleset`、`snapshot`、`candidate_set`、
  `plan_comparison`、`rotation_plan` 等 v1 kind 会在 typed dispatch
  中明确报“no v1→v2 migration step registered”。
- 这个 CLI 不提供 IO 规范化路径的 batch/restore 子命令。

## 5. 安全使用顺序

1. 保留原项目的独立副本，先运行 preview 或 CLI `--dry-run`。
2. project/ruleset/snapshot/candidate/rotation 使用 IO 规范化路径；
   不要把它们传给 typed CLI 期待生成 v2 envelope。
3. 项目规范化优先使用默认非原位输出，检查 `.migrated*.json`
   后再切换使用的文件。
4. 仅在需要覆盖源文件时使用单文件 `in_place=true`，并保留
   response 中的 `backup_path`。
5. 批量应用为 all-or-nothing；它解决失败时的部分写入，
   不等于每个文件都有可由当前 API 点击恢复的 backup。
6. 迁移/规范化后再运行 project validation 与 privacy scan；两者
   不是 migration apply 的隐式步骤。
7. 遇到不支持的 kind 或未知字段时停止并保留原文件，不手工删字段
   强行通过。

## 6. 版本兼容表

下表只说明版本来源与进入 v2 的路径，不扩大第 1.1 节的种类覆盖。

| v1 版本 | 可直接读取/规范化 | typed CLI 迁移 | v2 阶段 |
|---|---|---|---|
| v1.8.x（M0 基线 `282fd99`） | project/ruleset/snapshot/candidate/comparison/rotation 按现有契约 | roster/layout JSON | v2.0.0-alpha（M5） |
| v1.9.0（最终 v1 tag） | 同上 | 同上 | v2.0.0-beta（M6） |
| v1.9.x（`v1.x-maintenance`） | 同上 | 同上 | v2.0.0-rc / final（M7） |

v1.x 最终 tag 与 `v1.x-maintenance` 分支在 M6 beta.1 建立（见
`SeatTrellis_v2.0.0_开发与发布总计划_修订版.md` §9.1）。只有这些引用已创建、
推送并保护后，v2 主线才能删除 Python oracle。
