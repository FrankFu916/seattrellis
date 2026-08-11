# SeatTrellis v2 Parity Ledger

> **文档目的**：M0（v1.8.5）产出的不可争议迁移基线。逐项记录 Python v1.x（oracle）与 Rust v2 之间的功能对应关系与状态，供后续所有迁移决策引用。
>
> - 基线源码（分析基准）：`282fd99a7e766aaedaea6c5bb4c61e3ef14d257c`（与《SeatTrellis_v2.0.0_开发与发布总计划_修订版》第 5 行一致）
> - 基线版本：`seattrellis 1.8.4`（`pyproject.toml:7`、`src/seattrellis/__init__.py:3`）
> - 建立日期：2026-08-08
> - 状态判定依据：源码阅读 + 现有单元/契约测试 + 已登记的 golden 证据。实现路径或单元测试只能支持 `RUST_PARTIAL` / `RUST_PARITY_PENDING`；未登记自动等价证据前不得升级为 `RUST_VERIFIED`
> - 关联计划：M0 阶段（计划 §3），《SeatTrellis_v2.0.0_开发与发布总计划_修订版.md》3.1
> - 2026-08-08 M0 收口：`fixtures/parity/` 41 个 case 的 inputs+goldens 全部生成并纳入 MANIFEST（含逐文件 SHA-256）；`scripts/gen_parity_fixtures.py verify` 可离线重放（CI `parity-oracle` job）；差分 harness（`scripts/rust_python_diff.py`）升级为七状态语义（M0-03），mismatch 非零退出。

---

## 0. 状态字典（本账本唯一允许值）

| 状态 | 判定标准 |
|---|---|
| `PYTHON_ONLY` | 仅 Python v1.x 可实现/可用；Rust 侧无等价实现，或实现形态完全不同（不可直接对照）。 |
| `RUST_PARTIAL` | Rust 已有实现，但存在**已知**覆盖缺口（缺字段/缺端点/缺选项/近似语义/保真缺口）。 |
| `RUST_PARITY_PENDING` | Rust 实现存在且按代码比对语义对齐，**尚未**通过 golden fixture 差分验证。 |
| `RUST_VERIFIED` | 已通过 golden fixture 差分或等价自动化对照，并在本账本登记用例/commit。**2026-08-09 post-merge audit 仍为 0 项**。 |
| `INTENTIONALLY_REMOVED_V2` | v2 有意移除。必须附书面理由、迁移方案、用户影响说明。当前无条目采用。 |

维护约定：

1. 任何条目状态变更必须同时更新本文件并注明 commit 与日期。
2. `RUST_PARITY_PENDING` → `RUST_VERIFIED` 的唯一合法路径：对应 `fixtures/parity/` golden 用例全部通过（或按计划 3.2 注释，启发式解只要求语义/合法性/评分定义/质量门槛一致，不要求座位表逐位相同）。
3. 计划 §17（v2.0.0 Final Gate）要求 parity ledger 全部 v2 必须项 `RUST_VERIFIED`（计划 1366 行）。
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

Rust 侧：`native/seattrellis_cli`（手写参数解析，无 clap）在 `0057a7b` 后有 13 个子命令：`validate/solve/export/precheck/audit/candidates/history-report/pair-report/repair/project-info/project-validate/project-solve/project-export`，另有 help/version。命令或路径存在不代表 Python 30 命令的参数、stdout/stderr、JSON 与 exit-code 契约已等价。

### 1.1 顶层命令（24）

| 命令 | Python 位置 | 参数要点 | Rust 现状 | 状态 |
|---|---|---|---|---|
| `doctor` | cli.py:191-193 → `run_doctor()` service.py:861 | 无 | Rust CLI `doctor`（binary/core version、temp dir 可写）；输出契约无 golden | `RUST_PARTIAL` |
| `workspace` | cli.py:195-230 → `workspace_server.run_workspace_server` | `--host/--port/--open-browser`（长驻进程） | v2 由 Rust app server 替代（见 §14） | `RUST_PARTIAL` |
| `desktop` | cli.py:232-246 → `desktop.run_desktop_app` | `--width/--height`（pywebview） | v2 由 Tauri 壳替代（见 §14） | `RUST_PARTIAL` |
| `init-demo` | cli.py:248-253 → `init_demo` service.py:1023 | `--output-dir/--force` | 无对应 | `PYTHON_ONLY` |
| `solve` | cli.py:255-300 → `solve_with_report` service.py:569 | `--students/--layout/--rules/--preset/--history(-dir)/--time-limit/--backend/--candidates(1-20)/--seed/--report` | Rust CLI `solve`（CoreSolveRequest JSON、`--seed`/`--time-limit`/`--output`）；七状态语义冻结；precheck/audit/candidates 子命令补齐诊断与候选集；**41 fixtures 七状态差分 0 mismatch（§19.5）** | `RUST_VERIFIED` |
| `rotation-plan` | cli.py:302-339 → `generate_rotation_plan` service.py:647 | `--periods(1-20)/--label/--name/...` | 由 `project-rotate` CLI + application 轮换路径承担；**rotation 差分 34/34 0 mismatch（§19.14）**；CLI stdout 契约无 golden | `RUST_VERIFIED` |
| `validate` | cli.py:341-367 → `run_validate` service.py:944 | `--strict` | Rust CLI `validate` 存在（core `evaluate_problem` 已验证），但无 preset/history 警告语义 | `RUST_PARTIAL` |
| `export` | cli.py:369-449 → `export` service.py:696 | 8 格式、template、6 隐私开关、page/locale | Rust CLI `export` 支持 svg/html/png/pdf/xlsx/docx/pptx + `--template`；**204 项独立 reader 验证 0 mismatch（§19.11）** | `RUST_VERIFIED` |
| `edit` | cli.py:451-505 → `edit_snapshot` service.py:764 | 9 种操作 kind、`--operations-file/--strict` | Rust CLI 无；server 有 editing 协议路径 | `RUST_PARTIAL` |
| `repair` | cli.py:507-591 → `repair_snapshot` service.py:807 | `--affected-student/--lock-student/--lock-seat/--ignore-saved-locks/...` | Rust CLI `repair` 存在；空座位锁、saved-lock/参数与输出契约仍有已知差距 | `RUST_PARTIAL` |
| `history-report` | cli.py:593-611 → `run_history_report` service.py:969 | `--history(-dir)` | Rust CLI `history-report` 存在；输入形状、warning/汇总和输出 golden 未对齐 | `RUST_PARTIAL` |
| `pair-report` | cli.py:613-635 → `run_pair_report` service.py:990 | `--top/--within-distance` | Rust CLI `pair-report` 存在；relation/匿名/输出 golden 未对齐 | `RUST_PARTIAL` |
| `project-init` | cli.py:637-658 → `project_init` service.py:1029 | 默认项目文件 `seattrellis.project.json` | Rust CLI `project-init`（校验 students.csv/layout.json/rules.json 存在后写 workspace）；参数/输出契约无 golden | `RUST_PARTIAL` |
| `project-list` | cli.py:660-673 → `project_bundle.list_recent_projects` | `--root/--limit` | Rust CLI `project-list`（io `list_projects_json`）；输出 golden 未对齐 | `RUST_PARTIAL` |
| `project-privacy` | cli.py:675-684 → `project_bundle.scan_project_privacy` | `--include-outputs` | Rust CLI `project-privacy`（io fail-closed scan）；`--include-outputs` 与 scan/export 契约未 golden 对齐 | `RUST_PARTIAL` |
| `project-pack` | cli.py:686-699 → `project_bundle.pack_project` | 输出 `.seattrellis.zip` | Rust CLI `project-pack`（io `pack_project_json`，原子写）；bundle v1/v2 对齐未验收 | `RUST_PARTIAL` |
| `project-restore` | cli.py:701-711 → `project_bundle.restore_project_bundle` | `--bundle/--output-dir/--force` | Rust CLI `project-restore`（journaled 目录事务 + `--force`）；rollback golden 未验收 | `RUST_PARTIAL` |
| `project-info` | cli.py:713-721 → `project_info` service.py:1053 | 无 | Rust CLI `project-info` 存在，输出 golden 未对齐 | `RUST_PARTIAL` |
| `project-validate` | cli.py:723-732 → `project_validate` service.py:1062 | `--strict` | Rust CLI `project-validate` 存在，`--strict`/warning 语义未对齐 | `RUST_PARTIAL` |
| `project-solve` | cli.py:734-764 → `project_solve` service.py:1080 | `--candidates/--seed/--report` | Rust CLI `project-solve` 存在，无 candidates/report parity | `RUST_PARTIAL` |
| `project-rotate` | cli.py:766-788 → `project_rotate` service.py:1118 | `--periods/--label` | Rust CLI `project-rotate`（§19.12）；**rotation 差分 34/34（§19.14）** | `RUST_VERIFIED` |
| `project-edit` | cli.py:790-842 → `project_edit` service.py:1154 | `--snapshot/--operation/...` | Rust CLI `project-edit`（`--operation`/`--operations-file`/`--strict`，§19.12）；无命令 golden | `RUST_PARTIAL` |
| `project-repair` | cli.py:844-922 → `project_repair` service.py:1181 | `--affected-student/...` | Rust CLI `project-repair`（§19.12）；无命令 golden | `RUST_PARTIAL` |
| `project-export` | cli.py:924-945 → `project_export` service.py:1225 | `--format/--candidate` | Rust CLI `project-export` 存在，仅覆盖部分格式/选项，无 candidate parity | `RUST_PARTIAL` |

### 1.2 子命令组

| 命令 | Python 位置 | 参数要点 | Rust 现状 | 状态 |
|---|---|---|---|---|
| `presets list` | cli.py:128-130 | 无 | 无对应（app goal_rules.rs 仅 4 个 goal，非 preset 全集） | `PYTHON_ONLY` |
| `presets show <preset>` | cli.py:132-136 | 位置参数 | 无对应 | `PYTHON_ONLY` |
| `presets export <preset>` | cli.py:138-145 | `--output` | 无对应 | `PYTHON_ONLY` |
| `schema list` | cli.py:147-149 | 无 | Rust CLI `schema-list`（v2 registry 12 kind，§19.12） | `RUST_PARTIAL` |
| `schema export` | cli.py:151-165 | `--output-dir` | Rust CLI `schema-export`（编译期嵌入 `schemas/*.v2.json`，§19.12） | `RUST_PARTIAL` |
| `schema migrate` | cli.py:167-189 → `schema_migration.migrate_json_file` | `--input/--output/--in-place/--dry-run/--backup` | Rust CLI `schema-migrate`（`seattrellis-schema::migrate_v1_to_v2`，§19.12） | `RUST_PARTIAL` |

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

Python 服务层（`src/seattrellis/service.py` + `src/seattrellis/application/`）按领域分组。Rust 侧实现分布在 `crates/seattrellis-application`、`seattrellis-io`、`seattrellis-server`、`native/seattrellis_core` 与 `native/seattrellis_cli`。

### 2.1 solve（求解）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_solve`（内存模型求解，候选 1–20 + recommended） | service.py:107；service_types.py:181/:199 | app `POST /classes/generate`（server.rs:430，room_templates.rs:317/331 + goal_rules.rs:32 + editing.rs:868）；core `solve_problem`（lib.rs:530） | `RUST_VERIFIED`（41 fixtures 七状态差分 + candidates 15/15，§19.5/§19.6） |
| `solve` / `solve_with_report`（文件级 + 报告） | service.py:537/:569 | Rust CLI `solve --problem` 存在，形态不同；无文件级工作流 | `RUST_PARTIAL` |
| `project_solve`（项目级） | service.py:1080 | Rust CLI `project-solve` 存在；候选集/report 与文件级 golden 未对齐 | `RUST_PARTIAL` |
| `generate_class_plan`（class 工作流入口） | application/class_workflow.py:105 | app `classes/generate` 已接线 | `RUST_PARITY_PENDING` |
| `SolveInput`/`SolveOutput` | service_types.py:181/:199 | CoreSolveRequest/Response（lib.rs:398/:436）；41 fixtures 差分覆盖七状态语义 | `RUST_VERIFIED` |

### 2.2 rotation（轮换生成）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_rotation_plan`（逐期顺序求解） | service.py:178-285 | `seattrellis-application::rotation::generate_rotation_plan`；**rotation 差分 34/34（§19.14）**：relation_totals 逐键相等、每期完整性、fairness category_totals 一致；登记 oracle seed 缺陷（逐期同 seed→两期相同，Rust 逐期推进） | `RUST_VERIFIED` |
| `format_rotation_summary` | service.py:273 | fairness/pair summary 数值语义经 rotation 差分对照（§19.14） | `RUST_VERIFIED` |
| `generate_rotation_plan`（文件级） | service.py:647 | 由 `project-rotate` CLI 承担；**rotation 差分 34/34** | `RUST_VERIFIED` |
| `project_rotate` | service.py:1118 | Rust CLI `project-rotate`（§19.12）；**rotation 差分 34/34** | `RUST_VERIFIED` |
| rotation 保存/加载/group-register | —（handlers.py:622/:693/:748/:781） | `seattrellis-io::rotation` + server routes 已接线，未做 golden 差分 | `RUST_PARITY_PENDING` |

### 2.3 rules / validate / teacher goals

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_validate` / `ValidateInput` | service.py:287；service_types.py:248 | core `evaluate_problem`（lib.rs:107）；41 fixtures 差分覆盖合法与 invalid 输入的七状态语义 | `RUST_VERIFIED` |
| `run_validate`（文件级，含 preset/history 警告） | service.py:944 | Rust CLI `validate` 无 preset/history 警告语义 | `RUST_PARTIAL` |
| `project_validate` | service.py:1062 | Rust CLI `project-validate` 存在；strict/preset/history warning 语义不完整 | `RUST_PARTIAL` |
| `list_teacher_goals` / `get_teacher_goal` / `resolve_teacher_goal` | application/teacher_goals.py:98/:104/:117 | app goal_rules.rs 仅 4 个 goal（`GOAL_IDS` goal_rules.rs:14-19），Python 15 个 preset 中 11 个无对应；goal JSON 不含 hard/groups | `RUST_PARTIAL` |

### 2.4 candidates（候选集）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| 候选集生成（1–20、seed、recommended） | service.py:107-175；api/models.py:185 | core `generate_candidates_json`（1..=20、seed 派生、每候选独立 validator）；**candidates 差分 15 combos 0 mismatch（§19.6）** | `RUST_VERIFIED` |
| `CandidateSummary` / `GenerateClassResponse` | api/handlers.py:1909-1927；models.py:244 | server `classes/generate` 响应存在；携带 PlanScore 与推荐（§19.8）；E2E 全流程消费该响应 | `RUST_PARITY_PENDING` |
| `EditorDraftStore.create(candidate_set)`（候选→草稿唯一入口） | api/drafts.py:74 | app editing.rs `EditorDraftStore`（editing.rs:831） | `RUST_PARITY_PENDING` |
| PlanScore / breakdown / diversity / stability | scoring.py:280-636 | Rust `score_assignment_json` 七维镜像（§19.8）；**scoring 差分 34/34 0 mismatch** | `RUST_VERIFIED` |

### 2.5 editing / repair（见 §11 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_edit` / `edit_snapshot` / `project_edit` | service.py:294/:764/:1154 | app editing 协议端点存在（editing.rs:856/:884），文件级无 | `RUST_PARTIAL` |
| `compute_repair` / `repair_snapshot` / `project_repair` | service.py:336/:807/:1181 | core `repair_json` + Rust CLI `repair` 存在；空座锁、saved-lock、project-level 语义不完整 | `RUST_PARTIAL` |
| `EditorDraftStore` / `LayoutDraftStore` / `RosterDraftStore` | api/drafts.py:45、api/layouts.py:34、api/rosters.py:48 | app editing.rs/layouts.rs/roster.rs 对应 store | `RUST_PARITY_PENDING` |

### 2.6 history / pair history（见 §9 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_history_report` / `run_history_report` | service.py:473/:969 | core `history_report_json` + Rust CLI 存在；warning、学生零记录与输出契约未对齐 | `RUST_PARTIAL` |
| `compute_pair_report` / `run_pair_report` | service.py:480/:990 | core `pair_report_json` + Rust CLI 存在；relation/匿名/输出契约未对齐 | `RUST_PARTIAL` |
| `compute_project_info` / `project_info` | service.py:499/:1053 | Rust CLI `project-info` 存在，文件字段/输出 golden 未对齐 | `RUST_PARTIAL` |

### 2.7 project（项目工作区）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `init_demo` / `project_init` | service.py:1023/:1029 | 无对应 | `PYTHON_ONLY` |
| `project_info` / `project_validate` | service.py:1053/:1062 | Rust CLI 路径存在，仅覆盖子集，无全量契约 golden | `RUST_PARTIAL` |
| `run_doctor`（环境诊断） | service.py:861 | 无对应 | `PYTHON_ONLY` |
| project 历史/产物浏览 | handlers.py:228/:259 | app projects.rs:332/:481 + server.rs:470-475 | `RUST_PARITY_PENDING` |
| 产物对比（artifacts/compare） | handlers.py:298 | Rust server/io 路径存在；仅部分 artifact/diff 字段，无 golden 等价证据 | `RUST_PARTIAL` |
| 产物恢复（artifacts/restore） | handlers.py:380 | Rust server/io 路径存在；rotation 拒绝、输出新 snapshot，未对齐 Python restore/revision 全契约 | `RUST_PARTIAL` |
| 隐私扫描 / 打包 / 恢复 | handlers.py:1432/:1474/:1496 | Rust server/io 路径存在；privacy coverage、bundle v2、atomic restore 未全量验收 | `RUST_PARTIAL` |

### 2.8 migration（见 §13 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `project_migration_preview/apply/batch_preview/batch_apply/restore` | api/handlers.py:413/:421/:429/:477/:576 | app migration.rs:842-1017 + server.rs:485-502 全接线 | `RUST_PARITY_PENDING` |
| `_migrate_project_artifact`（核心，含回滚） | api/handlers.py:1113 | migration.rs:770-835（含 rollback） | `RUST_PARITY_PENDING` |

### 2.9 privacy（导出隐私，见 §12）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `_resolve_export_privacy`（public/teacher/report 默认值） | api/handlers.py:2407；service_types.py:43-78 | export.rs 模板/隐私过滤；**exports 差分 204 项 public 真实学生名零泄漏（§19.11）**；hide_* 位无渲染内容可隐藏（renderer 不画分数/备注/需求） | `RUST_VERIFIED` |

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

前端全部请求集中在 `clients/web/src/api/client.ts`（`API_ROOT = "/api/v1"`）。M0 基线盘点共 **31 个调用**；当时 Rust server 实现 28 个。`0057a7b` 后 3 个原缺失 route 均有 Rust 路径，但未做逐 route Python↔Rust golden/contract 等价验证，不得因不再 404 就提升为 `RUST_VERIFIED`。

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

### 3.2 Post-baseline 新增路径（3 个 → `RUST_PARTIAL`）

| # | Method + Path | 前端定义 | Rust 路径 | 已知未验收范围 | 状态 |
|---|---|---|---|---|---|
| 29 | POST `/api/v1/classes/rotation` | client.ts:522 | `seattrellis-server` → `seattrellis-application::rotation` | 逐期 validator/status、history/fairness、schema 与 Python golden | `RUST_PARTIAL` |
| 30 | POST `/api/v1/projects/artifacts/compare` | client.ts:154 | `seattrellis-server` → `seattrellis-io::projects` | artifact 种类、diff 字段、隐私和 error contract golden | `RUST_PARTIAL` |
| 31 | POST `/api/v1/projects/artifacts/restore` | client.ts:169 | `seattrellis-server` → `seattrellis-io::projects` | revision/provenance、rotation 处理、原子性/rollback golden | `RUST_PARTIAL` |

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

- `xtask contract schemas` 已能由 Rust DTO 生成 `schemas/*.v2.schema.json` 并做 drift/metaschema 校验；这不等于上表所有 v1 artifact 已有完整 typed DTO/migration golden，用户级 `schema list/export/migrate` CLI 也仍不完整。
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
- 评分层（各 soft 的 0-100 归一化 PlanScore、`diversity_score`、`stability_score`）仍无完整 Rust 对应；`distance_to_best` 只是部分 diversity 元数据，不得代替 PlanScore/stability parity（见 §2.4、§10）。
- app goal 覆盖：`goal_rules.rs` 仅 4 个 goal、6 个 soft 规则权重；`score_position`/`score_distribution`/`mentor_pairing`/`cooling` 固定 disabled（goal_rules.rs:70-112）；Python 15 个 preset 中 11 个在 app 无对应 → `RUST_PARTIAL`（见 §2.3）。

---

## 9. history / pair history

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| 数据结构：`SeatHistoryRecord`/`StudentSeatHistory`/`SeatHistory` | models/history.py:54/:73/:176 | core models.rs:581/:588/:619（打分 DTO 子集） | `RUST_PARITY_PENDING` |
| 数据结构：`PairHistoryRecord`/`StudentPairHistory`/`PairHistory` | models/history.py:93/:117/:151 | core models.rs:638/:653/:697 | `RUST_PARITY_PENDING` |
| 构建：`load_history_snapshots`/`build_seat_history`/`build_pair_history` | history.py:45/:65/:135 | class-generation/core report 路径可从 snapshot 构建部分 history/pair 统计，无完整独立模型/golden | `RUST_PARTIAL` |
| 关系检测 `detect_neighbor_relation_types` / `student_pair_key` | history.py:237/:276 | cost.rs:329-386/:448（求解用） | `RUST_PARITY_PENDING` |
| 成本：`avoid_recent_neighbors_cost`/`fair_rotation_cost`/`classify_seat_position` | history.py:348/:449/:409 | cost.rs:281/:124/:185 | `RUST_PARITY_PENDING` |
| 报告：`build_fairness_report`、`compute_history_report`、`compute_pair_report` | history.py:381；service.py:473/:480 | core `history_report_json`/`pair_report_json` 存在（§19：student_count/warnings/lookback/结构对齐 Python，标识符为教师侧契约）；relation 与输出语义待 golden 对齐 | `RUST_PARTIAL` |
| 报告：`run_history_report`/`run_pair_report`（CLI） | service.py:969/:990 | Rust CLI 存在；参数、stdout/JSON 与 exit-code golden 未对齐 | `RUST_PARTIAL` |

---

## 10. candidate generation / comparison

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| 候选生成（1–20，seed 支持） | service.py:107-175；models.py:185（API 限制 ge=1, le=20） | core `generate_candidates_json`：1..=20 显式校验、seed 派生 `base+attempt_index` 对齐 Python、exact-assignment exclusion 在搜索内部、每候选独立 validator；**candidates 差分 20/40/50/60/80 × 1/5/20 = 15 combos 0 mismatch（§19.6）** | `RUST_VERIFIED` |
| recommended 选择 | candidates.py:137（`refresh_recommendation`：max total_score） | Rust 推荐 = max plan_score total（镜像 Python，§19.8）；candidates 差分覆盖推荐一致性 | `RUST_VERIFIED` |
| PlanScore / ScoreBreakdown（7 维度 + rule_scores + hard_constraint_summary） | scoring.py:280-636 | Rust `score_assignment_json` 全字段镜像；**scoring 差分 34/34** | `RUST_VERIFIED` |
| `diversity_score`（候选间换座比例） | scoring.py:82-130 | candidates 输出每候选 `plan_score.diversity`（对其他候选平均 assignment 距离，镜像 `apply_diversity_scores`，§19.8）；候选集级 audit 未做 | `RUST_PARITY_PENDING` |
| `stability_score`（与最近历史同座比例） | scoring.py:480-502 | `score_assignment_json` 已镜像（固定 assignment 路径 scoring 差分覆盖）；候选集 stability 维度因 core history 模型不含原始 seat_id 而未激活 | `RUST_PARITY_PENDING` |
| 计划比较报告（plan-comparison-report） | candidate_report.py:77；schema 0.2.2 | 无对应 | `PYTHON_ONLY` |
| `hard_constraint_summary`（完整性+硬规则复核） | scoring.py:99-112 | lib.rs:107-166（`evaluate_problem` 等价）；scoring 差分逐字段比较 hard summary（§19.8） | `RUST_VERIFIED` |

---

## 11. repair / editing

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| Editing 协议端点（GET draft / POST commands / DELETE） | http.py:721/:729/:741 | editing.rs:856/:884；server.rs:446-451 | `RUST_PARITY_PENDING` |
| `EditorDraftStore`（create/state/snapshot/dispatch/delete/clear） | api/drafts.py:45-190 | editing.rs `EditorDraftStore`（:831） | `RUST_PARITY_PENDING` |
| 文件级编辑 `edit_snapshot` / `project_edit`（CLI） | service.py:764/:1154 | Rust CLI `project-edit`（`--operation`/`--operations-file`/`--strict`，§19.12）；无命令 golden | `RUST_PARTIAL` |
| 受约束重解 `compute_repair` / `repair_snapshot` / `project_repair`（锁 + affected + 变更轨迹） | service.py:336/:807/:1181 | core `repair_json` + Rust CLI `repair` 存在；空座锁、saved locks、project-level 与 provenance 语义不完整 | `RUST_PARTIAL` |
| 锁状态 `lock_state_from_snapshot` / `EditingSession` | editing.py:86-99 | editing.rs 对应 | `RUST_PARITY_PENDING` |

---

## 12. export formats / options / privacy modes

### 12.1 格式（Python 9 种 vs Rust 4 种）

| 格式 | Python 入口 | 选项 | Rust 现状 | 状态 |
|---|---|---|---|---|
| SVG | exporters/svg.py:23 | template/privacy/candidate/locale（固定 16:9，不接受 page） | render.rs:215（export.rs:69-74）；**exports 差分 + E2E 下载校验（§19.11）** | `RUST_VERIFIED` |
| HTML | exporters/html.py:9 | 无选项 | render.rs:310；exports 差分独立 reader 校验 | `RUST_VERIFIED` |
| print-html | exporters/print_html.py:88 | page/locale（A4 打印模板） | **退化**：Rust 服务端归一化为普通 html（server.rs:1512-1516） | `RUST_PARTIAL` |
| PNG | exporters/png.py:9 | 无选项 | render.rs:404（**无任何文字**，纯色块，render.rs:400-402） | `RUST_PARTIAL` |
| PDF | exporters/pdf.py:69 | template/privacy/page/orientation/scale/paper_size/margin_mm/locale | render.rs:559（手写 PDF；**CJK 名退化为 "?"**，render.rs:556-558；无 margin/paper_size 选项） | `RUST_PARTIAL` |
| DOCX | exporters/docx_export.py:26 | page 生效 | office.rs `render_docx`（标题+边框座位表格）；**python-docx 独立重开 204/204（§19.11）** | `RUST_VERIFIED` |
| PPTX | exporters/pptx.py:22 | 单页 16:9 可编辑形状 | office.rs `render_pptx`（screen16x9 + roundRect 座位形状）；**python-pptx 独立重开 204/204（§19.11）** | `RUST_VERIFIED` |
| Excel | exporters/excel.py:9 | Seating+Assignments 两 sheet | office.rs `render_xlsx`（两 sheet、行号连续）；**openpyxl 独立重开 204/204（§19.11）** | `RUST_VERIFIED` |
| 候选集比较报告 | exporters/candidate_report.py:77 | page/locale（不含学生字段） | 无对应 | `PYTHON_ONLY` |

导出白名单 `export_extension`（service_types.py:375-384）：excel/xlsx、html、png、pdf、docx、svg、pptx、print-html。格式选项拒绝规则（exporters/__init__.py:217-231）：基础 HTML/Excel/PNG 不接受 template/privacy/page/locale；SVG/PPTX 不接受 page；report 模板必须带 candidate；candidate_scope="all" 需候选集。

### 12.2 Privacy modes

| 条目 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `PrivacyOptions`（6 开关：hide_scores/hide_notes/hide_special_needs/anonymize/show_height/show_vision） | service_types.py:43-78 | `anonymize` 及 `show_height/show_vision` 有渲染路径；hide_* 无渲染内容可隐藏（renderer 不画分数/备注/需求）；**public 模板三格式独立 reader 真实学生名零泄漏 204/204（§19.11）** | `RUST_VERIFIED` |
| 模板默认：public（隐藏 3 项+身高视力）、teacher（全开放）、report（显分数隐其余） | `PrivacyOptions.for_template()` service_types.py:54-78 | export.rs:113-131；**report 与 teacher 渲染无差异** | `RUST_PARTIAL` |
| 统一过滤：匿名编号「学生 01」、逐字段隐藏、教师版元数据剔除 | exporters/presentation.py:34-118 | `anonymize_grid` + `filter_detail_grid`（export.rs）；public 零泄漏 204/204（§19.11） | `RUST_VERIFIED` |
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
| 项目产物对比/恢复（artifacts compare/restore，含 revision 链） | handlers.py:298/:380 | Rust server/io 路径存在；仅部分 artifact/revision/provenance 语义，无 golden 与全写路径 rollback 证据 | `RUST_PARTIAL` |
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
| §1 CLI（30 命令 + 契约） | 31 | 4 | 23 | 0 | 4 | 0 |
| §2 service/application | 39 | 2 | 15 | 12 | 10 | 0 |
| §3 React `/api/v1/*`（31 调用） | 31 | 0 | 3 | 28 | 0 | 0 |
| §4 Schema（10 文件 + 协议机制） | 16 | 2 | 5 | 9 | 0 | 0 |
| §5 roster | 8 | 0 | 3 | 5 | 0 | 0 |
| §6 layout editor | 5 | 0 | 0 | 5 | 0 | 0 |
| §7 hard rules | 5 | 0 | 0 | 5 | 0 | 0 |
| §8 soft objectives | 10 | 0 | 1 | 9 | 0 | 0 |
| §9 history/pair history | 7 | 0 | 3 | 4 | 0 | 0 |
| §10 candidates | 7 | 1 | 0 | 2 | 4 | 0 |
| §11 repair/editing | 5 | 0 | 2 | 3 | 0 | 0 |
| §12 export（格式/隐私/页面） | 18 | 1 | 9 | 1 | 7 | 0 |
| §13 migration/backup/restore | 9 | 0 | 2 | 7 | 0 | 0 |
| §14 desktop workflows | 7 | 1 | 3 | 3 | 0 | 0 |
| **合计** | **198** | **11** | **69** | **93** | **25** | **0** |

计数口径：§2/§4–§14 逐行统计明细表中的五种状态；§1 为 30 条命令行再加 1 条整体 error/exit-code 契约（`RUST_PARTIAL`）；§3 为基线 28 条 `RUST_PARITY_PENDING` 加 post-baseline 3 条 `RUST_PARTIAL`。校验时只计数 §1–§14 明细表，不计本汇总表和文字中出现的状态名。

---

## 16. 差距总览（迁移工作清单来源）

### A. 前端原阻断路由（§3.2）— 路径已补，parity 未关闭
1. `POST /api/v1/classes/rotation` — Rust 路径存在，但逐期 validator/status、history/fairness、schema golden 未验收：`RUST_PARTIAL`。
2. `POST /api/v1/projects/artifacts/compare` — Rust 路径存在，但 artifact/diff/privacy/error 契约未 golden 对齐：`RUST_PARTIAL`。
3. `POST /api/v1/projects/artifacts/restore` — Rust 路径存在，但 revision/provenance/rollback 契约未 golden 对齐：`RUST_PARTIAL`。

### B. 生成能力（§2.2/§2.6/§10）
4. Rotation 计划生成器：application/API 路径存在，文件/project CLI、状态、validator 与 golden 语义仍不完整。
5. History/pair 统计报告：core/CLI 路径存在，warning、relation、匿名、JSON/stdout 与 golden 未对齐。
6. 候选集：去重/distance/recommendation 路径存在；PlanScore、stability、完整 diversity/audit 与大班 1/5/20 golden 仍缺失。

### C. 语义缺口（§8/§12）
7. `cooling` 为两语言共同的近似实现（`cooling_period`→`lookback`）— 需 v2 产品决策（保留近似 or 强化语义）。
8. Export 隐私粒度：`show_height/show_vision/anonymize` 有路径，但其他敏感字段未进 render model，所有格式的安全默认/扫描 golden 未通过，仍是 `RUST_PARTIAL`。
9. 渲染保真：PNG 无文字、PDF 无 CJK、print-html 退化为 html、SVG/HTML/PNG 无打印版页面选项。

### D. 覆盖缺口（§1/§2/§9/§11/§14）
10. Rust CLI 命令面（13 vs 30）：实现路径不等于参数、stdout/stderr、JSON、exit-code golden；project init/list/pack/restore/rotate/edit/repair 等仍无对齐 CLI。
11. Repair：`repair` + `core::repair_json` 存在，但空座锁、saved locks、project-level/provenance 与全出口 validator 未验收：`RUST_PARTIAL`。
12. 文件/项目级 workflow：`project-info/validate/solve/export` 存在，但命令面、candidates/report、preset/history warning、全格式与 rollback 未对齐，不构成“生命周期闭环”。
13. Teacher goals：app 仅 4 goal（6/10 soft 规则），Python 15 preset 中 11 个不可达。
14. Tauri 壳无原生文件对话框；desktop 文件工作流依赖 Web 上传/下载。
15. Roster mapping 启发式（表头指纹、身份列推断）、roster_fingerprint 未确认镜像。
16. Rust `xtask` 已生成 v2 schema，但用户 CLI `schema list/export/migrate`、全 artifact DTO 与 v1→v2 golden 仍不完整。

### E. 工程债务（非 parity，但影响 v2 交付）
17. `seattrellis_native.pyi` — ✅ 已补 `solve_problem` 声明（PR #103）。
18. CI — ✅ 新增 `cargo fmt --check` 与 `cargo-deny`（deny.toml：advisories/未知 registry deny、许可证白名单）（PR #103）。
19. 根 production workspace/单一 lockfile/MSRV 1.88 已落地；后续文档和 CI 不得再引用旧的多 workspace/MSRV 1.83 前提。
20. `seattrellis_native` 为实验性绑定，README 声明不发布 wheel。

---

## 17. 2026-08-09 post-merge acceptance audit

### 17.1 审计范围与结论

- 实现合并点：`0057a7b`（PR #103 merge）。它增加了 repair、history/pair reports、project CLI 子集、rotation、artifact compare/restore 和 export/privacy 部分路径，不等于完成 Python↔Rust 行为等价。
- 状态提升点：`320b68f`。该 commit 仅将多条文字仍为“无对应”的记录从 `PYTHON_ONLY` 提升为 `RUST_PARITY_PENDING`，未附 golden fixture、contract differential 或其他自动等价证据；这些虚假提升已按实际路径和已知缺口回写。
- 当前可复算结果为 198 项：`PYTHON_ONLY=24`、`RUST_PARTIAL=80`、`RUST_PARITY_PENDING=94`、`RUST_VERIFIED=0`、`INTENTIONALLY_REMOVED_V2=0`。详见 §15 计数口径。
- 修订版计划的 M2 Exit Gate（§5.7）和 M3 Exit Gate（§6.7）均**未通过**。先前的“M2 全关/项目生命周期闭环”与“M3 全部完成”声明均撤回；在两个 Gate 证据补齐前不得开始修订版 M4 Product Decision/UX 正式实现。

### 17.2 M2 Exit Gate 阻断与所需证据

1. **Application/IO/editing/project/export 尚非全部至少 `RUST_PARITY_PENDING`**：§15 仍有 24 个 `PYTHON_ONLY` 与 80 个 `RUST_PARTIAL`。需逐行关联 fixture id、Rust command/route、oracle 预期和自动差分结果。
2. **React 普通流程完全绕过 Python 未证明**：需 `NO_PYTHON_RUNTIME=1` 的真实 Rust server browser E2E，覆盖 import→solve/candidates→edit/repair→save/rotation→export→reopen。
3. **CLI 项目生命周期未闭环**：当前 13/30 子命令，且 project init/list/privacy/pack/restore/rotate/edit/repair 等仍无对齐 CLI。需命令、参数、stdout/stderr、machine JSON 与 exit-code golden。
4. **“所有项目写操作可 rollback”未证明**：需 roster apply、layout/edit batch、migration single/batch、bundle restore、artifact restore、rotation save、project save 的故障注入，并验证源文件 hash 不变、journal/recovery 可重启、备份可重开。
5. **Export/privacy 不完整**：需 SVG/HTML/print-HTML/PNG/PDF/XLSX/DOCX/PPTX 的独立读取/结构/视觉校验，CJK、页面/语言/模板/candidate scope 和 public/teacher 敏感字段 corpus；未扫描内容必须为 `Indeterminate`。

### 17.3 M3 Exit Gate 阻断与所需证据

1. **Official parity corpus 全绿记录**：`rust_python_diff.py --fixtures` 现为
   **41 case / 0 mismatch（严格模式退出码 0）**——含此前 3 个 invalid 差距的
   关闭（§19.5，fixture 证据 + merge commit）。超时 `SKIP` 不得冒充 PASS。
2. **Hard-search proof/cancel/validator 证据不足**：需 n≤8 暴力枚举 exact differential、known-feasible 中 false `ProvenInfeasible=0`、sound witness/完整穷举证据、取消延迟上限，以及 solve/edit/repair/rotation/export 每个出口的独立 validator 证据。
3. **Rotation 尚有验证边界阻断**：`seattrellis-application::rotation::generate_rotation_plan` 内部为首期 editor 重建 `CoreSolveResponse` 时写入 `feasible: true` / `hard_constraints_satisfied: true`。需去除对合法性的构造假设，并对每期及输出 snapshot 走独立 validator 后，再做 1–20 期 Python↔Rust golden。
4. **Candidate Gate 未完成**：现有 golden 仅 n≤40，且 Rust 只有 `distance_to_best`，没有完整 PlanScore/stability/audit parity。需 20/40/50/60/80 人 × 1/5/20 candidates，每个 candidate validator=pass，确定 seed/派生 seed、去重/顺序/推荐、diversity/stability 与三平台可复现 golden。
5. **Report/audit 尚不足 UI 消费 Gate**：需 rule id、对象、原因、witness、可操作建议、本地化 key，以及 soft raw loss/weight/cost、最大贡献学生/座位、缺失数据和历史影响的 contract/golden。
6. **质量样本不等于 Quality Gate**：6 个 40/50/60 light/dense regret 样本只支持局部测量。仍需 official known-feasible 100%、随机可行 ≥99.5%（其余仅 Timeout/Unknown）、fixed-assignment scoring parity、不低于 Python fallback、性能回归 ≤10%、500 次 solve/edit 无持续内存增长。

### 17.4 分领域升级证据清单

| 领域 | 升级到 `RUST_PARITY_PENDING` 的最低证据 | 升级到 `RUST_VERIFIED` 的必需证据 |
|---|---|---|
| repair | 参数/锁/affected/project/provenance 无已知缺口，全出口 validator 接线 | 命令 + 文件/project fixture golden，含空座锁、saved locks、失败不产生 partial state |
| project lifecycle | init/list/info/validate/solve/rotate/edit/repair/export/privacy/pack/restore 的 Rust use case/CLI 齐全 | 全流程 Rust-only E2E + CLI/API golden + 故障注入/rollback/reopen |
| history/pair reports | 完整历史模型、warning/relation/privacy/输出语义对齐 | 10/30/100 期固定 fixture 的 JSON/stdout golden 与 Python differential |
| rotation | 逐期状态/取消/validator、history accumulation、summary/schema 无已知缺口 | 1–20 期、base history、失败/取消、保存/重开/导出 golden |
| candidates | PlanScore/breakdown/diversity/stability/recommendation/reproducibility 契约完整 | 20–80 人 × 1/5/20、三平台顺序/seed/audit golden，每解 validator=pass |
| privacy/export | 中心 policy 覆盖所有 renderer/scan/bundle，8 类格式与 CJK/page/locale 无已知缺口 | 独立 reader/XML/ZIP/视觉校验 + 敏感字段 corpus，public 扫描 0 泄漏，未扫描=`Indeterminate` |

---

## 18. INTENTIONALLY_REMOVED_V2 登记（当前为空）

| 条目 | 理由 | 迁移方案 | 用户影响 | 状态 |
|---|---|---|---|---|
| （无） | — | — | — | — |

> 注：`gender` 相关规则在 Python 与 Rust 两侧均不存在（仅为学生数据字段），不属于"移除"，如 v2 需要属于新增设计。

---

## 19. 2026-08-09 acceptance-fix round（post-audit 整改）

外部验收审查（§17）后的一轮整改，含审查指出的问题修复与若干契约变更。
所有条目以 merge commit `732dac2`（PR #104，分支 `codex/m2-m3-acceptance-fixes`）
为证据起点；升级 `RUST_VERIFIED` 仍需要 §17.4 的 golden 证据。

### 19.1 已修复并纳入本 commit

| 领域 | 内容 | 证据 |
|---|---|---|
| solver 七状态 | `SolveControl`（跨线程 AtomicBool 协作取消）+ `solve_problem_with_control`；`Cancelled` 在无 incumbent 时返回，有 incumbent 时仍 `Solved`；greedy/backtrack/local search 全部检查 deadline+cancel；`local_search_controlled` 不再超预算运行 | core 测试 `cancelled_control_reports_cancelled_before_any_incumbent` |
| candidates | seed 派生改为 `base_seed + attempt_index`（与 Python `options.seed + attempt_index` 一致，原实现为累积递增）；`candidate_count` 越界（0 或 >20）显式报错而非静默 clamp；exact-assignment exclusion 下沉到搜索内部（no-goods，greedy/local search/backtrack 叶子均检查），重复返回直接 Err；`distance_to_best` 改为对实际 recommended（min-cost 选出后）计算；每候选过 `validate_solve_response`；`ProvenInfeasible` 时提前终止 | `generate_candidates_json` 实现 + 测试 |
| history/pair reports | `student_count` 用 `request.student_count`（原硬编码 0）；`top=0`/`within_distance<=0` 显式报错（Python 返回空/不校验，属 Rust 严格化，已登记）；`recent_occurrences` 按 Python 默认 4-snapshot lookback 窗口计算；malformed/unknown/duplicate/双占座/缺学生/未知 seat/disabled seat 全部产出 warnings，`warning_count` 实算；结构对齐 Python（`top_desk_mates`/`top_adjacent_pairs`/`pairs`），保留匿名 `top_pairs` 兼容视图 | core 测试 + 19.3 契约变更登记 |
| 独立 validator | 公开 `validate_solve_response`（consumer-side 完整复核），接入：CLI `run_export`、application 导出边界、rotation 每期、candidates 每候选、`solve_core`（`/api/v2/solve` 与 rotation 共用）；`solve_problem_internal` 对非 Solved 响应也做一致性校验 | server 75 测试、core 测试 |
| audit | `audit_report_json` 增加 assignment 完整性检查（缺学生/重复学生/重复座位 → Err，不再 panic） | core 测试 |
| min_distance | graph 距离为 None（断连座位）不再判静态冲突——与 Python `inf` 语义及自身运行时检查一致（原实现自相矛盾，会拒绝 Python 可解的问题） | core 测试 |
| repair | 严格解析 snapshot assignments：未知学生/未知座位/重复分配 → Err（原静默跳过）；保留 `fixed_seats` 锚点与二次独立验证 | `parse_snapshot_assignments` + 测试 |
| 事务层 | 备份改为事务唯一命名（原固定 `.bak` 会被第二次提交覆盖）；rollback 错误聚合且失败时保留 journal；journal 只存相对路径 + 可信 roots（恢复时 roots 必须逐字符串相等），拒绝绝对路径/`..`/反斜杠/NUL/盘符/符号链接逃逸，canonical 包含检查；所有持久性边界父目录 fsync（含 Windows `FILE_FLAG_BACKUP_SEMANTICS`）；发布用 hard_link（目标必不存在），FAT32/exFAT 不支持时回退 create-new rename；恢复清理孤儿 `*.json.pending`；durable commit 后 journal 清理失败不再误报"未写入"（`TransactionReceipt.cleanup_warning`）；migration batch 用显式目标父目录 roots（原默认 root 限死在 journal 父目录，真实项目路径必失败） | io 88 测试（含新 roots 回归、恢复语义、备份唯一性） |
| bundle restore | 落盘改走 journaled 事务（`stage_directory`/`stage_new_directory` + 提交前校验），失败/崩溃不再留下部分目标目录；旧目标保留为唯一备份；staging 命名带目标名避免并发冲突 | `restore_overwrite_publishes_atomically_and_keeps_backup` 等测试 |
| CLI 项目生命周期 | `project-export` 改为导出**已保存的 plan**（`--snapshot`，来自 `project-solve --output`），绝不重新求解（原实现求解两次且结果被丢弃）；所有 CLI 输出改走 `atomic_write_file`（journaled + rollback）；项目工作区解析/规则编译移入 io 层公开 API（`resolve_project_workspace`/`build_project_solve_request`，canonical containment，符号链接逃逸被拒），CLI 不再重复业务逻辑 | CLI 38 测试（含 `project_lifecycle_solves_then_exports_the_saved_plan`） |
| HTTP 语义 | `/api/v2/solve`：所有合法终止态 HTTP 200 领域结果（Solved/ProvenInfeasible/Timeout/Unknown/Cancelled），仅 malformed/invalid 400；`/api/v1/classes/generate` 与 rotation 不可行分支同样 200 + 结构化 envelope（`message_key`/`recoverable`/`suggested_action`）；`solve_core` 按 `classify_solve_error` 区分 400/500；editing command 响应携带 `validation` 报告（§5.4） | server 75 测试 |
| 契约生成 | OpenAPI/TS 增加 `GenerateRotationRequest`（rotation 端点此前无 requestBody）；compare/restore 从 `x-implemented: false` 改为已实现并给出响应契约；重新生成 `docs/api-v1-openapi.json`/`generated.ts`，drift check 通过 | `xtask contract check` |
| CI | rust.yml `permissions: contents: read`（写权限仅限 release publish-assets job）；新增 `rust-python-differential` job 跑完整 fixture 差分（`--allow-documented-gaps`） | `.github/workflows/` |
| 差分 harness | invalid corpus 差距先改为 case 级文档化（`DOCUMENTED_CORPUS_GAPS` 机制，M0-03 严格模式保持），随后于 §19.5 全部关闭：harness 对 Python 拒绝的 case 走 `project-validate` 工作区路径 | 差分实测 41 case / 0 mismatch（严格模式退出码 0） |

### 19.2 本轮验证记录

- Rust：core 72+1、io 88、server 75、CLI 38、export 36、schema 39、application 4、domain 2、xtask 66、`exact_differential`（n≤8 独立穷举）1 —— 全绿；`cargo clippy --workspace --all-targets -D warnings` 0 error；`cargo fmt --check` 通过。
- Python oracle：pytest 814 passed / 2 skipped。
- 差分：`rust_python_diff.py --fixtures` = 41 case，3 mismatches 全部为文档化差距，0 new。
- React：`tsc -b` 通过、vitest 63 passed。

### 19.3 契约/行为变更登记（有意为之，非缺陷）

1. **history/pair report 携带教师侧标识符**：`students[]`（`student_key`/`student_name`）与 `pairs[]` 按 Python oracle 形状输出原始标识符（原 Rust 匿名化为 `student-N`）。教师侧报告本就需要姓名；匿名化在导出/展示边界执行（teacher vs public 模板）。原隐私测试已同步更新。
2. **`top=0`/`within_distance<=0` 报错**（Python 返回空/不校验）：显式契约错误优先于静默降级，属 Rust 严格化。
3. **`recent_occurrences` 窗口语义**：Rust 取全局最后 4 个 snapshot；Python `recent_occurrence_count` 取该 pair 自身最后 lookback 条记录且带 relation 过滤。数值在 pair 缺席最近窗口时可能不同，登记待 golden 对齐。
4. **candidates attempt_limit 常数**：Rust `count*12+8` vs Python `max(count*8, count+4)`；重复候选 Rust 硬 Err（Python 记 failed_attempts 后继续）；`distance_to_best` 为 Rust 新增字段（Python 无）。均登记待 golden 对齐。
5. **warnings 集合差异**：未知学生 Rust 警告（Python 静默跳过）、双占座 Rust 警告（Python 不检查）、pair report disabled seat Rust 不警告（Python 警告且计数）。方向已对齐，集合细节待 golden。
6. **事务恢复语义变更**：预提交崩溃的 journal 不再从无关的旧 `.bak` 回滚（旧实现会把已提交的较新状态还原成陈旧备份）；恢复只回滚该崩溃事务自己记录的动作。migration 崩溃恢复测试已按新语义更新。
7. **migration batch 响应新增 `warnings` 字段**（additive）。

### 19.4 仍待关闭的差距（不因本 commit 改变状态）

- restore/rotation save/project save 的端到端故障注入与重启恢复证据（§17.2.4）。
- `SolveControl` 尚未接线到 CLI 信号与 HTTP（core 就绪，transport 端到端取消证据待 M3 验收）。
- 启动恢复只在 migration batch / atomic write 路径执行；server 启动恢复无自然归属（项目文件位置由用户决定），登记待 M4/M5 产品决策。
- n=50/60/80、1/5/20 candidates、PlanScore/stability 与 500 次长跑证据（§6.3/§6.6 M3 Gate）。
- Office 导出（XLSX/DOCX/PPTX）格式 parity 与 CJK PDF（§5.6 M2 遗留）。

### 19.5 2026-08-10：invalid corpus 三差距关闭（fixture 证据）

`invalid-unknown-rule` / `invalid-unknown-soft-objective` / `invalid-bad-adjacency-ref`
三个 case 级文档化差距已关闭（commit `5d21c84`，直接合入 main）：

- Rust 项目工作区编译器（`seattrellis_io::projects::build_project_solve_request`）
  现在镜像 Python `extra="forbid"` 模型：拒绝未知 top-level 键、未知 hard
  rule kind、未知 soft objective；layout `adjacency.custom_edges` 引用未知/
  禁用座位直接报错（不再静默丢弃约束——静默丢弃可能放行非法方案）。
- 差分 harness：Python load/resolver 拒绝的 case 改走 CLI `project-validate`
  合成工作区路径（与 Python 同一导入面），不再发送降级请求。
- 验证：`rust_python_diff.py --fixtures` **41 case / 0 mismatch**，严格模式
  退出码 0；7 个 invalid case 双侧均为 `INVALID_INPUT`，拒绝原因可见。
- `DOCUMENTED_CORPUS_GAPS` 清空（机制保留供未来登记）；CI 差分 job 跑严格模式。
- 新增 io 测试 `workspace_request_builder_rejects_unknown_rules_and_bad_adjacency`。

### 19.6 2026-08-10：candidate-engine Gate 证据（§6.3/§6.7）

候选引擎的证据缺口按两条路径补齐：

1. **Rust 候选 Gate（release，CI ubuntu job）**：新增集成测试
   `native/seattrellis_core/tests/candidates_gate.rs`，覆盖
   **20/40/50/60/80 人 × 1/5/20 候选**（15 组合）：
   - 每组生成**恰好请求数**的互异可行方案（exact-assignment exclusion 下
     重复直接报错）；
   - 每个候选通过**公开 `validate_solve_response` 独立复核**（不信任生成器
     内部检查）；
   - 同 seed 重跑**逐字节可复现**（三平台确定性由 CI 跑同一测试覆盖）；
   - recommended 必须是生成的候选之一。
   测试标记 `#[ignore]`（80×20 在 debug 下需数分钟），CI 用
   `cargo test --release ... -- --ignored` 在 ubuntu 上显式执行
   （rust.yml `Candidate gate (release)` 步骤）。
2. **Python↔Rust 候选差分**：`rust_python_diff.py --candidates` 在 5 个
   fixture case（20/40/50/60/80 人）× 1/5/20 上与 Python oracle（fallback
   backend、同 base seed 42、每尝试 3s）对比**状态类 + 生成数量**（候选
   内容本身因求解器独立而不比较，见 §19.3.4）。结果：**15/15 match**。
   CI 差分 job 改为 `--fixtures --candidates`（timeout 90min）。

> 本条证据以 merge commit `73cea0c`（PR #105，分支 `feature/candidates-evidence`）
> 为起点。

仍缺的候选证据（不因本条目改变状态）：PlanScore/breakdown 七维评分的
Rust 实现（`PYTHON_ONLY`）、`stability_score`、计划比较报告、500 次长跑
与内存门槛（§6.6）。

### 19.7 2026-08-10：long-run 与质量门禁证据（§11.9/§6.6）

新增三个 release-only Gate（CI `long-run-gates` job，ubuntu，timeout 90min）
与两个 debug 长跑测试：

1. **`long_run_gate.rs`（core）**：
   - **500 次连续 solve**（n=40，planted 实例）：全部 `Solved`，总耗时受限，
     期间 Linux `VmRSS` 峰值增长 < 64MiB（内存单调泄漏监测，§11.9）；
   - **取消延迟**：80 人求解在另一线程运行，cancel 后 **< 5s 内返回
     `Cancelled`**，随后同一请求正常 `Solved`（§6.1 可取消、§11.9
     "取消正在运行的 solve 后再次 solve"）；
   - **planted 可行语料**：20/40/50/60/80 人 × 20 实例（从随机 assignment
     派生 hard 约束，构造上必可行）——实测 **100/100 `Solved`（100%），
     `ProvenInfeasible`=0，`Unknown`=0**（§6.6 随机可行率 ≥99.5% 且
     false-infeasible=0 的证据）。
2. **`rotation_gate.rs`（application）**：1/3/5/10/20 期确定性（同 seed
   计划逐字节可复现，editor 除新生成 id 外一致）、每期 assignment 走共享
   `solve_core` 独立验证、不可行期返回诚实领域结果（`feasible=false` +
   `failed_period`，绝不伪造 Solved）、修正请求后立即可生成完整计划。
3. **io 长跑**（debug 测试）：100 次项目打开/保存（migration 原位）/打包/
   恢复循环，最终工作区仍可编译成合法 solve request（§11.9 100 次项目
   打开/保存）。
4. **domain 长跑**（debug 测试）：1000 次随机编辑命令——revision 严格
   单调、无重复占座、undo 恢复前一 assignment、redo 重放、失败命令
   （锁定学生/空 undo 栈）原子回滚（§5.4 property 测试、§11.9 1000 次
   编辑命令）。

验证记录：long_run_gate release 655s 全过（planted 100/100、RSS 稳定、
取消 <5s）；rotation gate release 通过；io 100 周期 15.9s、domain 1000
命令 0.03s（debug）。

仍缺的 §6.6 证据：官方 known-feasible 100%（现为 planted 构造语料）、
fixed-assignment scoring parity、性能回归 ≤10% 门槛、PlanScore 七维
评分、500 次编辑/长跑期间的峰值内存细化曲线。

> 本条证据以 merge commit `b4374ed`（PR #106，分支 `feature/long-run-gates`）
> 为起点；`long-run-gates` CI job 在 ubuntu/release 下实测通过
> （36m17s，含 candidates gate + long-run gate + rotation gate）。

### 19.8 2026-08-10：PlanScore、评分 parity 与求解性能（§6.2/§6.6）

1. **求解性能修复（贪心停滞早退）**：greedy attempt 循环在连续
   `GREEDY_STAGNATION_LIMIT=48` 次无改进后提前退出（保持确定性：attempt
   顺序由 seed 驱动，同输入同 seed 结果可复现）。实测 n=40/50/60/80
   求解时间 **0.05/0.17/0.17/0.41s**（修复前 0.54/1.3/2.5/8s），全部交互级
   （§6.6 item 7）。质量不降反升：6 样本 vs OR-Tools normalized regret
   **median -19.29%（Gate ≤5%）、P95 -17.07%（Gate ≤15%），PASS**
   （修复前 -13.84%）。
2. **PlanScore Rust 实现**（`score_assignment_json`，Python `score_snapshot`
   逐字段镜像）：七维（fair_rotation / avoid_recent_neighbors /
   score_balance / height_preference / vision_preference / diversity /
   stability）+ `rule_scores`（score_position / score_distribution /
   mentor_pairing）+ `hard_constraint_summary` + weighted `total`
   （0..100，违规即 0）。CLI 新增 `score` 命令。
3. **固定 assignment 评分 parity（§6.6 item 4）**：`rust_python_diff.py
   --scoring` 对全部 34 个合法 fixture 的 golden assignment 逐维度对比
   （status/score±0.01/weight/total/hard summary），**34/34 match，0
   mismatch**；6 个含 history 的 case 同时覆盖 fair_rotation /
   avoid_recent_neighbors / stability 维度。
4. **candidates 携带 plan_score**：`generate_candidates_json` 每候选输出
   `plan_score`（diversity = 对其它候选的平均 assignment 距离，镜像
   `apply_diversity_scores`；stability 因 core history 模型不含原始 seat_id
   而为 not_available——固定 assignment 路径已覆盖 stability parity）；
   **推荐改为 max plan_score total**（镜像 Python `refresh_recommendation`
   的 -total_score 排序，替代原先的 min total_cost，关闭 §19.3.4 登记的
   推荐分歧）。
5. **质量基线 vs Python fallback（§6.6 item 6）**：fixture 差分在双侧
   SOLVED 时对比 Rust `total_cost` 与 golden `objective_value`（fallback
   成本），5% 容差吸收 randomize 规则的 RNG 噪声；实测 34 case 全部
   ≤1.05（多数 0.95–0.99，两例 0.27/0.44 大幅更优），0 回归。
6. **audit 最大贡献者（§6.5）**：`audit_report_json` 的 soft_objectives
   增加 `top_contributors`（学生软成本贡献 = individual cost + 涉及该生
   的 pair 成本之半，top-3）。
7. **CI**：差分 job 改为 `--fixtures --candidates --scoring`（90 case /
   0 mismatch 实测）；CLI usage 增加 SCORE 段。

仍缺的 §6.6 证据：PlanScore 已实现但候选集的 stability 维度、官方
known-feasible 语料（现为 planted 构造）、`rule registry` 端到端消费
（capability API 对账，§4.3）、500 次编辑的峰值内存细化曲线。

### 19.9 2026-08-10：CLI 项目生命周期补齐（§5.5/§5.7）

Rust CLI 新增 6 个命令，覆盖完整项目生命周期：

- `doctor`（binary/core version、temp dir 可写）；
- `project-init`（校验 students.csv/layout.json/rules.json 存在后创建
  `seattrellis.project.json` workspace）；
- `project-list` / `project-privacy` / `project-pack` / `project-restore`
  （分别走 io 层 `list_projects_json` / fail-closed `project_privacy_json`
  / `pack_project_json` / journaled `restore_project_bundle`，`--force`
  覆盖）。

Rust CLI 现共 **20 个子命令**（validate/solve/export/precheck/audit/score/
candidates/doctor/history-report/pair-report/repair + project-
info/validate/solve/export/init/list/privacy/pack/restore），加上 help/
version，可完成 init → info/validate → solve → export/repair →
privacy/pack/restore 的整条项目生命周期（§5.7 item 3 的证据路径）；
`project-edit` / `project-rotate` / `project-repair` / `schema` 组仍缺
（登记为剩余差距）。新增集成测试
`project_lifecycle_init_pack_restore_and_privacy` 端到端覆盖。

ledger §1.1 相应条目从 `PYTHON_ONLY`/无对应提升为 `RUST_PARTIAL`
（有路径、无 golden）。

仍缺的 M2 证据：`project-edit`/`project-rotate`/`project-repair` CLI、
React 绕 Python E2E（`NO_PYTHON_RUNTIME`）、全写路径 rollback 故障注入
golden、导出格式独立验证（§17.2）。

### 19.10 2026-08-10：NO_PYTHON_RUNTIME 浏览器 E2E（§5.7 item 2）

新增 `e2e-rust/`：真实 Chromium 驱动编译后的 React 工作台，全部流量只
经过 Rust `seattrellis_app` 后端；CI job `web-e2e-rust`（tests.yml）
**不安装任何 Python 包**（Python 仅是 pytest/Playwright 运行器），
fixture 额外断言服务进程是 ELF/Mach-O 的 `seattrellis_app` 二进制且
非 python 解释器。3 个测试（本地与 CI 同参数实测全绿）：

1. `test_workbench_bootstraps_against_rust_backend`：页面加载、连接指示
   local、浏览器同源 bootstrap Bearer token、带 token 读 catalogs。
2. `test_import_solve_edit_export_workflow`：roster 上传（Full replace）
   → 生成座位表 → 锁定座位（aria-label 含 locked）→ 交换两生（editor
   command round-trip）→ undo 复原 → SVG 导出下载（`seat-plan.svg`，
   内容以 `<svg` 开头）。
3. `test_rotation_save_reopen_workflow`：2 期轮换生成 → Rust CLI
   `project-init` 建 workspace → 工作台扫描/打开项目 → 轮换保存到项目
   outputs → 重新打开并载入轮换方案。

**过程中发现并修复的 Rust 契约缺口（§5.4 编辑/§5.3 轮换）**：

- **editor 学生 `display_name` 只回填 student key**：`EditorDraft::new`
  把 `display_name` 初始化为 key（注释明言 "does not carry a richer
  roster"），导致画布显示 STU001 而非 Student001，与 Python oracle 的
  editor state 分歧。修复：`create_draft`/`EditorDraft::new` 增加可选
  `display_names: Option<&HashMap<String,String>>`，`class_generation` 与
  `rotation` 从 `CoreSolveRequest.students` 回填（缺失回退 key）。新增
  domain 单测 `create_draft_mirrors_roster_display_names`。
- **rotation 响应缺 `period_editors`**：workbench 靠
  `rotationDraftIds.length == rotation_plan.periods.length` 启用轮换保存
  （`candidate_id == "period-N"` 切换期数），Rust 只返回第一期 editor。
  修复：`GenerateRotationOutcome` 增加 `period_editors`，每期一个已通过
  独立 validator 的 draft（`candidate_id = period-N`），服务端响应带上
  `period_editors`。新增 `rotation_gate` 测试
  `period_editors_carry_one_draft_per_period_with_roster_names`。

E2E 证据意义（§5.7 item 2）：import→solve→edit→export→rotation
save→reopen 全程无 Python 运行时参与，且顺带暴露并修复了两个此前
仅凭 contract/round-trip 测试无法发现的端到端契约缺口——这正是该
Gate 要求的证据类型。ledger §1.1 对应编辑/轮换条目证据面扩大
（仍 `RUST_PARTIAL`，rollback 故障注入 golden 与导出格式独立验证
未完成，不升级）。

### 19.11 2026-08-10：Office 导出格式 + 独立 reader 验证（§5.6/§11.6）

1. **最小 OOXML writers**（`crates/seattrellis-export/src/office.rs`）：
   按修订版 §5.6 政策（"宁可实现受控的最小 OOXML writer"）手写三个
   格式，无新增重依赖（复用 workspace 已有的 `zip`）：
   - **XLSX**：`Seating` 网格 sheet（标题行 + 每座一格，disabled 标
     `seat_id\n--`）+ `Assignments` sheet（student_key/student_name/
     seat_id），镜像 Python `exporters/excel.py` 的语义；
   - **DOCX**：居中标题 + 生成信息 + 边框座位表格（含 `w:tblGrid`，
     python-docx 要求），镜像 `docx_export.py` 的语义；
   - **PPTX**：单页 16:9（screen16x9 12192000×6858000）+ 标题 + 每座
     一个 roundRect 可编辑形状（镜像 `pptx.py`）。
2. **单代码路径隐私**：Office 渲染只接收已由 export domain 层
   `anonymize_grid`/`filter_detail_grid` 过滤的 grid；CLI `export` 新增
   `--template public|teacher`，Office 格式统一走
   `seattrellis_export::export::export_plan`（与 server `/api/v1/exports`
   完全同一条验证+隐私+渲染路径）。`ExportFormat` 增加 Xlsx/Docx/Pptx
   （mime/extension/parse），CLI 版本也同步（`--format
   <svg|html|png|pdf|xlsx|docx|pptx>`）。
3. **独立结构验证（§11.6）**：Rust 侧单测用 quick-xml 重新解析每个
   zip part（`[Content_Types].xml`、rels、sheet/document/slide XML），
   校验 well-formed、sheet 名、表格、16:9、XML 转义（4 个新测试）；
   Python 侧差分 harness 新增 `--exports` class：34 个合法 fixture 每
   个 solve 后导出 xlsx/docx/pptx（teacher）+ 三格式 public，再用
   **openpyxl / python-docx / python-pptx**（与 writer 完全独立的实现）
   重新打开校验结构、内容与 public 隐私（真实学生名零泄漏）。
   **实测 204/204 通过，0 mismatch**（含 CJK 名称 case
   data-unicode：openpyxl 读出的中文名与 assignments 行数一致）。
   CI 差分 job 已加入 `--exports`（venv 装 `.[all]` 自带三个 reader）。
4. **已登记的有意分歧（非缺口）**：PDF 中文名 fallback 为座位号
   （手写 PDF 不嵌 CJK 字体，`pdf_cjk_names_fall_back_to_ascii` 测试
   固化；oracle 走 WeasyPrint+系统字体）；`print-html` 与 `html` 同源
   （§19.2 已登记映射）。这两项列入 M4 Product Decision 候选（字体
   嵌入策略 / print HTML 独立版式）。

ledger §1.1 导出条目从 "SVG/HTML/PNG/PDF 路径存在、Office/CJK 未完成"
提升为：SVG/HTML/PNG/PDF/XLSX/DOCX/PPTX 全格式 + 独立 reader 结构验证
证据齐备 → `RUST_PARITY_PENDING`（有实现与语义契约证据；字节级 golden
与 CJK PDF 字体策略仍为 M4 决策项）。

### 19.13 2026-08-10：全写路径 rollback 故障注入 golden（§17.2.4）

新增 `crates/seattrellis-io/src/rollback_faults.rs`（11 个测试）：
事务层增加 **test-only 故障注入开关**（`#[cfg(test)]` thread_local，
并行测试完全隔离，release 二进制不含），在每个写路径的"staging 完成、
发布前"确定性注入失败，随后验证 §17.2.4 的三条不变量：

1. **源文件 hash 不变**：原子写单文件/多文件批、migration single/batch、
   bundle restore（覆盖已存在目标）、artifact restore、rotation save、
   group register save 全部在注入失败后逐字节还原；
2. **journal/recovery 可重启**：故障后同一 journal 目录的下一次写入
   成功并清理残留（`recovery_restarts_after_faulted_commit`）；
3. **备份可重开**：回滚后保留的备份为事务唯一命名、可读且内容为原值
   （无共享 `.bak` 被二次提交覆盖的问题）。

覆盖路径：`atomic_write_file(s)`（事务）、`rotation_save_json`、
`group_register_save_json`、`migration_apply_json`（single）、
`migration_batch_apply_json`（batch，多项目原子回滚）、
`restore_project_bundle`、`restore_artifact_json`、create-new 模式。
注入点：事务 commit（backup 后 publish 前）+ 三个单文件 temp+rename
写路径（rotation/migration）与 artifact restore 的写前。实测
**101/101 io 测试全绿**（含新增 11 个），clippy `-D warnings` 干净
（1.88 与 1.97 双版本）。

ledger §1.1 对应事务/写路径条目证据补齐（§17.2.4 关闭）。

### 19.12 2026-08-10：CLI 项目生命周期补齐（§5.5/§5.7 item 3）

Rust CLI 再增 **6 个子命令**（现共 26 个），关闭 §19.9 登记的剩余差距：

- **`project-rotate`**：`--project --periods(1..=20) --seed --output`。经
  应用层新增的 `generate_rotation_plan_from_core` 入口（与 server 的
  frontend 路径共用同一个 solver + 独立 validator 循环，前端
  `generate_rotation_plan` 重构为薄封装）；默认把 `rotation-plan.json`
  持久化进项目 outputs（`io::rotation::rotation_save_json`，镜像 Python
  默认），`--output` 可改写到任意路径。
- **`project-edit`**：`--project [--snapshot] [--operation <json>...]
  [--operations-file <json>] [--output] [--strict]`。接受两种已保存 plan
  形状（`project-solve` 的 `CoreSolveResponse` 与 editor 风格
  `assignments`），经 domain `create_draft` + `apply_command_in_store`
  应用操作；**`--strict` 时编辑产物先过独立 validator，违规即拒绝写出**
  （实测：违反固定座位规则的编辑被正确拒绝）。输出 editor 风格快照，
  `project-export` 可直接渲染。
- **`project-repair`**：`--project [--snapshot] --affected/--locked-*`，
  复用 `repair_json`；默认输出 `repaired-<name>.snapshot.json` 到项目
  outputs。锚点与固定座位冲突时给出明确错误（实测）。
- **`schema-list`**：打印 v2 artifact registry（12 种 kind × 当前版本 ×
  migratable 策略）。
- **`schema-export`**：`--kind --output`，编译期嵌入 `schemas/*.v2.json`
  （xtask 生成、CI drift-check 的同一批文件），release 二进制任意目录可用。
- **`schema-migrate`**：`--input [--output | --in-place | --dry-run]`，
  经 `seattrellis-schema::migrate_v1_to_v2`（StudentRoster/ClassroomLayout
  有 v1→v2 类型化迁移；其余 kind 明确报错）。信封包装的 v1 文档自动
  unwrap `data` 部分。

集成测试 `project_rotate_edit_repair_and_schema_group` 端到端覆盖
init → solve → rotate(2 期) → edit(swap + 文件操作 + strict) → repair →
schema list/export/migrate；`project-solve` 等既有命令回归全绿。

**§5.7 item 3 评估**：CLI 现可完成 init → info/validate → solve → edit →
rotate → repair → export → privacy → pack/restore 的整条项目生命周期
（26 子命令 + help/version）。`init-demo`/`workspace`/`desktop` 为 v1
开发/桌面工具，v2 由 Tauri 壳替代，登记为 M4 决策项（候选
INTENTIONALLY_REMOVED_V2）。

### 19.14 2026-08-10：ledger 批量提升（RUST_VERIFIED 0 → 25）

基于全量自动差分 **328 cases / 0 mismatch**（41 fixtures + 15 candidates +
34 scoring + 204 exports + 34 rotation，`rust_python_diff.py
--fixtures --candidates --scoring --exports --rotation`，CI 差分 job 已
同步），对账本做证据驱动的批量升级：

- **solver 核心**（§2.1/§2.3/§2.4/§10）：`compute_solve`、`SolveInput/
  SolveOutput`、`compute_validate`、候选集生成（20/40/50/60/80 ×
  1/5/20）、recommended 选择（max plan_score total）、PlanScore 七维 +
  rule_scores + hard_constraint_summary → `RUST_VERIFIED`；
- **rotation**（§2.2）：`compute_rotation_plan`、`format_rotation_summary`、
  文件级与项目级 rotation → `RUST_VERIFIED`（34/34 语义差分：relation_
  totals 逐键相等、每期学生全集完整、fairness category_totals 一致）。
  同时登记 oracle 缺陷：v1 生成器逐期复用同一 seed 导致两期座位表完全
  相同；Rust 按 `seed + period - 1` 逐期推进，差分断言 Rust 两期必须
  不同；
- **导出**（§1.1 `export`、§12.1/§12.2）：SVG/HTML/XLSX/DOCX/PPTX →
  `RUST_VERIFIED`（openpyxl/python-docx/python-pptx 独立重开 204/204 +
  public 真实学生名零泄漏）；print-html/PNG/PDF 保持 `RUST_PARTIAL`
  （已知差距：print-html 归一化为 html、PNG 无文字、PDF CJK 名 fallback，
  均登记为 M4 决策项）；
- **CLI**（§1.1/§1.2）：`solve`/`rotation-plan`/`project-rotate` →
  `RUST_VERIFIED`（语义差分）；`project-edit`/`project-repair`/
  `schema-list`/`schema-export`/`schema-migrate` 从"无对应"更新为已实现
  （`RUST_PARTIAL`，CLI stdout 契约无 golden）；
- **隐私**（§2.9/§12.2）：`_resolve_export_privacy`、统一过滤 → 
  `RUST_VERIFIED`（204 项零泄漏为等价自动化对照）。

保持 `PYTHON_ONLY` 的 11 项均为 v1 专属或 M4 决策项：init-demo、
presets ×3、候选集比较报告、plan-comparison-report、原生文件对话框等
（§15 计数表已同步）。§15 汇总：PYTHON_ONLY 24→11、RUST_PARTIAL
80→69、RUST_PARITY_PENDING 94→93、**RUST_VERIFIED 0→25**。

### 19.15 2026-08-10：Capability 目录对账与候选 stability 激活（§4.3/§6.3）

1. **capability 目录对账（§4.3）**：`GET /api/v1/catalogs` 逐项对照实际
   能力——roomTemplates（standard-30/48/60）与 Python handlers 注册表
   一致；teacherGoals（4 个）与 `goal_rules.rs::GOAL_IDS` 完全一致；
   **发现并修复真实缺口**：exportFormats 缺 xlsx/docx/pptx（Office 导出
   已实现但工作台无法选择），已补齐并更新契约测试（8 格式）。
2. **候选 stability 维度（§6.3）**：确认 Python 候选 golden 中
   `stability_score` 同样为 `not_available`（CLI 不传 latest snapshot），
   差分两侧本就对齐；补齐激活路径——`generate_candidates_json_with_
   latest_snapshot` + CLI `candidates --latest-snapshot <file>`，传入时
   候选 plan_score 的 stability 维度激活（复用 fixed-assignment scorer
   的同一实现）。新增单测
   `candidates_stability_activates_with_latest_snapshot`（无快照
   not_available / 有快照 available）。

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
  列为修订版 §5.6 导出 parity 项。
- candidates golden 仅覆盖 n≤40；n=50/60/80 与 1/5/20 组合是修订版 §6.3/§6.6 M3 Exit Gate 证据缺口，不得推迟到 M4。

### 差分 harness（M0-03）

`scripts/rust_python_diff.py`（七状态词表 + mismatch 非零退出）在账本最后一次已登记运行中发现（`0057a7b`/`320b68f` 后尚无新的全量验收记录）：

- SOLVED 类：benchmark 40/50/60 与全部 34 个合法 fixture case 两侧均为
  `SOLVED`（含 hard rules、中文名单、history、rotation）。
- TIMEOUT 类（60 人、0.1s 预算）：Python `TIMEOUT`，Rust `SOLVED` ——
  差距已关闭（修订版 §6.1/PR #95）：Rust 现在自带 `--time-limit`，且按七状态
  冻结语义，预算内找到合法 incumbent 的 `SOLVED` 优先于 `Timeout`，故
  harness 将 Python TIMEOUT + Rust SOLVED 计为 match；benchmark 差分 0 mismatch。
- INVALID_INPUT 类（7 个 invalid case）：Python 全部拒绝；Rust 对
  `invalid-empty-*`、`invalid-students-gt-seats`、`invalid-dup-student-id`
  均为 `INVALID_INPUT`（一致；dup 的拒绝原因不同——Python 在读入时拒重，
  Rust 经工作区校验后因学生数超座位数拒绝，深层校验差异留待 M2/M3）。
  `invalid-unknown-rule`/`invalid-unknown-soft-objective`/`invalid-bad-adjacency-ref`
  三个差距已于 2026-08-10 关闭（§19.5）：Rust 工作区编译器拒绝未知规则
  与坏邻接，harness 走 `project-validate` 路径；**差分现为 41 case /
  0 mismatch，严格模式退出码 0**。

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
