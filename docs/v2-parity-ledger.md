# SeatTrellis v2 Parity Ledger

> **文档目的**：M0（v1.8.5）产出的不可争议迁移基线。逐项记录 Python v1.x（oracle）与 Rust v2 之间的功能对应关系与状态，供后续所有迁移决策引用。
>
> - 基线源码（分析基准）：`282fd99a7e766aaedaea6c5bb4c61e3ef14d257c`（与《SeatTrellis_v2.0.0_开发与发布总计划_修订版》第 5 行一致）
> - 基线版本：`seattrellis 1.8.4`（`pyproject.toml:7`、`src/seattrellis/__init__.py:3`）
> - 建立日期：2026-08-08
> - 状态判定依据：源码阅读（本次盘点）+ 现有单元测试；**尚未**执行 golden fixture 差分（3.2 之后才可升级为 `RUST_VERIFIED`）
> - 关联计划：M0 阶段（计划 §3），《SeatTrellis_v2.0.0_开发与发布总计划_修订版.md》3.1
> - 2026-08-08 M0 收口：`fixtures/parity/` 41 个 case 的 inputs+goldens 全部生成并纳入 MANIFEST（含逐文件 SHA-256）；`scripts/gen_parity_fixtures.py verify` 可离线重放（CI `parity-oracle` job）；差分 harness（`scripts/rust_python_diff.py`）升级为七状态语义（M0-03），mismatch 非零退出。

---

## 0. 状态字典（本账本唯一允许值）

| 状态 | 判定标准 |
|---|---|
| `PYTHON_ONLY` | 仅 Python v1.x 可实现/可用；Rust 侧无等价实现，或实现形态完全不同（不可直接对照）。 |
| `RUST_PARTIAL` | Rust 已有实现，但存在**已知**覆盖缺口（缺字段/缺端点/缺选项/近似语义/保真缺口）。 |
| `RUST_PARITY_PENDING` | Rust 实现存在且按代码比对语义对齐，**尚未**通过 golden fixture 差分验证。 |
| `RUST_VERIFIED` | 已通过 golden fixture 差分（3.2 产出）或等价自动化对照验证。**本账本建立时暂无此项**（M0 阶段所有条目最高为 `RUST_PARITY_PENDING`）。 |
| `INTENTIONALLY_REMOVED_V2` | v2 有意移除。必须附书面理由、迁移方案、用户影响说明。当前无条目采用。 |

维护约定：

1. 任何条目状态变更必须同时更新本文件并注明 commit 与日期。
2. `RUST_PARITY_PENDING` → `RUST_VERIFIED` 的唯一合法路径：对应 `fixtures/parity/` golden 用例全部通过（或按计划 3.2 注释，启发式解只要求语义/合法性/评分定义/质量门槛一致，不要求座位表逐位相同）。
3. 计划 §13（v2.0 发布门禁）要求 parity ledger 全部 v2 必须项 `RUST_VERIFIED`（计划 1366 行）。
4. 引入 `INTENTIONALLY_REMOVED_V2` 需在 §11 登记。

> 词表对照（计划 §五 使用的简化词表与本账本的映射）：计划书
> `PYTHON_ORACLE` ≈ 本账本 `PYTHON_ONLY`（仅 Python 可实现）；计划书
> `RUST_PARTIAL` 覆盖本账本 `RUST_PARTIAL` + `RUST_PARITY_PENDING`（已实现但
> 未经验证对等）；计划书 `RUST_MISSING` ≈ 本账本 `PYTHON_ONLY`（无 Rust 实现）；
> 计划书 `REMOVAL_APPROVED` ≈ 本账本 `INTENTIONALLY_REMOVED_V2`。本账本词表
> 是唯一操作值；计划书词表仅作宏观视图，不一致处以本账本为准。

---

## 1. Python public CLI 命令

Python 侧入口（`src/seattrellis/cli.py`，Typer）：24 个顶层命令 + `presets`（3）+ `schema`（3）子命令组，共 30 个可执行命令；另有独立 `seattrellis-desktop`（argparse）。console_scripts（`pyproject.toml:64-67`）：`seattrellis`、`seatplanner`（同一 `cli.main`）、`seattrellis-desktop`。

Rust 侧：`native/seattrellis_cli`（手写参数解析，无 clap）目前仅 3 个子命令：`validate --problem <file>`、`solve --problem <file> [--seed] [--output]`、`export --problem --solution --format <svg|html|png|pdf> --output`（`native/seattrellis_cli/src/main.rs:233-235`）。参数模型为 CoreSolveRequest JSON，与 Python 高层文件输入完全不同。

### 1.1 顶层命令（24）

| 命令 | Python 位置 | 参数要点 | Rust 现状 | 状态 |
|---|---|---|---|---|
| `doctor` | cli.py:191-193 → `run_doctor()` service.py:861 | 无 | 无对应 | `PYTHON_ONLY` |
| `workspace` | cli.py:195-230 → `workspace_server.run_workspace_server` | `--host/--port/--open-browser`（长驻进程） | v2 由 Rust app server 替代（见 §14） | `RUST_PARTIAL` |
| `desktop` | cli.py:232-246 → `desktop.run_desktop_app` | `--width/--height`（pywebview） | v2 由 Tauri 壳替代（见 §14） | `RUST_PARTIAL` |
| `init-demo` | cli.py:248-253 → `init_demo` service.py:1023 | `--output-dir/--force` | 无对应 | `PYTHON_ONLY` |
| `solve` | cli.py:255-300 → `solve_with_report` service.py:569 | `--students/--layout/--rules/--preset/--history(-dir)/--time-limit/--backend/--candidates(1-20)/--seed/--report` | Rust CLI `solve` 存在但输入为 CoreSolveRequest JSON、无候选集/报告 | `RUST_PARTIAL` |
| `rotation-plan` | cli.py:302-339 → `generate_rotation_plan` service.py:647 | `--periods(1-20)/--label/--name/...` | 无对应（Rust rotation 仅保存/加载已生成 plan） | `PYTHON_ONLY` |
| `validate` | cli.py:341-367 → `run_validate` service.py:944 | `--strict` | Rust CLI `validate` 存在（core `evaluate_problem` 已验证），但无 preset/history 警告语义 | `RUST_PARTIAL` |
| `export` | cli.py:369-449 → `export` service.py:696 | 8 格式、template、6 隐私开关、page/locale | Rust CLI `export` 仅 svg/html/png/pdf | `RUST_PARTIAL` |
| `edit` | cli.py:451-505 → `edit_snapshot` service.py:764 | 9 种操作 kind、`--operations-file/--strict` | Rust CLI 无；app server 有 editing 协议端点（§11） | `PYTHON_ONLY` |
| `repair` | cli.py:507-591 → `repair_snapshot` service.py:807 | `--affected-student/--lock-student/--lock-seat/--ignore-saved-locks/...` | 无对应 | `PYTHON_ONLY` |
| `history-report` | cli.py:593-611 → `run_history_report` service.py:969 | `--history(-dir)` | 无对应（app 仅目录浏览，无统计报告） | `PYTHON_ONLY` |
| `pair-report` | cli.py:613-635 → `run_pair_report` service.py:990 | `--top/--within-distance` | 无对应 | `PYTHON_ONLY` |
| `project-init` | cli.py:637-658 → `project_init` service.py:1029 | 默认项目文件 `seattrellis.project.json` | 无对应 CLI；app projects.rs 可读 | `PYTHON_ONLY` |
| `project-list` | cli.py:660-673 → `project_bundle.list_recent_projects` | `--root/--limit` | app `GET /projects/recent`（server.rs:470） | `RUST_PARITY_PENDING` |
| `project-privacy` | cli.py:675-684 → `project_bundle.scan_project_privacy` | `--include-outputs` | app `POST /projects/privacy`（server.rs:476） | `RUST_PARITY_PENDING` |
| `project-pack` | cli.py:686-699 → `project_bundle.pack_project` | 输出 `.seattrellis.zip` | app `POST /projects/bundle`（server.rs:479，格式 v1 双向对齐） | `RUST_PARITY_PENDING` |
| `project-restore` | cli.py:701-711 → `project_bundle.restore_project_bundle` | `--bundle/--output-dir/--force` | app `POST /projects/restore`（server.rs:482） | `RUST_PARITY_PENDING` |
| `project-info` | cli.py:713-721 → `project_info` service.py:1053 | 无 | 无对应 | `PYTHON_ONLY` |
| `project-validate` | cli.py:723-732 → `project_validate` service.py:1062 | `--strict` | 无对应 | `PYTHON_ONLY` |
| `project-solve` | cli.py:734-764 → `project_solve` service.py:1080 | `--candidates/--seed/--report` | 无对应 CLI（app `classes/generate` 部分覆盖，见 §2） | `PYTHON_ONLY` |
| `project-rotate` | cli.py:766-788 → `project_rotate` service.py:1118 | `--periods/--label` | 无对应（旋转生成 Python-only） | `PYTHON_ONLY` |
| `project-edit` | cli.py:790-842 → `project_edit` service.py:1154 | `--snapshot/--operation/...` | 无对应 | `PYTHON_ONLY` |
| `project-repair` | cli.py:844-922 → `project_repair` service.py:1181 | `--affected-student/...` | 无对应 | `PYTHON_ONLY` |
| `project-export` | cli.py:924-945 → `project_export` service.py:1225 | `--format/--candidate` | 无对应 CLI（app `POST /exports` 部分覆盖，见 §12） | `PYTHON_ONLY` |

### 1.2 子命令组

| 命令 | Python 位置 | 参数要点 | Rust 现状 | 状态 |
|---|---|---|---|---|
| `presets list` | cli.py:128-130 | 无 | 无对应（app goal_rules.rs 仅 4 个 goal，非 preset 全集） | `PYTHON_ONLY` |
| `presets show <preset>` | cli.py:132-136 | 位置参数 | 无对应 | `PYTHON_ONLY` |
| `presets export <preset>` | cli.py:138-145 | `--output` | 无对应 | `PYTHON_ONLY` |
| `schema list` | cli.py:147-149 | 无 | 无对应 | `PYTHON_ONLY` |
| `schema export` | cli.py:151-165 | `--output-dir` | 无对应（Rust 无 JSON Schema 生成器） | `PYTHON_ONLY` |
| `schema migrate` | cli.py:167-189 → `schema_migration.migrate_json_file` | `--input/--output/--in-place/--dry-run/--backup` | app migration.rs 等价逻辑存在（§13），但无 Rust CLI 入口 | `RUST_PARTIAL` |

### 1.3 `seattrellis-desktop`（独立 argparse）

`src/seattrellis/desktop_app.py:12-60`：`--width(1280)/--height(900)/--title/--version`，exit code 0 成功 / 2 解析错误。v2 由 Tauri 壳替代 → `RUST_PARTIAL`（见 §14）。

### 1.4 错误/退出码契约（Python）

| 行为 | 约定 | 出处 |
|---|---|---|
| 业务错误 | stderr 打印 `Error: ...`，`typer.Exit(1)` | cli.py:993-998 |
| `--version/-V` | 打印 `seattrellis {__version__}`，退出 0 | cli.py:109-124 |
| 无参数 | 显示帮助（退出 0） | cli.py:95 |
| 参数解析错误 | 退出 2（Click/Typer 惯例） | — |
| typer 未安装 | `SystemExit` 带提示 | cli.py:984-990 |

Rust CLI 需在 v2 对照此契约（当前 `main.rs` 手写解析，退出码未统一校验——`RUST_PARTIAL` 备注项）。

### 1.5 验收参照

`scripts/smoke_cli.py`（`_commands()` 于 :123-581）是 CLI 面最完整的验收清单：覆盖 `--help`、init-demo、doctor、presets、schema、validate、solve、project-*、edit、repair、history/pair-report、export 各格式。任何 Rust CLI 迁移以 `python -m seattrellis.cli` 对照逐条比对；该脚本支持 `--command` 替换被测可执行文件（smoke_cli.py:36-40）。

---

## 2. service / application 公开用例

Python 服务层（`src/seattrellis/service.py` + `src/seattrellis/application/`）按领域分组。Rust 侧实现位于 `app/src/`（loopback HTTP server）与 `native/seattrellis_core`。

### 2.1 solve（求解）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_solve`（内存模型求解，候选 1–20 + recommended） | service.py:107；service_types.py:181/:199 | app `POST /classes/generate`（server.rs:430，room_templates.rs:317/331 + goal_rules.rs:32 + editing.rs:868）；core `solve_problem`（lib.rs:530） | `RUST_PARTIAL`（候选集多解生成、recommended 选择、计划比较报告未确认对等） |
| `solve` / `solve_with_report`（文件级 + 报告） | service.py:537/:569 | Rust CLI `solve --problem` 存在，形态不同；无文件级工作流 | `RUST_PARTIAL` |
| `project_solve`（项目级） | service.py:1080 | 无对应 | `PYTHON_ONLY` |
| `generate_class_plan`（class 工作流入口） | application/class_workflow.py:105 | app `classes/generate` 已接线 | `RUST_PARITY_PENDING` |
| `SolveInput`/`SolveOutput` | service_types.py:181/:199 | CoreSolveRequest/Response（lib.rs:398/:436）为求解子集 | `RUST_PARTIAL` |

### 2.2 rotation（轮换生成）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_rotation_plan`（逐期顺序求解） | service.py:178-285 | **无生成器**；app rotation.rs 仅保存/读取/呈现（rotation.rs:290-410） | `PYTHON_ONLY` |
| `format_rotation_summary` | service.py:273 | 无对应 | `PYTHON_ONLY` |
| `generate_rotation_plan`（文件级） | service.py:647 | 无对应 | `PYTHON_ONLY` |
| `project_rotate` | service.py:1118 | 无对应 | `PYTHON_ONLY` |
| rotation 保存/加载/group-register | —（handlers.py:622/:693/:748/:781） | app rotation.rs:290-410 + server.rs:503-517 全接线 | `RUST_PARITY_PENDING` |

### 2.3 rules / validate / teacher goals

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_validate` / `ValidateInput` | service.py:287；service_types.py:248 | core `evaluate_problem`（lib.rs:107）等价；hard rule 判定 Rust 已镜像（§7） | `RUST_PARITY_PENDING` |
| `run_validate`（文件级，含 preset/history 警告） | service.py:944 | Rust CLI `validate` 无 preset/history 警告语义 | `RUST_PARTIAL` |
| `project_validate` | service.py:1062 | 无对应 | `PYTHON_ONLY` |
| `list_teacher_goals` / `get_teacher_goal` / `resolve_teacher_goal` | application/teacher_goals.py:98/:104/:117 | app goal_rules.rs 仅 4 个 goal（`GOAL_IDS` goal_rules.rs:14-19），Python 15 个 preset 中 11 个无对应；goal JSON 不含 hard/groups | `RUST_PARTIAL` |

### 2.4 candidates（候选集）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| 候选集生成（1–20、seed、recommended） | service.py:107-175；api/models.py:185 | core 单解 `solve_problem`；多候选/recommended/多样性未确认 | `RUST_PARTIAL` |
| `CandidateSummary` / `GenerateClassResponse` | api/handlers.py:1909-1927；models.py:244 | app `classes/generate` 响应 | `RUST_PARITY_PENDING` |
| `EditorDraftStore.create(candidate_set)`（候选→草稿唯一入口） | api/drafts.py:74 | app editing.rs `EditorDraftStore`（editing.rs:831） | `RUST_PARITY_PENDING` |
| PlanScore / breakdown / diversity / stability | scoring.py:280-636 | **无对应**（Rust core 只算成本总和 lib.rs:753-818） | `PYTHON_ONLY` |

### 2.5 editing / repair（见 §11 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_edit` / `edit_snapshot` / `project_edit` | service.py:294/:764/:1154 | app editing 协议端点存在（editing.rs:856/:884），文件级无 | `RUST_PARTIAL` |
| `compute_repair` / `repair_snapshot` / `project_repair` | service.py:336/:807/:1181 | **无对应**（受约束重解） | `PYTHON_ONLY` |
| `EditorDraftStore` / `LayoutDraftStore` / `RosterDraftStore` | api/drafts.py:45、api/layouts.py:34、api/rosters.py:48 | app editing.rs/layouts.rs/roster.rs 对应 store | `RUST_PARITY_PENDING` |

### 2.6 history / pair history（见 §9 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_history_report` / `run_history_report` | service.py:473/:969 | 无对应（app 仅文件目录浏览 projects.rs:451-483） | `PYTHON_ONLY` |
| `compute_pair_report` / `run_pair_report` | service.py:480/:990 | 无对应 | `PYTHON_ONLY` |
| `compute_project_info` / `project_info` | service.py:499/:1053 | 无对应 | `PYTHON_ONLY` |

### 2.7 project（项目工作区）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `init_demo` / `project_init` | service.py:1023/:1029 | 无对应 | `PYTHON_ONLY` |
| `project_info` / `project_validate` | service.py:1053/:1062 | 无对应 | `PYTHON_ONLY` |
| `run_doctor`（环境诊断） | service.py:861 | 无对应 | `PYTHON_ONLY` |
| project 历史/产物浏览 | handlers.py:228/:259 | app projects.rs:332/:481 + server.rs:470-475 | `RUST_PARITY_PENDING` |
| 产物对比（artifacts/compare） | handlers.py:298 | **无对应端点**（前端依赖，Rust 404） | `PYTHON_ONLY` |
| 产物恢复（artifacts/restore） | handlers.py:380 | **无对应端点** | `PYTHON_ONLY` |
| 隐私扫描 / 打包 / 恢复 | handlers.py:1432/:1474/:1496 | app projects.rs:706/:947/:1013 + server.rs:476-484 | `RUST_PARITY_PENDING` |

### 2.8 migration（见 §13 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `project_migration_preview/apply/batch_preview/batch_apply/restore` | api/handlers.py:413/:421/:429/:477/:576 | app migration.rs:842-1017 + server.rs:485-502 全接线 | `RUST_PARITY_PENDING` |
| `_migrate_project_artifact`（核心，含回滚） | api/handlers.py:1113 | migration.rs:770-835（含 rollback） | `RUST_PARITY_PENDING` |

### 2.9 privacy（导出隐私，见 §12）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `_resolve_export_privacy`（public/teacher/report 默认值） | api/handlers.py:2407；service_types.py:43-78 | export.rs:158-173 仅 `anonymize` 生效，其余 5 位契约兼容无效 | `RUST_PARTIAL` |

### 2.10 roster（见 §5 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `import_roster` / `import_roster_records` / `summarize_roster` | application/roster_import.py:39/:51/:61 | app roster.rs:976 上传端点 | `RUST_PARITY_PENDING` |
| `suggest_roster_mapping`（含表头/身份列启发式） | application/roster_mapping.py:238 | 未知是否镜像启发式 | `RUST_PARTIAL` |
| `preview_roster_update` / `apply_roster_update`（指纹+冲突+版本） | application/roster_update.py:133/:366 | app roster.rs:1000 预览端点 | `RUST_PARITY_PENDING` |

### 2.11 layout（见 §6 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `list_room_templates` / `get_room_template` / `recommend_room_template` | application/room_templates.py:57/:63/:85 | app room_templates.rs:317/:331 | `RUST_PARITY_PENDING` |
| `build_standard_room` / `build_room_from_template` | application/room_templates.py:100/:136 | app room_templates.rs 对应 | `RUST_PARITY_PENDING` |

---

## 3. React 当前调用的全部 `/api/v1/*`

前端全部请求集中在 `clients/web/src/api/client.ts`（`API_ROOT = "/api/v1"`，client.ts:35）。共 **31 个调用**；Rust server（`app/src/server.rs` route() :427-524）已实现 **28 个**，**3 个缺失**。

### 3.1 已由 Rust server 提供（28 个 → `RUST_PARITY_PENDING`）

| # | Method + Path | 前端定义 | 前端调用点 | Rust 分发行 |
|---|---|---|---|---|
| 1 | GET `/api/v1/health` | client.ts:111 | App.tsx:322 | server.rs:428 |
| 2 | GET `/api/v1/catalogs` | client.ts:112 | App.tsx:322 | server.rs:429 |
| 3 | GET `/api/v1/projects/recent` | client.ts:132 | ProjectWorkspacePanel.tsx:332 | server.rs:470 |
| 4 | POST `/api/v1/projects/history` | client.ts:139 | ProjectWorkspacePanel.tsx:315 | server.rs:473 |
| 5 | POST `/api/v1/projects/migration/preview` | client.ts:184 | ProjectWorkspacePanel.tsx:452 | server.rs:485 |
| 6 | POST `/api/v1/projects/migration/apply` | client.ts:200 | ProjectWorkspacePanel.tsx:473 | server.rs:488 |
| 7 | POST `/api/v1/projects/migration/batch/preview` | client.ts:215 | ProjectWorkspacePanel.tsx:519 | server.rs:494 |
| 8 | POST `/api/v1/projects/migration/batch/apply` | client.ts:226 | ProjectWorkspacePanel.tsx:539 | server.rs:497 |
| 9 | POST `/api/v1/projects/migration/restore` | client.ts:238-239 | ProjectWorkspacePanel.tsx:498 | server.rs:500 |
| 10 | POST `/api/v1/projects/rotation/save` | client.ts:259-260 | ProjectWorkspacePanel.tsx:564 | server.rs:503 |
| 11 | POST `/api/v1/projects/rotation/load` | client.ts:279-280 | ProjectWorkspacePanel.tsx:585 | server.rs:506 |
| 12 | POST `/api/v1/projects/rotation/group-register` | client.ts:304 | ProjectWorkspacePanel.tsx:602 | server.rs:509 |
| 13 | POST `/api/v1/projects/rotation/group-register/preview` | client.ts:331 | ProjectWorkspacePanel.tsx:625 | server.rs:512 |
| 14 | POST `/api/v1/projects/privacy` | client.ts:348 | ProjectWorkspacePanel.tsx:360 | server.rs:476 |
| 15 | POST `/api/v1/projects/bundle` | client.ts:367 | ProjectWorkspacePanel.tsx:375 | server.rs:479 |
| 16 | POST `/api/v1/projects/restore` | client.ts:396 | ProjectWorkspacePanel.tsx:392 | server.rs:482 |
| 17 | POST `/api/v1/rosters/drafts` | client.ts:408 | RosterImportPanel.tsx:153 | server.rs:434 |
| 18 | GET `/api/v1/rosters/drafts/{id}` | client.ts:444 | （定义未直接调用） | server.rs:437 |
| 19 | POST `/api/v1/rosters/drafts/{id}/preview` | client.ts:452 | RosterImportPanel.tsx:235 | server.rs:440 |
| 20 | DELETE `/api/v1/rosters/drafts/{id}` | client.ts:463 | （定义未直接调用） | server.rs:443 |
| 21 | POST `/api/v1/layouts/drafts` | client.ts:469 | LayoutEditorPanel.tsx:117 | server.rs:455 |
| 22 | POST `/api/v1/layouts/drafts/{id}/commands` | client.ts:481 | LayoutEditorPanel.tsx:148 | server.rs:461 |
| 23 | GET `/api/v1/layouts/drafts/{id}/compiled` | client.ts:494 | LayoutEditorPanel.tsx:172 | server.rs:464 |
| 24 | DELETE `/api/v1/layouts/drafts/{id}` | client.ts:499 | LayoutEditorPanel.tsx:191 | server.rs:467 |
| 25 | POST `/api/v1/classes/generate` | client.ts:508 | App.tsx:745 | server.rs:430 |
| 26 | GET `/api/v1/editing/drafts/{id}` | client.ts:533 | App.tsx:694/:751 | server.rs:446 |
| 27 | POST `/api/v1/editing/drafts/{id}/commands` | client.ts:540 | App.tsx:606 | server.rs:449 |
| 28 | POST `/api/v1/exports` | client.ts:558 | App.tsx:789 | server.rs:452 |

### 3.2 Rust 缺失 → Python-only（3 个 → `PYTHON_ONLY`）

| # | Method + Path | 前端定义 | 前端调用点 | Python 路由 | 影响 |
|---|---|---|---|---|---|
| 29 | POST `/api/v1/classes/rotation` | client.ts:522 | App.tsx:739（轮转设置启用时的**核心路径**） | http.py:585 | Rust 桌面后端下轮转生成直接 404，**适配层最显著缺口** |
| 30 | POST `/api/v1/projects/artifacts/compare` | client.ts:154 | ProjectWorkspacePanel.tsx:418 | http.py:609 | 项目产物对比不可用 |
| 31 | POST `/api/v1/projects/artifacts/restore` | client.ts:169 | ProjectWorkspacePanel.tsx:435 | http.py:617 | 项目产物恢复不可用 |

### 3.3 两侧各自独有（非 parity 缺口，仅记录）

- Rust 独有：`POST /api/v1/solve`（server.rs:431 别名）、`POST /projects/migration/reference-checks`（server.rs:491）、`POST /projects/rotation/group-register/save`（server.rs:515）、`GET /layouts/drafts/{id}`（server.rs:458）。
- Python 独有且前端未调用：`GET /capabilities`、`/room-templates`、`/teacher-goals`、`POST /classes/inspect`、`DELETE /editing/drafts/{id}`（http.py:537-557/:565/:741）。

### 3.4 架构前提

Rust server（`app`，main.rs 绑定 127.0.0.1）与 Python `workspace_server`（默认 127.0.0.1:8765，workspace_server.py:29）是**并列的两个同源后端**；前端用相对路径 `/api/v1` 打向同源服务器，vite dev 代理把 `/api` 指向 8765（vite.config.ts:15-20）。Rust **不转发**到 Python（无任何 proxy 逻辑）。

---

## 4. Schema（Project / RuleSet / Snapshot / CandidateSet / Rotation / Editor protocol）

### 4.1 JSON Schema 文件（`schemas/`，10 个）

| Schema | 顶层结构 / 版本 | Python 来源 | Rust 现状 | 状态 |
|---|---|---|---|---|
| `project.schema.json`（82 行） | `schema_version: int = 1`；`kind=seattrellis_project`；students/layout/rules/history_dir/outputs_dir/default_candidates/… | `models/project.py:15` | projects.rs `ProjectFile` 为**部分字段**结构体（load_project 用 `serde_json::Value` 校验 kind/schema_version，projects.rs:201-233） | `RUST_PARTIAL` |
| `ruleset.schema.json`（470 行） | `schema_version: int = 1`；hard/soft/groups + 15+ 规则 definitions | `models/rules.py`；`RULESET_SCHEMA_VERSION` schema.py:15 | core `models.rs` `RuleSet`/`SoftRules`/各规则 struct 全量镜像（models.rs:253-508） | `RUST_PARITY_PENDING` |
| `seating-snapshot.schema.json`（890 行） | `schema_version: "1.0"`；students/layout/rules/assignments/solver_status/objective_value/metrics | `models/snapshot.py:21` | core `models.rs` 仅**求解所需字段子集**（文件头注释 models.rs:11-16；无 schema_version 字段） | `RUST_PARTIAL` |
| `candidate-set.schema.json`（1138 行） | `schema_version: "0.2.2"`；candidates/recommended_candidate_id/warnings + PlanScore | `models/candidate.py:115` | 无 typed 结构（migration.rs 用 `Value` 泛化处理） | `PYTHON_ONLY` |
| `rotation-plan.schema.json`（983 行） | `schema_version: "1.0"`；periods/name/fairness_summary/pair_repeat_summary | `models/rotation.py:36` | rotation.rs `RotationPlanData`（:186-236）serde **只取所需字段**（读取兼容，无完整写入/校验） | `RUST_PARTIAL` |
| `editor-command.schema.json`（436 行） | `protocol_version: "1.0"`；command_id/draft_id/base_revision/action/operations（9 种） | `editing_protocol.py:152-231` | editing.rs 全量端口（`EDITOR_PROTOCOL_VERSION` editing.rs:41） | `RUST_PARITY_PENDING` |
| `editor-state.schema.json`（207 行） | `protocol_version: "1.0"`；draft_id/revision/candidate_id/undo_depth/redo_depth/students/seats/hard_constraints | `editing_protocol.py:257-267` | editing.rs `EditorState`（:616-627） | `RUST_PARITY_PENDING` |
| `student.schema.json`（116 行） | 无版本字段；student_id/name/gender/height_cm/score/vision/notes/tags/needs/attributes | `models/student.py` | core `models.rs` `Student`（:31-46）**无 gender 字段**（求解子集） | `RUST_PARTIAL` |
| `classroom-layout.schema.json`（215 行） | 无版本字段；layout_id/seats/adjacency | `models/layout.py` | core `models.rs` `Layout`/`Seat`/`AdjacencyConfig`（:75/:187/:131）为求解子集（zone/group_id/near_* 等展示字段缺失） | `RUST_PARTIAL` |
| `plan-comparison-report.schema.json`（217 行） | `schema_version: "0.2.2"`；candidates/PlanComparisonEntry/explanations | `models/candidate.py` 相关 | 无 typed 结构（PlanScore 未移植） | `PYTHON_ONLY` |

版本常量（Python `schema.py:11-16`，Rust `migration.rs:48-52` 完全镜像）：Project=1、Snapshot="1.0"、Candidate="0.2.2"、RotationPlan="1.0"、Ruleset=1、EditorProtocol="1.0"。

### 4.2 Editing Protocol 机制（draft_id / revision / command_id）

| 机制 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| 三件套冲突检测：draft_id 匹配 / command_id 幂等 / base_revision 校验 | `_validate_command_target` api/drafts.py:243-260 | `validate_command` editing.rs:657-688 | `RUST_PARITY_PENDING` |
| dispatch 成功 `revision += 1` + 记录 command_id + 匿名 command_log | api/drafts.py:157-182（:173-180） | `apply_command` editing.rs:782-783 | `RUST_PARITY_PENDING` |
| undo/redo 整命令级快照栈 | api/drafts.py:202-230 | editing.rs（EditorDraft） | `RUST_PARITY_PENDING` |
| 操作种类（9 种：swap/move/batch_move/seat/unseat/lock/unlock × student/seat） | `EditorOperationDTO` editing_protocol.py:37-137 | `EditorOperation` editing.rs:591-596（payload 保持 JsonMap 延迟解析） | `RUST_PARITY_PENDING` |
| 上限：apply ≥1 操作、undo/redo =0、展开后 ≤100 | editing_protocol.py:211-231 | editing.rs 对应校验 | `RUST_PARITY_PENDING` |
| Draft store：有界 20、TTL 6h、线程安全 | api/drafts.py:232-240 | `EditorDraftStore = Mutex<HashMap>` editing.rs:831 | `RUST_PARITY_PENDING` |

注：`protocol_version` 两侧均为 "1.0"（editing_protocol.py:16、editing.rs:41），schema 判别联合用 `discriminator: kind`。

### 4.3 Rust 侧 schema 支持总体

- Rust **无 JSON Schema 生成器**（schemas/*.json 由 Python pydantic `JSON_SCHEMA_ARTIFACTS` 生成，schema.py:33-102）；`schema list/export` 命令 Rust-only 无对应。
- `native/seattrellis_core` 的 serde 结构是**求解专用协议**（`CoreSolveRequest` lib.rs:397-433），不是 seat 数据 schema。

---

## 5. roster import / mapping / update

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| CSV/XLSX 上传解析（20MB 上限、413/503） | api/rosters.py:48；DEFAULT_MAX_ROSTER_FILE_BYTES | roster.rs:976（server.rs:434） | `RUST_PARITY_PENDING` |
| 名单摘要（总人数/性别/高度等） | application/roster_import.py:61 | 对应响应字段 | `RUST_PARITY_PENDING` |
| 自动推断列映射（`_looks_like_identifier`/`_looks_like_person_name` 启发式） | application/roster_mapping.py:214-238 | 未确认镜像（Rust roster.rs 建议映射逻辑需逐项比对） | `RUST_PARTIAL` |
| 映射校验/模板生成/模板应用 | application/roster_mapping.py:351/:394/:407 | 未确认 | `RUST_PARTIAL` |
| `roster_fingerprint`（防无变更提交） | application/roster_update.py:119 | 未确认 | `RUST_PARTIAL` |
| 增量/替换更新预览（匹配链 student_id→name→new、冲突检测） | application/roster_update.py:133 | roster.rs:1000（server.rs:440） | `RUST_PARITY_PENDING` |
| 应用预览（并发版本检查 `StaleRosterRevisionError`） | application/roster_update.py:366 | roster.rs 对应 | `RUST_PARITY_PENDING` |
| `RosterDraftStore`（有界、TTL） | api/rosters.py:48 | roster.rs store | `RUST_PARITY_PENDING` |

---

## 6. layout editor

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| 草稿创建（template_id / layout / rows+columns 三选一） | api/layouts.py:51；models.py:757 | layouts.rs:759（server.rs:455） | `RUST_PARITY_PENDING` |
| 命令（apply/undo/redo + 7 种 operation：set_cell/insert_row/delete_row/insert_column/delete_column/translate/mirror_horizontal/flip_vertical） | api/layouts.py:70；api/models.py:819-830 | layouts.rs:769（server.rs:461） | `RUST_PARITY_PENDING` |
| 版本/幂等冲突（409）、拒绝（422） | api/layouts.py（LayoutRevisionConflictError） | layouts.rs 对应 | `RUST_PARITY_PENDING` |
| 编译为可求解 `ClassroomLayout` | api/layouts.py:106 | layouts.rs:774（server.rs:464） | `RUST_PARITY_PENDING` |
| `LayoutDraft`：rectangular/from_layout/ordered_cells | application/layout_editor.py:82/:110/:136/:170 | layouts.rs 对应 | `RUST_PARITY_PENDING` |

---

## 7. Hard rules

| 规则 id | 参数 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|---|
| `fixed_seats` | `student`, `seat_id` | 模型 rules.py:23；解析 rule_compiler.py:139-192；判定 assignment_validator.py:100-109 | 校验 lib.rs:124-126/:226-228；求解 lib.rs:932 | `RUST_PARITY_PENDING` |
| `must_be_adjacent` | `students[2]` | rules.py:37-50；判定 assignment_validator.py:111-125 | 校验 lib.rs:129-137；求解 lib.rs:939-951；测试 lib.rs:1265 | `RUST_PARITY_PENDING` |
| `cannot_be_adjacent` | `students[2]` | rules.py:37；判定 assignment_validator.py:111-125 | 校验 lib.rs:139-147；求解 lib.rs:952-963 | `RUST_PARITY_PENDING` |
| `min_distance` | `students[2]`, `distance>0`, `metric∈{euclidean,graph}` | rules.py:53-61；判定 assignment_validator.py:128-162 | 校验 lib.rs:149-160/:240-245；求解 lib.rs:965-977 | `RUST_PARITY_PENDING` |
| `groups`（展开为成对约束） | `name`, `students[]`, `separate`, `together` | rules.py:144-172；展开 rule_compiler.py:194-214 | 展开 `resolve_group_rules` lib.rs:467-514（与 Python 完全镜像） | `RUST_PARITY_PENDING` |

说明：以上 5 项 Rust 实现完整且有单元测试（`seattrellis_core` 共 32 个 `#[cfg(test)]`），但尚未经 golden fixture 差分 → 状态为 `RUST_PARITY_PENDING`；3.2 验证通过后升级 `RUST_VERIFIED`。

---

## 8. Soft objectives

| 目标 id | 参数 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|---|
| `vision_front` | `enabled`, `weight` | 模型 rules.py:273-288；成本 backend_common.py:30-31；评分 scoring.py:441-477 | 模型 models.rs:464-467；成本 cost.rs:98-102 | `RUST_PARITY_PENDING` |
| `height_back` | `enabled`, `weight` | rules.py:273；成本 backend_common.py:32-34；评分 scoring.py:407-438 | 模型 models.rs:468-471；成本 cost.rs:103-110（银行家舍入 cost.rs:59-74） | `RUST_PARITY_PENDING` |
| `randomize` | `enabled`, `weight`, `seed` | rules.py:273；种子 fallback_backend.py:320 | 模型 models.rs:472-475；成本 cost.rs:111-113（SplitMix64，确定性等价） | `RUST_PARITY_PENDING` |
| `score_balance` | `enabled`, `weight` | fallback_backend.py:326-340；ortools_backend.py:250+；评分 scoring.py:371-404 | 模型 models.rs:476；成本 lib.rs:773-788 | `RUST_PARITY_PENDING` |
| `fair_rotation`（历史公平/多期轮换） | `enabled`, `weight`, `avoid_repeating_categories[7]`, `lookback=4` | 模型 rules.py:77-103；成本 history.py:449-480；评分 scoring.py:280-312 | 模型 models.rs:346-378；成本 cost.rs:124-184；分类 cost.rs:185-260 | `RUST_PARITY_PENDING` |
| `avoid_recent_neighbors` | `enabled`, `weight`, `relation_types`, `lookback=4`, `max_recent_count=1`, `within_distance=2` | 模型 rules.py:106-141；成本 history.py:348-378；评分 scoring.py:315-368 | 模型 models.rs:379-408；成本 cost.rs:281-328；关系检测 cost.rs:329-386 | `RUST_PARITY_PENDING` |
| `cooling`（关系冷却） | `enabled`, `weight=5`, `cooling_period=3`, `relation_types`, `within_distance=2` | rules.py:175-216；合并 rules.py:308-346 | 模型 models.rs:410-436；合并 models.rs:536-571 | `RUST_PARTIAL`（**两语言同为近似**：均合并进 avoid_recent_neighbors，`cooling_period`→`lookback` 近似，无法表达"冷却期内完全禁配"强语义；需 v2 决策） |
| `score_position` | `enabled`, `weight`, `direction∈{high_front,high_back}` | rules.py:219-226；评估 soft_objectives.py:155-173；评分 scoring.py:505-536 | 模型 models.rs:271-291；评估 objectives.rs:200-239 | `RUST_PARITY_PENDING` |
| `score_distribution` | `enabled`, `weight`, `scope∈{row,group}` | rules.py:229-232；评估 soft_objectives.py:175-209；评分 scoring.py:505-536 | 模型 models.rs:292-312；评估 objectives.rs:241-308 | `RUST_PARITY_PENDING` |
| `mentor_pairing` | `enabled`, `weight`, `mentor_percentile=0.75`, `learner_percentile=0.25`, `relation`, `avoid_recent_repeats`, `history_lookback=4` | rules.py:235-260；配对选择 soft_objectives.py:262-341；评估 :211-260 | 模型 models.rs:313-345；评估 objectives.rs:310-380；选择 objectives.rs:389+（匈牙利移植） | `RUST_PARITY_PENDING` |

特别确认：

- **历史公平 / 近期邻座 / 冷却 / 多期轮换**均已覆盖（上表）；类别枚举 `models/history.py:10-20`（10 类座位位置）、关系类型枚举 `models/history.py:23-29`（6 种邻座关系），Rust 侧对应存在。
- **gender 无任何规则/目标**：Python 与 Rust 两侧均无（`gender` 仅是学生数据字段，models/student.py:46；Rust Student 甚至无此字段）。若 v2 需要性别相关目标，属从零设计，不属于 parity 缺口。
- 评分层（各 soft 的 0-100 归一化 PlanScore、`diversity_score`、`stability_score`）**无 Rust 对应** → `PYTHON_ONLY`（见 §2.4、§10）。
- app goal 覆盖：`goal_rules.rs` 仅 4 个 goal、6 个 soft 规则权重；`score_position`/`score_distribution`/`mentor_pairing`/`cooling` 固定 disabled（goal_rules.rs:70-112）；Python 15 个 preset 中 11 个在 app 无对应 → `RUST_PARTIAL`（见 §2.3）。

---

## 9. history / pair history

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| 数据结构：`SeatHistoryRecord`/`StudentSeatHistory`/`SeatHistory` | models/history.py:54/:73/:176 | core models.rs:581/:588/:619（打分 DTO 子集） | `RUST_PARITY_PENDING` |
| 数据结构：`PairHistoryRecord`/`StudentPairHistory`/`PairHistory` | models/history.py:93/:117/:151 | core models.rs:638/:653/:697 | `RUST_PARITY_PENDING` |
| 构建：`load_history_snapshots`/`build_seat_history`/`build_pair_history` | history.py:45/:65/:135 | 无独立模块（app 仅文件目录浏览 projects.rs:451-483） | `PYTHON_ONLY` |
| 关系检测 `detect_neighbor_relation_types` / `student_pair_key` | history.py:237/:276 | cost.rs:329-386/:448（求解用） | `RUST_PARITY_PENDING` |
| 成本：`avoid_recent_neighbors_cost`/`fair_rotation_cost`/`classify_seat_position` | history.py:348/:449/:409 | cost.rs:281/:124/:185 | `RUST_PARITY_PENDING` |
| 报告：`build_fairness_report`、`compute_history_report`、`compute_pair_report` | history.py:381；service.py:473/:480 | **无对应**（docs/rust-migration.md:73 明确属待办） | `PYTHON_ONLY` |
| 报告：`run_history_report`/`run_pair_report`（CLI） | service.py:969/:990 | 无对应 | `PYTHON_ONLY` |

---

## 10. candidate generation / comparison

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| 候选生成（1–20，seed 支持） | service.py:107-175；models.py:185（API 限制 ge=1, le=20） | core `solve_problem`（单解）；多候选路径未确认 | `RUST_PARTIAL` |
| recommended 选择 | service.py（compute_solve） | 未确认 | `RUST_PARTIAL` |
| PlanScore / ScoreBreakdown（7 维度 + rule_scores + hard_constraint_summary） | scoring.py:280-636 | 无对应（Rust 只有成本总和） | `PYTHON_ONLY` |
| `diversity_score`（候选间换座比例） | scoring.py:82-130 | 无对应 | `PYTHON_ONLY` |
| `stability_score`（与最近历史同座比例） | scoring.py:480-502 | 无对应 | `PYTHON_ONLY` |
| 计划比较报告（plan-comparison-report） | candidate_report.py:77；schema 0.2.2 | 无对应 | `PYTHON_ONLY` |
| `hard_constraint_summary`（完整性+硬规则复核） | scoring.py:99-112 | lib.rs:107-166（`evaluate_problem` 等价） | `RUST_PARITY_PENDING` |

---

## 11. repair / editing

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| Editing 协议端点（GET draft / POST commands / DELETE） | http.py:721/:729/:741 | editing.rs:856/:884；server.rs:446-451 | `RUST_PARITY_PENDING` |
| `EditorDraftStore`（create/state/snapshot/dispatch/delete/clear） | api/drafts.py:45-190 | editing.rs `EditorDraftStore`（:831） | `RUST_PARITY_PENDING` |
| 文件级编辑 `edit_snapshot` / `project_edit`（CLI） | service.py:764/:1154 | 无对应 CLI | `PYTHON_ONLY` |
| 受约束重解 `compute_repair` / `repair_snapshot` / `project_repair`（锁 + affected + 变更轨迹） | service.py:336/:807/:1181 | **无对应**（app 无 repair 端点/服务） | `PYTHON_ONLY` |
| 锁状态 `lock_state_from_snapshot` / `EditingSession` | editing.py:86-99 | editing.rs 对应 | `RUST_PARITY_PENDING` |

---

## 12. export formats / options / privacy modes

### 12.1 格式（Python 9 种 vs Rust 4 种）

| 格式 | Python 入口 | 选项 | Rust 现状 | 状态 |
|---|---|---|---|---|
| SVG | exporters/svg.py:23 | template/privacy/candidate/locale（固定 16:9，不接受 page） | render.rs:215（export.rs:69-74） | `RUST_PARITY_PENDING` |
| HTML | exporters/html.py:9 | 无选项 | render.rs:310 | `RUST_PARITY_PENDING` |
| print-html | exporters/print_html.py:88 | page/locale（A4 打印模板） | **退化**：Rust 服务端归一化为普通 html（server.rs:1512-1516） | `RUST_PARTIAL` |
| PNG | exporters/png.py:9 | 无选项 | render.rs:404（**无任何文字**，纯色块，render.rs:400-402） | `RUST_PARTIAL` |
| PDF | exporters/pdf.py:69 | template/privacy/page/orientation/scale/paper_size/margin_mm/locale | render.rs:559（手写 PDF；**CJK 名退化为 "?"**，render.rs:556-558；无 margin/paper_size 选项） | `RUST_PARTIAL` |
| DOCX | exporters/docx_export.py:26 | page 生效 | 无对应 | `PYTHON_ONLY` |
| PPTX | exporters/pptx.py:22 | 单页 16:9 可编辑形状 | 无对应 | `PYTHON_ONLY` |
| Excel | exporters/excel.py:9 | Seating+Assignments 两 sheet | 无对应 | `PYTHON_ONLY` |
| 候选集比较报告 | exporters/candidate_report.py:77 | page/locale（不含学生字段） | 无对应 | `PYTHON_ONLY` |

导出白名单 `export_extension`（service_types.py:375-384）：excel/xlsx、html、png、pdf、docx、svg、pptx、print-html。格式选项拒绝规则（exporters/__init__.py:217-231）：基础 HTML/Excel/PNG 不接受 template/privacy/page/locale；SVG/PPTX 不接受 page；report 模板必须带 candidate；candidate_scope="all" 需候选集。

### 12.2 Privacy modes

| 条目 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `PrivacyOptions`（6 开关：hide_scores/hide_notes/hide_special_needs/anonymize/show_height/show_vision） | service_types.py:43-78 | export.rs:158-173 接受全部 6 位，**仅 `anonymize` 生效**（export.rs:44-46 注释） | `RUST_PARTIAL` |
| 模板默认：public（隐藏 3 项+身高视力）、teacher（全开放）、report（显分数隐其余） | `PrivacyOptions.for_template()` service_types.py:54-78 | export.rs:113-131；**report 与 teacher 渲染无差异** | `RUST_PARTIAL` |
| 统一过滤：匿名编号「学生 01」、逐字段隐藏、教师版元数据剔除 | exporters/presentation.py:34-118 | 仅 `anonymize_grid` export.rs:301-325（public 或 anonymize 时替换姓名） | `RUST_PARTIAL` |
| public 版「🔒 班级公示版」徽标 | print_html.py:235-248 | 无对应 | `RUST_PARTIAL` |
| 项目隐私扫描（`_SENSITIVE_KEYS`） | project_bundle.py:27/:137 | projects.rs:706（scan） | `RUST_PARITY_PENDING` |

### 12.3 页面选项

| 条目 | Python | Rust | 状态 |
|---|---|---|---|
| orientation（portrait/landscape） | PDF/DOCX/print-html | 仅 PDF 生效（export.rs:264-269） | `RUST_PARTIAL` |
| page_scale | 同上 | 仅 PDF（clamp 0.5–2.0，render.rs:545-548） | `RUST_PARTIAL` |
| margin_mm / paper_size | PDF/DOCX/print-html | 无对应 | `RUST_PARTIAL` |
| locale（zh/en） | 全部 | 仅影响匿名占位符（export.rs:302-305） | `RUST_PARTIAL` |

---

## 13. migration / backup / restore

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| artifact 识别（6 类：candidate_set/plan_comparison_report/project/rotation_plan/snapshot/ruleset） | schema_migration.py:134-157 | migration.rs:200-230 | `RUST_PARITY_PENDING` |
| 迁移（读→校验→overlay→merge→变更摘要→备份→原子写） | schema_migration.py:35-84 | migration.rs:770-835（migrate_internal） | `RUST_PARITY_PENDING` |
| 失败回滚（写前备份、校验失败恢复） | schema_migration.py:87-116；handlers.py 回滚 | migration.rs:803（rollback_single_write） | `RUST_PARITY_PENDING` |
| 备份命名 `.bak`/`.bak.N` | schema_migration.py:199-214 | migration.rs 对应 | `RUST_PARITY_PENDING` |
| 单/批量 preview/apply/restore 端点 | http.py:625-657 | server.rs:485-502 | `RUST_PARITY_PENDING` |
| 当前迁移表为 no-op（仅校验+规范化+备份） | schema_migration.py:43-49 | 同（forward-safe） | `RUST_PARITY_PENDING` |
| Project bundle 打包/恢复（.seattrellis.zip v1、100MB/500MB 上限、manifest、防路径穿越） | project_bundle.py:20-22/:158/:210/:364-409 | projects.rs:56-62/:921/:1013/:1106-1177 | `RUST_PARITY_PENDING` |
| 项目产物对比/恢复（artifacts compare/restore，含 revision 链） | handlers.py:298/:380 | **无对应端点**（见 §3.2） | `PYTHON_ONLY` |
| 迁移 CLI（schema migrate） | cli.py:167-189 | 无 CLI 入口（app server 端点存在） | `RUST_PARTIAL` |

---

## 14. desktop native file workflows

| 条目 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `seattrellis workspace`（浏览器工作台启动器） | cli.py:195-230；workspace_server.py | app server（main.rs）等价启动器存在（`--port/--open-browser/--version`，main.rs:13/:36-60） | `RUST_PARITY_PENDING` |
| `seattrellis desktop`（pywebview 桌面壳） | cli.py:232-246；desktop.py:355 | Tauri 壳替代（app/src-tauri） | `RUST_PARTIAL` |
| `seattrellis-desktop`（独立 argparse CLI） | desktop_app.py:12-60 | Tauri 壳替代 | `RUST_PARTIAL` |
| Tauri 壳能力 | — | **空壳**：0 个 `#[tauri::command]`、无文件对话框/托盘/菜单/IPC（src-tauri/lib.rs:15-65），仅 WebView 加载 loopback HTTP；capabilities 仅 `core:default` | `RUST_PARTIAL` |
| 原生文件打开/保存对话框（桌面工作流核心体验） | desktop.py（pywebview 文件对话框） | **无对应**（依赖前端 `<input type=file>` + HTTP 上传/下载） | `PYTHON_ONLY` |
| Rust app server 整体（27 路由） | — | server.rs:427-524 | `RUST_PARITY_PENDING` |
| 前端静态资源内嵌 | — | embedded_web.rs:7/:13 | `RUST_PARITY_PENDING`（非 parity 项，仅记录） |

---

## 15. 汇总表

| 领域 | 条目数 | PYTHON_ONLY | RUST_PARTIAL | RUST_PARITY_PENDING | RUST_VERIFIED | INTENTIONALLY_REMOVED_V2 |
|---|---|---|---|---|---|---|
| §1 CLI（30 命令 + 契约） | 31 | 19 | 12 | 0 | 0 | 0 |
| §2 service/application | 24 | 8 | 10 | 6 | 0 | 0 |
| §3 React `/api/v1/*`（31 调用） | 31 | 3 | 0 | 28 | 0 | 0 |
| §4 Schema（10 文件 + 协议机制） | 16 | 3 | 5 | 8 | 0 | 0 |
| §5 roster | 8 | 0 | 3 | 5 | 0 | 0 |
| §6 layout editor | 5 | 0 | 0 | 5 | 0 | 0 |
| §7 hard rules | 5 | 0 | 0 | 5 | 0 | 0 |
| §8 soft objectives | 10 | 0 | 1 | 9 | 0 | 0 |
| §9 history/pair history | 7 | 4 | 0 | 3 | 0 | 0 |
| §10 candidates | 7 | 4 | 2 | 1 | 0 | 0 |
| §11 repair/editing | 5 | 2 | 0 | 3 | 0 | 0 |
| §12 export（格式/隐私/页面） | 18 | 5 | 12 | 1 | 0 | 0 |
| §13 migration/backup/restore | 9 | 1 | 1 | 7 | 0 | 0 |
| §14 desktop workflows | 7 | 1 | 4 | 2 | 0 | 0 |
| **合计** | **183** | **50** | **50** | **83** | **0** | **0** |

---

## 16. 差距总览（迁移工作清单来源）

### A. 前端阻断（§3.2，3 个 Python-only 端点）
1. `POST /api/v1/classes/rotation` — 轮转生成主流程（App.tsx:739），Rust 后端 404。
2. `POST /api/v1/projects/artifacts/compare` — 项目产物对比。
3. `POST /api/v1/projects/artifacts/restore` — 项目产物恢复。

### B. 生成能力（§2.2/§2.6/§10）
4. Rotation 计划生成器（逐期顺序求解 + 公平性惩罚）— Python-only。
5. History/pair 统计报告（history-report/pair-report/fairness report）— Python-only。
6. 候选集多解生成 / recommended / diversity / stability / PlanScore 评分层 — Python-only。

### C. 语义缺口（§8/§12）
7. `cooling` 为两语言共同的近似实现（`cooling_period`→`lookback`）— 需 v2 产品决策（保留近似 or 强化语义）。
8. Export 隐私粒度：Rust 仅 `anonymize` 生效，`hide_scores/hide_notes/hide_special_needs/show_height/show_vision` 与 report 模板渲染无差异。
9. 渲染保真：PNG 无文字、PDF 无 CJK、print-html 退化为 html、SVG/HTML/PNG 无打印版页面选项。

### D. 覆盖缺口（§1/§2/§9/§11/§14）
10. Rust CLI 命令面远小于 Python（3 vs 30）；history/project/migration 命令在 `native/seattrellis_cli/README.md:7-11` 明确列为 roadmap。
11. Repair（受约束重解）无 Rust 对应。
12. 文件级 edit/export/validate 的 preset/history 警告语义无 Rust CLI 对应。
13. Teacher goals：app 仅 4 goal（6/10 soft 规则），Python 15 preset 中 11 个不可达。
14. Tauri 壳无原生文件对话框；desktop 文件工作流依赖 Web 上传/下载。
15. Roster mapping 启发式（表头指纹、身份列推断）、roster_fingerprint 未确认镜像。
16. Rust 无 JSON Schema 生成器（`schema list/export` Python-only）。

### E. 工程债务（非 parity，但影响 v2 交付）
17. `seattrellis_native.pyi` 缺 `solve_problem` 声明（native/seattrellis_native/src/lib.rs:40 vs .pyi:4-17）。
18. CI 无 `cargo fmt` 步骤；无 deny.toml（无 cargo-deny 依赖审计）（.github/workflows/rust.yml）。
19. `app/` 与 `native/` 为两个独立 workspace（各自 Cargo.lock）；`app/src-tauri` 用 rust-toolchain 1.88.0，core 声明 rust-version 1.83。
20. `seattrellis_native` 为实验性绑定，README 声明不发布 wheel。

---

## 11. INTENTIONALLY_REMOVED_V2 登记（当前为空）

| 条目 | 理由 | 迁移方案 | 用户影响 | 状态 |
|---|---|---|---|---|
| （无） | — | — | — | — |

> 注：`gender` 相关规则在 Python 与 Rust 两侧均不存在（仅为学生数据字段），不属于"移除"，如 v2 需要属于新增设计。

---

## 附：M0 收口——oracle golden corpus 与差分 harness（2026-08-08）

### corpus 状态

- `fixtures/parity/`：41 个 case（34 合法 + 7 invalid），inputs 148 文件、
  goldens 192 文件；`MANIFEST.json` 记录每个文件的 SHA-256 与字节数、生成
  环境（Python 3.12.13、fallback backend、source commit）与
  `golden_contract`（启发式解不要求逐位一致，只要求语义/合法性/评分定义/
  质量门槛一致；时间戳字段规范化）。
- 可重放：`python scripts/gen_parity_fixtures.py verify` 在临时目录重生成
  全部 inputs+goldens 并逐字节比对（CI `parity-oracle` job 固定 Python 3.12）。
- 确定性预算：solve golden 使用 `--time-limit 1800`，保证 fallback 的
  `attempts = max(40, n*12)` 全部完成，不受墙钟截止影响（M0-03 发现：
  短预算下大 case 的 attempt 数随运行波动，golden 不稳定；300s 在慢 CI
  runner 上仍会命中截止，故取 1800s；`verify` 对截止命中的运行显式
  SKIP 警告而非误报 DIFF）。
- 导出 golden 契约：**没有任何导出格式字节稳定**——文本格式内容嵌入
  「生成时间」时间戳、PDF/DOCX/PPTX 携带时间戳、xlsx zip 存储文件 mtime
  （2s 粒度）、PNG deflate 流依赖平台 zlib 版本（M0-03 发现）。golden
  只记录语义契约（exit code、规范化输出、文本格式行数）；字节稳定性
  列为 M5-04 导出 parity 项。
- candidates golden 仅覆盖 n≤40（n=50/60/80 的候选集引擎是 M4-03 工作）。

### 差分 harness（M0-03）

`scripts/rust_python_diff.py`（七状态词表 + mismatch 非零退出）当前发现：

- SOLVED 类：benchmark 40/50/60 与全部 34 个合法 fixture case 两侧均为
  `SOLVED`（含 hard rules、中文名单、history、rotation）。
- TIMEOUT 类（60 人、0.1s 预算）：Python `TIMEOUT`，Rust `SOLVED` ——
  差距已关闭（M3-04/PR #95）：Rust 现在自带 `--time-limit`，且按 M1-03
  冻结语义，预算内找到合法 incumbent 的 `SOLVED` 优先于 `Timeout`，故
  harness 将 Python TIMEOUT + Rust SOLVED 计为 match；benchmark 差分 0 mismatch。
- INVALID_INPUT 类（7 个 invalid case）：Python 全部拒绝；Rust 对
  `invalid-empty-*`、`invalid-students-gt-seats`、`invalid-dup-student-id`
  均为 `INVALID_INPUT`（一致；dup 的拒绝原因不同——Python 在读入时拒重，
  Rust 经 degraded 转换后因学生数超座位数拒绝，深层校验差异留待 M2/M3），
  对 `invalid-unknown-rule`/`invalid-unknown-soft-objective`（core serde
  忽略未知字段）、`invalid-bad-adjacency-ref`（CLI 无法表达坏邻接布局）为
  `SOLVED` —— 3 个真实差距，对应 §4.1/§16，纳入 M2/M3 修复清单。

### 本轮修正的 fixture 缺陷

- `score_position.prefer`/`score_distribution.strategy` 字段名与 Python 模型
  不符（应为 `direction`/`scope`）。
- `p40-irregular-spare-sparse`（阶梯布局座位不足）与
  `p40-disabled-extra-dense`（禁用率过高）导致 39 座 vs 40 人；已加硬性
  守卫（合法 case 座位数 < 学生数即失败退出）。
- `invalid-dup-student-id` 此前未真正生成重复行；fixed 规则改为选取布局
  首个启用座位，避免固定到被禁用座位。

---

## 附：本次盘点来源

- 调研基于基线 commit `282fd99a7e766aaedaea6c5bb4c61e3ef14d257c` 的源码直接阅读，全部条目附 `文件:行号`。
- 关键参照文档：`docs/rust-migration.md`（:47-65 已交付项、:71-74 未完成项）、`docs/export.zh.md`、`docs/privacy.md`、`docs/rules.zh.md`、`docs/history.md`、`docs/pair-history.md`、`docs/project.zh.md`。
- 本账本状态与 `fixtures/parity/`（3.2 产出）联动：每条 `RUST_PARITY_PENDING` 升级 `RUST_VERIFIED` 时登记对应 golden 用例 id。
