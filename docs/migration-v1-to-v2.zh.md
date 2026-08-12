# v1 → v2 迁移指南（SeatTrellis v2.0.0）

> 本文档是 M6 §9.1 的交付物之一（v1 → v2 migration documentation）。
> 适用于从 v1.x（Python）保存的项目/工件迁移到 v2（Rust-only）的
> 用户与升级路径说明。代码依据：`crates/seattrellis-io/src/migration.rs`、
> `crates/seattrellis-schema/src/migration.rs`、CLI `schema-migrate`。

## 1. 什么会自动迁移

v2 迁移是**显式、可预览、可回滚**的：任何写操作前先备份，迁移报告
记录每个文件的变更；失败即回滚，绝不留下半迁移状态（事务层 + 崩溃
恢复，§17.2.4 验收）。

| 工件 | v1 形态 | v2 形态 | 迁移方式 |
|---|---|---|---|
| 项目文件 | `seattrellis.project.json`（schema_version 1） | v2 项目（Project DTO，`.v2.` schema） | 工作台项目迁移（single/batch/bundle）或 CLI `schema-migrate` |
| 学生名单 | `students.csv`（同一 CSV 格式） | 不变（roster CSV 是 v1/v2 共用格式，直接读取） | 无需迁移 |
| 教室布局 | `classroom.json`（layout v1） | v2 `classroom-layout` 工件 | 自动（`migrate_classroom_layout_v1_to_v2`，无损包装） |
| 规则集 | `rules.json`（ruleset v1） | v2 `ruleset` 工件 | 自动（envelope 包装；未知字段拒绝） |
| 排座快照 | `seating-snapshot.schema.json`（v1 契约） | v2 `seating_snapshot` 工件 | `schema-migrate`（kind 显式支持时才迁移） |

## 2. 迁移入口

### 2.1 工作台（推荐）

- **单个项目**：项目页 → 迁移 → 预览 → 应用（`/api/v1/projects/migration/…`）；
  应用前自动备份到项目 outputs 目录，可在迁移后恢复。
- **批量**：迁移页可选择多个项目批量预览/应用；每个项目独立
  journal 事务，单个失败不影响其他项目。
- **bundle**：`project-pack` 打包的 `.seattrellis.zip` 经 `project-restore`
  恢复（双向互操作已验收，§19.30）。

### 2.2 CLI

```bash
# 预览（不写盘）
seattrellis_cli schema-migrate --input v1-project.json --dry-run
# 应用（写盘前自动备份；--in-place 直接覆盖原文件）
seattrellis_cli schema-migrate --input v1-project.json --output v2-project.json
# 项目级迁移由 project-* 命令承担（project-info 会提示待迁移状态）
```

## 3. 迁移语义与安全

- **无损原则**：v1→v2 是"包装 + 字段保留"；v2 envelope 增加
  `kind`/`schema_version`/`extensions` 命名空间，`deny_unknown_fields`
  保证未知内容不被静默丢弃——不支持的种类**显式报错**，绝不猜测
  （`migrate_v1_to_v2` dispatch 显式枚举，migration.rs:283-291）。
- **备份**：每次应用生成 `.seattrellis-backup-<txn>-` 文件；恢复命令
  校验备份指纹后写回，并保留一份 safety copy。
- **崩溃安全**：journal + fsync + 父目录 fsync + 残留事务恢复
  （`recover_leftover_transactions`）；跨平台路径以正斜杠存 journal。
- **隐私**：迁移不改变文件的所有权/权限语义；privacy 扫描
  （`project-privacy`）在迁移后仍可运行。

## 4. 不迁移 / 保持 v1 契约的项

| 项 | 说明 |
|---|---|
| `rotation_plan` | Rust 旋转工件仍镜像 v1 契约（schema_version "0.2.2"，ledger L5/§19.31），无 v2 DTO；schema-export 对其保留 v1 schema |
| `history_archive` / `editing_operation_log` / `export_preset` | 注册表条目，无 typed DTO（M2-03 未覆盖）；CLI schema-export 对它们显式报"无 typed DTO" |
| 历史快照 | `*.snapshot.json` 由 history/pair 报告与 fair-rotation 直接消费，格式不变（仅 key 折叠为 student_id/name，见 §19.30） |

## 5. 版本兼容表

| v1 版本 | 迁移 | v2 版本 |
|---|---|---|
| v1.8.x（M0 基线 282fd99） | 项目/roster/layout/rules/snapshot | v2.0.0-alpha（M5） |
| v1.9.x（M1–M4 中间线） | 同上 | v2.0.0-beta（M6） |
| v1.10.x+（v1.x-maintenance 分支） | 同上（maintenance 分支不再进入 v2 主线） | v2.0.0-rc（M7） |

v1.x 最终 tag 与 `v1.x-maintenance` 分支在 M6 beta.1 建立（见
`SeatTrellis_v2.0.0_开发与发布总计划_修订版.md` §9.1）；在此之前，
主线上的 v1 代码（`src/seattrellis/`）仅作 oracle 差分身份。
