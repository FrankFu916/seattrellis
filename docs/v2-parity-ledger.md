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

Rust 侧：`crates/seattrellis-cli`（手写参数解析，无 clap）现有 28 个命令（help/version/doctor + validate/precheck/audit/score/candidates/history-report/pair-report/repair/edit + project-init/list/info/validate/solve/export/rotate/edit/repair/privacy/pack/restore + schema-list/export/migrate + solve/export）。命令或路径存在不代表 Python 30 命令的参数、stdout/stderr、JSON 与 exit-code 契约已等价；**2026-08-12 起 `scripts/rust_python_diff.py --cli-golden` 对 33 个代表命令做 stdout/exit golden 对照，33/33 0 mismatch（§19.30）**。

### 1.1 顶层命令（24）

| 命令 | Python 位置 | 参数要点 | Rust 现状 | 状态 |
|---|---|---|---|---|
| `doctor` | cli.py:191-193 → `run_doctor()` service.py:861 | 无 | Rust CLI `doctor`（binary/core version、temp dir 可写）；**golden `fixtures/cli-goldens/doctor.json`（§19.30，33/33）+ Python exit 语义对照** | `RUST_VERIFIED` |
| `workspace` | cli.py:195-230 → `workspace_server.run_workspace_server` | `--host/--port/--open-browser`（长驻进程） | v2 由 Rust app server 替代（见 §14）；**PD-D15 决策移除，§18 登记** | `INTENTIONALLY_REMOVED_V2` |
| `desktop` | cli.py:232-246 → `desktop.run_desktop_app` | `--width/--height`（pywebview） | v2 由 Tauri 壳替代（见 §14）；**PD-D15 决策移除，§18 登记** | `INTENTIONALLY_REMOVED_V2` |
| `init-demo` | cli.py:248-253 → `init_demo` service.py:1023 | `--output-dir/--force` | 示例价值由 D10 内嵌示例名单承接；**PD-D15 决策移除，§18 登记** | `INTENTIONALLY_REMOVED_V2` |
| `solve` | cli.py:255-300 → `solve_with_report` service.py:569 | `--students/--layout/--rules/--preset/--history(-dir)/--time-limit/--backend/--candidates(1-20)/--seed/--report` | Rust CLI `solve`（CoreSolveRequest JSON、`--seed`/`--time-limit`/`--output`）；七状态语义冻结；precheck/audit/candidates 子命令补齐诊断与候选集；**41 fixtures 七状态差分 0 mismatch（§19.5）** | `RUST_VERIFIED` |
| `rotation-plan` | cli.py:302-339 → `generate_rotation_plan` service.py:647 | `--periods(1-20)/--label/--name/...` | 由 `project-rotate` CLI + application 轮换路径承担；**rotation 差分 34/34 0 mismatch（§19.14）**；CLI stdout 契约无 golden | `RUST_VERIFIED` |
| `validate` | cli.py:341-367 → `run_validate` service.py:944 | `--strict` | Rust CLI `validate`（core `evaluate_problem` 已验证）；**警告语义补齐（§19.30）**：`--preset/--history/--strict` + preset-context 警告（presets.rs 镜像 14 个 preset requirements，单测锁目录与消息文本）+ group-scope 能力警告；goldens validate/validate-warnings/validate-strict/validate-group-scope + Python exit 语义对照。**剩余边界：`--history-dir` 未镜像；"缺失 student_id" 警告在 CoreSolveRequest 形态不可表示**（key 已在编译期折叠）；按状态字典不得标 verified（§19.33 验收修正） | `RUST_PARTIAL` |
| `export` | cli.py:369-449 → `export` service.py:696 | 8 格式、template、6 隐私开关、page/locale | Rust CLI `export` 支持 svg/html/png/pdf/xlsx/docx/pptx + `--template`；**204 项独立 reader 验证 0 mismatch（§19.11）** | `RUST_VERIFIED` |
| `edit` | cli.py:451-505 → `edit_snapshot` service.py:764 | 9 种操作 kind、`--operations-file/--strict` | Rust CLI `edit`（§19.30）：Python 字符串操作语法（swap/move/batch-move/seat/unseat/lock-seat/unlock-seat/lock-student/unlock-student + 别名）、`--candidate`（candidate set 按 recommended_candidate_id 解析）、`--operations-file/--strict`；artifact 内嵌 students/layout/rules 经 io 共享编译路径过独立 validator；摘要镜像 `_format_edit_summary`；**golden `fixtures/cli-goldens/edit.json` + Python 同输入 exit 对照**。project-edit `--operation` 同步改为字符串语法（与 smoke_cli 一致） | `RUST_VERIFIED` |
| `repair` | cli.py:507-591 → `repair_snapshot` service.py:807 | `--affected-student/--lock-student/--lock-seat/--ignore-saved-locks/...` | Rust CLI `repair`；**空座锁已实现（§19.18）**；**saved-lock 语义对齐（§19.30）**：core `repair_json_with_options(reuse_saved_locks)`（默认 true）+ CLI `--ignore-saved-locks`（repair/project-repair）+ 摘要按有效锁计数（对齐 Python）；goldens repair/repair-saved-locks/repair-ignore-saved-locks（计划差异为自动证据）+ core 单测 | `RUST_VERIFIED` |
| `history-report` | cli.py:593-611 → `run_history_report` service.py:969 | `--history(-dir)` | Rust CLI `history-report`；**§19.30 补齐**：`--history-dir`（默认 glob `*.snapshot.json`）+ `--output`；golden 修正为传 snapshot 文件的真实报告（含 unknown student/seat warnings）+ Python 侧有效参数 exit 对照；**CLI stdout/exit golden（§19.18/§19.30，33/33 0 mismatch）** | `RUST_VERIFIED` |
| `pair-report` | cli.py:613-635 → `run_pair_report` service.py:990 | `--top/--within-distance` | Rust CLI `pair-report`；CLI stdout golden `fixtures/cli-goldens/pair-report.json`（§19.18，21/21 0 mismatch）+ Python exit 语义对照；§19.33 修正 pair-record lookback 并加回归测试，**待新增边界 golden 差分** | `RUST_PARITY_PENDING` |
| `project-init` | cli.py:637-658 → `project_init` service.py:1029 | 默认项目文件 `seattrellis.project.json` | Rust CLI `project-init`（校验 students.csv/layout.json/rules.json 存在后写 workspace）；**CLI stdout golden `fixtures/cli-goldens/project-init.json`（§19.18/§19.30，33/33）** | `RUST_VERIFIED` |
| `project-list` | cli.py:660-673 → `project_bundle.list_recent_projects` | `--root/--limit` | Rust CLI `project-list`（io `list_projects_json`）；**golden `fixtures/cli-goldens/project-list.json`（§19.30，modified_at 时间戳经 harness 归一化）+ Python exit 对照** | `RUST_VERIFIED` |
| `project-privacy` | cli.py:675-684 → `project_bundle.scan_project_privacy` | `--include-outputs` | Rust CLI `project-privacy`（io fail-closed scan）；**§19.30 补齐 `--no-include-outputs`（io `project_privacy_with_options`）**；goldens project-privacy/project-privacy-no-outputs | `RUST_VERIFIED` |
| `project-pack` | cli.py:686-699 → `project_bundle.pack_project` | 输出 `.seattrellis.zip` | Rust CLI `project-pack`（io `pack_project_json`，原子写）；**§19.30 补齐 `--force`（拒绝已存在 bundle，单测）**；golden project-pack + Python restore 同一 Rust bundle 成功（§19.30 bundle 互操作证据） | `RUST_VERIFIED` |
| `project-restore` | cli.py:701-711 → `project_bundle.restore_project_bundle` | `--bundle/--output-dir/--force` | Rust CLI `project-restore`（journaled 目录事务 + `--force`）；**golden `fixtures/cli-goldens/project-restore.json`（§19.30）+ Python 侧用同一 Rust 打包 bundle restore 成功（双向 bundle 互操作）** | `RUST_VERIFIED` |
| `project-info` | cli.py:713-721 → `project_info` service.py:1053 | 无 | Rust CLI `project-info` 存在；**CLI stdout golden `fixtures/cli-goldens/project-info.json`（§19.18/§19.30，33/33）** | `RUST_VERIFIED` |
| `project-validate` | cli.py:723-732 → `project_validate` service.py:1062 | `--strict` | Rust CLI `project-validate` 存在，golden `fixtures/cli-goldens/project-validate.json`（§19.18/§19.30，默认路径，0 mismatch）；**剩余边界：`--strict`/preset/history warning 语义未镜像**（`parse_project_command` 仅 --project/--seed/--format/--output/--snapshot，main.rs:729-786；文件级 `validate --strict` 已对齐，见上行） | `RUST_PARTIAL` |
| `project-solve` | cli.py:734-764 → `project_solve` service.py:1080 | `--candidates/--seed/--report` | Rust CLI `project-solve` 存在，golden `fixtures/cli-goldens/project-solve.json`（§19.18/§19.30，默认路径，0 mismatch）；**剩余边界：`--candidates/--report` 未镜像**（与 project-validate 共享 `parse_project_command` 参数面） | `RUST_PARTIAL` |
| `project-rotate` | cli.py:766-788 → `project_rotate` service.py:1118 | `--periods/--label` | Rust CLI `project-rotate`（§19.12）；**rotation 差分 34/34（§19.14）** | `RUST_VERIFIED` |
| `project-edit` | cli.py:790-842 → `project_edit` service.py:1154 | `--snapshot/--operation/...` | Rust CLI `project-edit`（`--operation`（Python 字符串语法，§19.30 与 smoke_cli 一致）/`--operations-file`/`--strict`，§19.12）；**golden `fixtures/cli-goldens/project-edit.json`（§19.30）** | `RUST_VERIFIED` |
| `project-repair` | cli.py:844-922 → `project_repair` service.py:1181 | `--affected-student/...` | Rust CLI `project-repair`（§19.12 + §19.30 `--ignore-saved-locks`）；**golden `fixtures/cli-goldens/project-repair.json`（§19.30）** | `RUST_VERIFIED` |
| `project-export` | cli.py:924-945 → `project_export` service.py:1225 | `--format/--candidate` | Rust CLI `project-export` 存在，golden `fixtures/cli-goldens/project-export.json`（§19.30：拒绝路径——未先 `project-solve --output` 时 exit 2 并解释生命周期；已保存 snapshot 的导出由 §19.12 生命周期测试覆盖）；**剩余边界：`--candidate` 与其余格式/选项（page/locale/隐私开关）未镜像** | `RUST_PARTIAL` |

### 1.2 子命令组

| 命令 | Python 位置 | 参数要点 | Rust 现状 | 状态 |
|---|---|---|---|---|
| `presets list` | cli.py:128-130 | 无 | 规则模板价值由 D3 句式模板承接；**PD-D15 决策移除，§18 登记** | `INTENTIONALLY_REMOVED_V2` |
| `presets show <preset>` | cli.py:132-136 | 位置参数 | 同上 | `INTENTIONALLY_REMOVED_V2` |
| `presets export <preset>` | cli.py:138-145 | `--output` | 同上 | `INTENTIONALLY_REMOVED_V2` |
| `schema list` | cli.py:147-149 | 无 | Rust CLI `schema-list`；**CLI stdout golden `fixtures/cli-goldens/schema-list.json`（§19.18，21/21 0 mismatch）** | `RUST_VERIFIED` |
| `schema export` | cli.py:151-165 | `--output-dir` | Rust CLI `schema-export`；**CLI stdout golden `fixtures/cli-goldens/schema-export.json`（§19.18，21/21 0 mismatch）** | `RUST_VERIFIED` |
| `schema migrate` | cli.py:167-189 → `schema_migration.migrate_json_file` | `--input/--output/--in-place/--dry-run/--backup` | Rust CLI `schema-migrate`（`seattrellis-schema::migrate_v1_to_v2`，§19.12）；**golden `schema-migrate.json`（§19.18/§19.30，--dry-run）**；`--output`/`--in-place` 路径由 §19.12 生命周期测试覆盖 | `RUST_VERIFIED` |

### 1.3 `seattrellis-desktop`（独立 argparse）

`src/seattrellis/desktop_app.py:12-60`：`--width(1280)/--height(900)/--title/--version`，exit code 0 成功 / 2 解析错误。与 `desktop` 命令同一 pywebview 桌面壳（pywebview 是 v2 final 移除红线）→ **`INTENTIONALLY_REMOVED_V2`（§18 登记扩展，PD-D15 移除范围；v2 由 Tauri 壳替代）**。

### 1.4 错误/退出码契约（Python）

| 行为 | 约定 | 出处 |
|---|---|---|
| 业务错误 | stderr 打印 `Error: ...`，`typer.Exit(1)` | cli.py:993-998 |
| `--version/-V` | 打印 `seattrellis {__version__}`，退出 0 | cli.py:109-124 |
| 无参数 | 显示帮助（退出 0） | cli.py:95 |
| 参数解析错误 | 退出 2（Click/Typer 惯例） | — |
| typer 未安装 | `SystemExit` 带提示 | cli.py:984-990 |

Rust CLI 对照此契约（v2 冻结退出码 0/2/3/4/5/70/130，`exit_code_for` main.rs:1458-1469，业务错误 stderr 打印 `error: ...`、解析错误 2、无参数 help 0、`--version` 0）；**§19.30 `--cli-golden` 33 命令逐条记录 exit 码（含 1/2 非零路径：audit/project-export/schema-migrate/validate-strict）+ Python 侧 0 vs 非零 exit 语义对照，33/33 0 mismatch → `RUST_VERIFIED`**。剩余边界：golden 为代表性命令集（§19.18 已登记 CLI 参数组合全量枚举未做），非全部错误路径逐条对照。

### 1.5 验收参照

`scripts/smoke_cli.py`（`_commands()` 于 :123-581）是 CLI 面最完整的验收清单：覆盖 `--help`、init-demo、doctor、presets、schema、validate、solve、project-*、edit、repair、history/pair-report、export 各格式。任何 Rust CLI 迁移以 `python -m seattrellis.cli` 对照逐条比对；该脚本支持 `--command` 替换被测可执行文件（smoke_cli.py:36-40）。

---

## 2. service / application 公开用例

Python 服务层（`src/seattrellis/service.py` + `src/seattrellis/application/`）按领域分组。Rust 侧实现分布在 `crates/seattrellis-application`、`seattrellis-io`、`seattrellis-server`、`crates/seattrellis-core` 与 `crates/seattrellis-cli`。

### 2.1 solve（求解）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_solve`（内存模型求解，候选 1–20 + recommended） | service.py:107；service_types.py:181/:199 | app `POST /classes/generate`（server.rs:430，room_templates.rs:317/331 + goal_rules.rs:32 + editing.rs:868）；core `solve_problem`（lib.rs:530） | `RUST_VERIFIED`（41 fixtures 七状态差分 + candidates 15/15，§19.5/§19.6） |
| `solve` / `solve_with_report`（文件级 + 报告） | service.py:537/:569 | Rust CLI `solve --problem <CoreSolveRequest.json>`；**41 fixtures 七状态差分 0 mismatch（§19.5）覆盖文件级工作流 + golden `fixtures/cli-goldens/solve.json`（§19.30）**；Python `--report` 由 `audit`/`score`/`precheck` 子命令承担（§19.8/§19.16） | `RUST_VERIFIED` |
| `project_solve`（项目级） | service.py:1080 | Rust CLI `project-solve` 存在，golden `project-solve.json`（默认路径，§19.30）；**剩余边界：`--candidates/--report` 未镜像** | `RUST_PARTIAL` |
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
| `run_validate`（文件级，含 preset/history 警告） | service.py:944 | Rust CLI `validate` 对齐（§19.30）：`--preset/--history/--strict` + preset-context 警告（`presets.rs` 镜像 14 个 preset requirements，单测锁目录与消息文本）+ group-scope 能力警告；goldens validate/validate-warnings/validate-strict/validate-group-scope + Python exit 对照。**剩余边界同 §1.1 `validate` 行：`--history-dir` 未镜像、"缺 student_id" 警告在 CoreSolveRequest 形态不可表示**（§19.33 验收修正） | `RUST_PARTIAL` |
| `project_validate` | service.py:1062 | Rust CLI `project-validate` 存在，golden `project-validate.json`（默认路径，§19.30）；**剩余边界：`--strict`/preset/history warning 语义未镜像** | `RUST_PARTIAL` |
| `list_teacher_goals` / `get_teacher_goal` / `resolve_teacher_goal` | application/teacher_goals.py:98/:104/:117 | app `goal_rules.rs` 仍仅 4 个 goal（`GOAL_IDS` goal_rules.rs:14-19），Python 15 个 preset 中 11 个无对应（§19.15 capability 对账确认 catalog 与 GOAL_IDS 自洽，但不构成 Python parity）；goal JSON 仍不含 hard/groups（goal_rules.rs:6-9 有意省略） | `RUST_PARTIAL` |

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
| `compute_edit` / `edit_snapshot` / `project_edit` | service.py:294/:764/:1154 | app editing 协议端点（editing.rs:856/:884）+ 文件级 CLI `edit`（§19.30：Python 字符串操作语法 9 种 kind + 别名、`--candidate`/`--operations-file`/`--strict`，摘要镜像 `_format_edit_summary`）与 `project-edit`（§19.12）；goldens `fixtures/cli-goldens/edit.json`/`project-edit.json`（§19.30） | `RUST_VERIFIED` |
| `compute_repair` / `repair_snapshot` / `project_repair` | service.py:336/:807/:1181 | core `repair_json`/`repair_json_with_options(reuse_saved_locks)` + CLI `repair`/`project-repair`（§19.18 空座锁 + saved-lock 语义；§19.30 `--ignore-saved-locks` + 摘要按有效锁计数）；goldens repair/repair-saved-locks/repair-ignore-saved-locks/project-repair（§19.30） | `RUST_VERIFIED` |
| `EditorDraftStore` / `LayoutDraftStore` / `RosterDraftStore` | api/drafts.py:45、api/layouts.py:34、api/rosters.py:48 | app editing.rs/layouts.rs/roster.rs 对应 store | `RUST_PARITY_PENDING` |

### 2.6 history / pair history（见 §9 详细条目）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `compute_history_report` / `run_history_report` | service.py:473/:969 | core `history_report_json` + Rust CLI `history-report`（§19.1 warning/student_count/lookback 结构对齐 Python；§19.30 `--history-dir` 默认 glob + `--output`）；golden `fixtures/cli-goldens/history-report.json`（§19.30）+ Python 有效参数 exit 对照 | `RUST_VERIFIED` |
| `compute_pair_report` / `run_pair_report` | service.py:480/:990 | core `pair_report_json` + Rust CLI `pair-report`（§19.1 结构对齐、§19.14 rotation 差分 relation_totals 逐键相等）；golden `fixtures/cli-goldens/pair-report.json`（§19.18，21/21 + §19.30）。§19.33 已按 Python `StudentPairHistory.recent_occurrence_count` 修正 pair-record lookback，并补“pair 缺席最近全局窗口”回归测试（pair_report_lookback.rs）+ 6 快照 live 差分（pair totals 与 Python 逐项相等）；41 fixtures/33 goldens 0 mismatch | `RUST_VERIFIED` |
| `compute_project_info` / `project_info` | service.py:499/:1053 | Rust CLI `project-info`；**golden `fixtures/cli-goldens/project-info.json`（§19.18/§19.30，33/33 0 mismatch）** | `RUST_VERIFIED` |

### 2.7 project（项目工作区）

| 用例 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `init_demo`（拆行：项目工作区） | service.py:1023 | **PD-D15 决策移除，§18 登记**（示例价值由 D10 内嵌示例名单承接） | `INTENTIONALLY_REMOVED_V2` |
| `project_init`（拆行） | service.py:1029 | Rust CLI `project-init`（校验 students.csv/layout.json/rules.json 后写 workspace）；**golden `fixtures/cli-goldens/project-init.json`（§19.18/§19.30，33/33）** | `RUST_VERIFIED` |
| `project_info`（拆行） | service.py:1053 | Rust CLI `project-info`；**golden `fixtures/cli-goldens/project-info.json`（§19.18/§19.30，33/33）** | `RUST_VERIFIED` |
| `project_validate`（拆行） | service.py:1062 | Rust CLI `project-validate`，golden `project-validate.json`（默认路径，§19.30）；**剩余边界：`--strict`/warning 语义未镜像** | `RUST_PARTIAL` |
| `run_doctor`（环境诊断） | service.py:861 | Rust CLI `doctor`（binary/core version、temp dir 可写）；**golden `fixtures/cli-goldens/doctor.json`（§19.30）+ Python exit 语义对照** | `RUST_VERIFIED` |
| project 历史/产物浏览 | handlers.py:228/:259 | app projects.rs:332/:481 + server.rs:470-475 | `RUST_PARITY_PENDING` |
| 产物对比（artifacts/compare） | handlers.py:298 | io `compare_artifacts_json`（projects.rs:1786-1877：left/right 摘要 + diff 字段）+ server 路由（server.rs:1237，§19.1 OpenAPI 契约生成 + 契约测试）；**剩余边界：artifact 种类与 diff 字段无 Python golden 等价证据** | `RUST_PARTIAL` |
| 产物恢复（artifacts/restore） | handlers.py:380 | io `restore_artifact_json`（rotation 拒绝、`restored_from`/`restored_at` 元数据、输出新 snapshot，projects.rs:1882+）+ **§19.13 写前故障注入 rollback 已验收**；**剩余边界：revision/provenance 全契约无 Python golden** | `RUST_PARTIAL` |
| 隐私扫描 / 打包 / 恢复 | handlers.py:1432/:1474/:1496 | server 路由（server.rs:1287-1340）复用 io 同一实现；**§19.30 goldens project-privacy/project-privacy-no-outputs/project-pack/project-restore + Python 用同一 Rust bundle restore 成功（双向互操作）+ §19.13 atomic restore 故障注入验收** | `RUST_VERIFIED` |

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
| `suggest_roster_mapping`（含表头/身份列启发式） | application/roster_mapping.py:238 | Rust `suggest_mapping`/`looks_like_identifier`/`looks_like_person_name` 实现存在（roster.rs:452/:675/:689），表头别名经 `roster_alias_mirror.rs` 测试逐项锁死（§19.18）；**§19.36 roster-mapping parity corpus（10 case）Rust 与 Python oracle 差分全等** | `RUST_VERIFIED` |
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

### 3.2 Post-baseline 新增路径（3 个 → 1 个 `RUST_VERIFIED`、2 个 `RUST_PARTIAL`）

| # | Method + Path | 前端定义 | Rust 路径 | 已知未验收范围 | 状态 |
|---|---|---|---|---|---|
| 29 | POST `/api/v1/classes/rotation` | client.ts:522 | `seattrellis-server` → `seattrellis-application::rotation` | **已关闭**：逐期 validator/诚实领域结果（§19.1/§19.7 rotation_gate）、history/fairness 与 Python golden（§19.14 rotation 差分 34/34）；plan 文档 `schema_version` 差异登记于 §4.1 rotation-plan 行（端点行为 parity 不受影响） | `RUST_VERIFIED` |
| 30 | POST `/api/v1/projects/artifacts/compare` | client.ts:154 | `seattrellis-server` → `seattrellis-io::projects` | §19.1 OpenAPI 契约生成 + server 契约测试已过；**artifact 种类、diff 字段、隐私和 error contract 仍无 Python golden** | `RUST_PARTIAL` |
| 31 | POST `/api/v1/projects/artifacts/restore` | client.ts:169 | `seattrellis-server` → `seattrellis-io::projects` | §19.13 写前故障注入 rollback 已验收；**revision/provenance 语义仍无 Python golden** | `RUST_PARTIAL` |

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
| `project.schema.json`（82 行） | `schema_version: int = 1`；`kind=seattrellis_project`；students/layout/rules/history_dir/outputs_dir/default_candidates/… | `models/project.py:15` | schema 层全字段 typed DTO 已存在（`dto::project::SeatTrellisProjectArtifact`，field-for-field + deny_unknown_fields）；但 io 运行时 `ProjectFile` 仍为**部分字段**结构体（projects.rs:179-190，仅 name/students/layout/rules/history_dir/outputs_dir），且 v1→v2 migration 对 project kind 仍明确报错（migration.rs:285-290） | `RUST_PARTIAL` |
| `ruleset.schema.json`（470 行） | `schema_version: int = 1`；hard/soft/groups + 15+ 规则 definitions | `models/rules.py`；`RULESET_SCHEMA_VERSION` schema.py:15 | core `models.rs` `RuleSet`/`SoftRules`/各规则 struct 全量镜像（models.rs:253-508） | `RUST_PARITY_PENDING` |
| `seating-snapshot.schema.json`（890 行） | `schema_version: "1.0"`；students/layout/rules/assignments/solver_status/objective_value/metrics | `models/snapshot.py:21` | `dto::snapshot::SeatingSnapshotArtifact` 全字段镜像（deny_unknown_fields，含 schema_version/created_at/seed/metadata）；**candidate-set oracle golden 内嵌 snapshot 全量解析 0 失败（candidate_dto_fixtures.rs，§19.18）**；core models.rs 仍为求解子集（求解专用，非 schema 缺口） | `RUST_VERIFIED` |
| `candidate-set.schema.json`（1138 行） | `schema_version: "0.2.2"`；candidates/recommended_candidate_id/warnings + PlanScore | `models/candidate.py:115` | typed DTO `dto::candidate_set`（deny_unknown_fields）；**oracle golden 全量解析 0 失败（candidate_dto_fixtures.rs，§19.18）** | `RUST_VERIFIED` |
| `rotation-plan.schema.json`（983 行） | `schema_version: "1.0"`；periods/name/fairness_summary/pair_repeat_summary | `models/rotation.py:36` | 完整写入与持久化路径已实现；§19.34 将 Rust plan `schema_version` 从错误的 "0.2.2" 修正为 oracle 冻结值 "1.0"，差分 harness 新增 kind/version 强制比较，rotation 34/34、0 mismatch，非 ignored 契约测试锁定产物值 | `RUST_VERIFIED` |
| `editor-command.schema.json`（436 行） | `protocol_version: "1.0"`；command_id/draft_id/base_revision/action/operations（9 种） | `editing_protocol.py:152-231` | editing.rs 全量端口（`EDITOR_PROTOCOL_VERSION` editing.rs:41） | `RUST_PARITY_PENDING` |
| `editor-state.schema.json`（207 行） | `protocol_version: "1.0"`；draft_id/revision/candidate_id/undo_depth/redo_depth/students/seats/hard_constraints | `editing_protocol.py:257-267` | editing.rs `EditorState`（:616-627） | `RUST_PARITY_PENDING` |
| `student.schema.json`（116 行） | 无版本字段；student_id/name/gender/height_cm/score/vision/notes/tags/needs/attributes | `models/student.py` | `dto::student_roster::RosterStudent` 全 10 字段镜像（deny_unknown_fields）；**oracle golden 内嵌 students（含 gender/notes/attributes）全量解析 0 失败（candidate_dto_fixtures.rs，§19.18）**；core `models.rs` `Student` 无 gender 为求解子集有意设计（schema 层已镜像；§8 注：gender 双侧均无规则/目标） | `RUST_VERIFIED` |
| `classroom-layout.schema.json`（215 行） | 无版本字段；layout_id/seats/adjacency | `models/layout.py` | `dto::classroom_layout::ClassroomLayout` 全镜像（含 SeatNode zone/group_id/near_*/tags/attributes + metadata）；core `Seat` 已含 zone/group_id/near_*（models.rs:86-96）；**oracle golden 内嵌 layout 全量解析 0 失败（candidate_dto_fixtures.rs，§19.18）** | `RUST_VERIFIED` |
| `plan-comparison-report.schema.json`（217 行） | `schema_version: "0.2.2"`；candidates/PlanComparisonEntry/explanations | `models/candidate.py` 相关 | typed DTO `dto::plan_comparison`（交叉字段不变量校验）；oracle schema 形状断言（§19.18） | `RUST_VERIFIED` |

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
- `crates/seattrellis-core` 的 serde 结构是**求解专用协议**（`CoreSolveRequest` lib.rs:397-433），不是 seat 数据 schema。

---

## 5. roster import / mapping / update

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| CSV/XLSX 上传解析（20MB 上限、413/503） | api/rosters.py:48；DEFAULT_MAX_ROSTER_FILE_BYTES | roster.rs:976（server.rs:434） | `RUST_PARITY_PENDING` |
| 名单摘要（总人数/性别/高度等） | application/roster_import.py:61 | 对应响应字段 | `RUST_PARITY_PENDING` |
| 自动推断列映射（`_looks_like_identifier`/`_looks_like_person_name` 启发式） | application/roster_mapping.py:214-238 | Rust 同名启发式实现存在（roster.rs:675/:689，`suggest_mapping` roster.rs:452）；**§19.36 roster-mapping parity corpus（10 case）Rust 与 Python oracle 差分全等**（表头别名已由 roster_alias_mirror.rs 逐项锁死，§19.18） | `RUST_VERIFIED` |
| 映射校验/模板生成/模板应用 | application/roster_mapping.py:351/:394/:407 | 映射校验存在（`mapping_issues` roster.rs:129-186）；**模板生成/模板应用无对应（roster.rs 无 template 实现）** | `RUST_PARTIAL` |
| `roster_fingerprint`（防无变更提交） | application/roster_update.py:119 | **无对应**（roster.rs/server.rs 无 fingerprint 实现；preview 端点未做"无变更拒绝"） | `RUST_PARTIAL` |
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
| 构建：`load_history_snapshots`/`build_seat_history`/`build_pair_history` | history.py:45/:65/:135 | core reports 路径（`history_report_json`/`pair_report_json` reports.rs:22/:240）+ CLI `--history-dir` 默认 glob 加载（§19.30）；goldens history-report/pair-report（§19.18/§19.30）；relation_totals 构建语义经 rotation 差分 34/34 对照（§19.14） | `RUST_VERIFIED` |
| 关系检测 `detect_neighbor_relation_types` / `student_pair_key` | history.py:237/:276 | cost.rs:329-386/:448（求解用） | `RUST_PARITY_PENDING` |
| 成本：`avoid_recent_neighbors_cost`/`fair_rotation_cost`/`classify_seat_position` | history.py:348/:449/:409 | cost.rs:281/:124/:185 | `RUST_PARITY_PENDING` |
| 报告：`build_fairness_report`、`compute_history_report`、`compute_pair_report` | history.py:381；service.py:473/:480 | core `history_report_json`/`pair_report_json`（§19.1 结构对齐：student_count/warnings/lookback、教师侧标识符契约 §19.3.1）；已有 goldens + relation_totals rotation 差分；§19.33 修正 `recent_occurrences` 的 pair-record lookback（pair_report_lookback.rs 边界回归 + 6 快照 live 差分，pair totals 与 Python 逐项相等；41 fixtures/33 goldens 0 mismatch） | `RUST_VERIFIED` |
| 报告：`run_history_report`/`run_pair_report`（CLI） | service.py:969/:990 | Rust CLI 对齐（§19.30）：`--history-dir`/`--output`、goldens + Python 有效参数 exit 对照；pair-record lookback 边界差分 §19.33 完成 | `RUST_VERIFIED` |

---

## 10. candidate generation / comparison

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| 候选生成（1–20，seed 支持） | service.py:107-175；models.py:185（API 限制 ge=1, le=20） | core `generate_candidates_json`：1..=20 显式校验、seed 派生 `base+attempt_index` 对齐 Python、exact-assignment exclusion 在搜索内部、每候选独立 validator；**candidates 差分 20/40/50/60/80 × 1/5/20 = 15 combos 0 mismatch（§19.6）** | `RUST_VERIFIED` |
| recommended 选择 | candidates.py:137（`refresh_recommendation`：max total_score） | Rust 推荐 = max plan_score total（镜像 Python，§19.8）；candidates 差分覆盖推荐一致性 | `RUST_VERIFIED` |
| PlanScore / ScoreBreakdown（7 维度 + rule_scores + hard_constraint_summary） | scoring.py:280-636 | Rust `score_assignment_json` 全字段镜像；**scoring 差分 34/34** | `RUST_VERIFIED` |
| `diversity_score`（候选间换座比例） | scoring.py:82-130 | candidates 输出每候选 `plan_score.diversity`（对其他候选平均 assignment 距离，镜像 `apply_diversity_scores`，§19.8）；候选集级 audit 未做 | `RUST_PARITY_PENDING` |
| `stability_score`（与最近历史同座比例） | scoring.py:480-502 | `score_assignment_json` 已镜像（固定 assignment 路径 scoring 差分覆盖）；候选集 stability 维度因 core history 模型不含原始 seat_id 而未激活 | `RUST_PARITY_PENDING` |
| 计划比较报告（plan-comparison-report） | candidate_report.py:77；schema 0.2.2 | 无报告**生成/渲染**对应；typed DTO 解析 golden 已 RUST_VERIFIED（§4.1/§19.18，`dto::plan_comparison` + oracle schema 形状断言）；v2 候选比较由 B5 候选面板（/audit 端点 + plan_score，§19.20）以 UI 形态呈现——可打印报告为选项级，无移除决策 | `PYTHON_ONLY` |
| `hard_constraint_summary`（完整性+硬规则复核） | scoring.py:99-112 | lib.rs:107-166（`evaluate_problem` 等价）；scoring 差分逐字段比较 hard summary（§19.8） | `RUST_VERIFIED` |

---

## 11. repair / editing

| 条目 | Python 位置 | Rust 位置 | 状态 |
|---|---|---|---|
| Editing 协议端点（GET draft / POST commands / DELETE） | http.py:721/:729/:741 | editing.rs:856/:884；server.rs:446-451 | `RUST_PARITY_PENDING` |
| `EditorDraftStore`（create/state/snapshot/dispatch/delete/clear） | api/drafts.py:45-190 | editing.rs `EditorDraftStore`（:831） | `RUST_PARITY_PENDING` |
| 文件级编辑 `edit_snapshot` / `project_edit`（CLI） | service.py:764/:1154 | Rust CLI `edit`（§19.30：Python 字符串操作语法 9 种 kind + 别名、`--candidate`/`--operations-file`/`--strict`）+ `project-edit`（§19.12）；goldens `fixtures/cli-goldens/edit.json`/`project-edit.json`（§19.30） | `RUST_VERIFIED` |
| 受约束重解 `compute_repair` / `repair_snapshot` / `project_repair`（锁 + affected + 变更轨迹） | service.py:336/:807/:1181 | core `repair_json`/`repair_json_with_options` + CLI `repair`/`project-repair`（§19.18 空座锁 + saved locks；§19.30 `--ignore-saved-locks`）；goldens repair/repair-saved-locks/repair-ignore-saved-locks/project-repair（§19.30） | `RUST_VERIFIED` |
| 锁状态 `lock_state_from_snapshot` / `EditingSession` | editing.py:86-99 | editing.rs 对应 | `RUST_PARITY_PENDING` |

---

## 12. export formats / options / privacy modes

### 12.1 格式（Python 9 种 vs Rust 4 种）

| 格式 | Python 入口 | 选项 | Rust 现状 | 状态 |
|---|---|---|---|---|
| SVG | exporters/svg.py:23 | template/privacy/candidate/locale（固定 16:9，不接受 page） | render.rs:215（export.rs:69-74）；**exports 差分 + E2E 下载校验（§19.11）** | `RUST_VERIFIED` |
| HTML | exporters/html.py:9 | 无选项 | render.rs:310；exports 差分独立 reader 校验 | `RUST_VERIFIED` |
| print-html | exporters/print_html.py:88 | page/locale（A4 打印模板） | **独立版式已实现（§19.19）**：横版默认、一页最大化、字号按最长姓名、结构标注、可配置项；5 结构测试；**exports 差分独立 reader 34 case 结构/姓名/标注/页脚/无明细泄漏（§19.31）** | `RUST_PARITY_PENDING` |
| PNG | exporters/png.py:9 | 无选项 | **姓名渲染已实现（§19.19）**：fontdue 光栅化、字号自适应、无字体优雅降级；像素级测试 | `RUST_PARITY_PENDING` |
| PDF | exporters/pdf.py:69 | template/privacy/page/orientation/scale/paper_size/margin_mm/locale | **144 DPI 光栅页（§19.26，Image XObject + Flate/RunLength，跨查看器可读）**；paper_size/margin_mm/orientation/scale 支持；**exports 差分独立 reader 34 case（pypdf：A4-ish 页面 + Image XObject 存在，§19.31）** | `RUST_PARITY_PENDING` |
| DOCX | exporters/docx_export.py:26 | page 生效 | office.rs `render_docx`（标题+边框座位表格）；**python-docx 独立重开 204/204（§19.11）** | `RUST_VERIFIED` |
| PPTX | exporters/pptx.py:22 | 单页 16:9 可编辑形状 | office.rs `render_pptx`（screen16x9 + roundRect 座位形状）；**python-pptx 独立重开 204/204（§19.11）** | `RUST_VERIFIED` |
| Excel | exporters/excel.py:9 | Seating+Assignments 两 sheet | office.rs `render_xlsx`（两 sheet、行号连续）；**openpyxl 独立重开 204/204（§19.11）** | `RUST_VERIFIED` |
| 候选集比较报告 | exporters/candidate_report.py:77 | page/locale（不含学生字段） | 无报告**渲染**对应；DTO/解析已 RUST_VERIFIED（§4.1/§19.18）；v2 由 B5 候选面板 UI 承担（§19.20）——选项级，无移除决策 | `PYTHON_ONLY` |

导出白名单 `export_extension`（service_types.py:375-384）：excel/xlsx、html、png、pdf、docx、svg、pptx、print-html。格式选项拒绝规则（exporters/__init__.py:217-231）：基础 HTML/Excel/PNG 不接受 template/privacy/page/locale；SVG/PPTX 不接受 page；report 模板必须带 candidate；candidate_scope="all" 需候选集。

### 12.2 Privacy modes

| 条目 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `PrivacyOptions`（6 开关：hide_scores/hide_notes/hide_special_needs/anonymize/show_height/show_vision） | service_types.py:43-78 | `anonymize` 及 `show_height/show_vision` 有渲染路径；hide_* 无渲染内容可隐藏（renderer 不画分数/备注/需求）；**public 模板三格式独立 reader 真实学生名零泄漏 204/204（§19.11）** | `RUST_VERIFIED` |
| 模板默认：public（隐藏 3 项+身高视力）、teacher（全开放）、report（显分数隐其余） | `PrivacyOptions.for_template()` service_types.py:54-78 | export.rs:113-131；**§19.27 缺省模板统一 teacher（真实姓名默认保留，public 强制匿名）**；report 与 teacher 渲染仍无差异（renderer 不画分数/备注，report 模板的"显分数"无法表达） | `RUST_PARTIAL` |
| 统一过滤：匿名编号「学生 01」、逐字段隐藏、教师版元数据剔除 | exporters/presentation.py:34-118 | `anonymize_grid` + `filter_detail_grid`（export.rs）；public 零泄漏 204/204（§19.11） | `RUST_VERIFIED` |
| public 版「🔒 班级公示版」徽标 | print_html.py:235-248 | **无对应**（print_html.rs 无该徽标；print-html 版式含页脚溯源 seed 标注，§19.19 A2） | `RUST_PARTIAL` |
| 项目隐私扫描（`_SENSITIVE_KEYS`） | project_bundle.py:27/:137 | projects.rs:706（scan） | `RUST_PARITY_PENDING` |

### 12.3 页面选项

| 条目 | Python | Rust | 状态 |
|---|---|---|---|
| orientation（portrait/landscape） | PDF/DOCX/print-html | PDF/DOCX/print-html 全部生效（§19.19）；print-html 默认横版 | `RUST_PARITY_PENDING` |
| page_scale | 同上 | PDF + print-html（clamp 0.5–2.0） | `RUST_PARITY_PENDING` |
| margin_mm / paper_size | PDF/DOCX/print-html | PDF + print-html（a4/a3/letter；margin clamp 5–25mm） | `RUST_PARITY_PENDING` |
| locale（zh/en） | 全部 | 仅影响匿名占位符（export.rs:441）与 print-html `lang` 属性（print_html.rs:119）；报告/版式文案未本地化 | `RUST_PARTIAL` |

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
| 项目产物对比/恢复（artifacts compare/restore，含 revision 链） | handlers.py:298/:380 | §19.13 写前故障注入 rollback 已验收（restore_artifact_json）；§19.1 OpenAPI 契约生成 + server 契约测试；**剩余边界：artifact/revision/provenance 语义仍无 Python golden 等价证据** | `RUST_PARTIAL` |
| 迁移 CLI（schema migrate） | cli.py:167-189 | Rust CLI `schema-migrate`（`migrate_v1_to_v2`，§19.12：StudentRoster/ClassroomLayout 类型化迁移，其余 kind 明确报错）；**golden `fixtures/cli-goldens/schema-migrate.json`（§19.18/§19.30）+ `--output/--in-place/--dry-run` 由 §19.12 生命周期测试覆盖** | `RUST_VERIFIED` |

---

## 14. desktop native file workflows

| 条目 | Python 位置 | Rust 现状 | 状态 |
|---|---|---|---|
| `seattrellis workspace`（浏览器工作台启动器） | cli.py:195-230；workspace_server.py | app server（main.rs）等价启动器存在（`--port/--open-browser/--version`，main.rs:13/:36-60） | `RUST_PARITY_PENDING` |
| `seattrellis desktop`（pywebview 桌面壳） | cli.py:232-246；desktop.py:355 | **PD-D15 决策移除，§18 登记**（由 Tauri 2 壳替代；pywebview 是 v2 final 移除红线） | `INTENTIONALLY_REMOVED_V2` |
| `seattrellis-desktop`（独立 argparse CLI） | desktop_app.py:12-60 | 同一 pywebview 桌面壳的独立入口；**§18 登记扩展（PD-D15 移除范围，理由/迁移/影响同 `desktop` 行）** | `INTENTIONALLY_REMOVED_V2` |
| Tauri 壳能力 | — | **§19.22 C1 已补**：2 个 `#[tauri::command]`（`read_user_file`/`write_user_file`，8MB 上限）+ `tauri-plugin-dialog`（dialog:default）+ 服务端 `POST /api/v1/files/read`（可信根内相对路径，绝对/`..`/NUL/反斜杠拒绝，canonical 防 symlink）+ `GET /api/v1/files/root`；浏览器 E2E 覆盖路径读取闭环与绝对路径拒绝（§19.22）。托盘/菜单非 v2 必须项 | `RUST_PARITY_PENDING` |
| 原生文件打开/保存对话框（桌面工作流核心体验） | desktop.py（pywebview 文件对话框） | **PD-D14 三入口融合已实现（§19.22 C1）**：① Tauri 系统对话框 ② 拖拽 ③ 可信根内相对路径输入（客户端+服务端双重校验）；server 86 测试（+7 files/read/root 契约）+ E2E；浏览器保留 input[type=file] 兜底 | `RUST_PARITY_PENDING` |
| Rust app server 整体（27 路由） | — | server.rs:427-524 | `RUST_PARITY_PENDING` |
| 前端静态资源内嵌 | — | embedded_web.rs:7/:13 | `RUST_PARITY_PENDING`（非 parity 项，仅记录） |

---

## 15. 汇总表

| 领域 | 条目数 | PYTHON_ONLY | RUST_PARTIAL | RUST_PARITY_PENDING | RUST_VERIFIED | INTENTIONALLY_REMOVED_V2 |
|---|---|---|---|---|---|---|
| §1 CLI（30 命令 + 契约） | 31 | 0 | 4 | 1 | 20 | 6 |
| §2 service/application | 41 | 0 | 7 | 12 | 21 | 1 |
| §3 React `/api/v1/*`（31 调用） | 31 | 0 | 2 | 28 | 1 | 0 |
| §4 Schema（10 文件 + 协议机制） | 16 | 0 | 1 | 9 | 6 | 0 |
| §5 roster | 8 | 0 | 1 | 5 | 2 | 0 |
| §6 layout editor | 5 | 0 | 0 | 5 | 0 | 0 |
| §7 hard rules | 5 | 0 | 0 | 5 | 0 | 0 |
| §8 soft objectives | 10 | 0 | 1 | 9 | 0 | 0 |
| §9 history/pair history | 7 | 0 | 0 | 4 | 3 | 0 |
| §10 candidates | 7 | 1 | 0 | 2 | 4 | 0 |
| §11 repair/editing | 5 | 0 | 0 | 3 | 2 | 0 |
| §12 export（格式/隐私/页面） | 18 | 1 | 3 | 7 | 7 | 0 |
| §13 migration/backup/restore | 9 | 0 | 1 | 7 | 1 | 0 |
| §14 desktop workflows | 7 | 0 | 0 | 5 | 0 | 2 |
| **合计** | **200** | **2** | **21** | **102** | **66** | **9** |

计数口径：§2/§4–§14 逐行统计明细表中的五种状态；§1 为 30 条命令行再加 1 条整体 error/exit-code 契约（§19.32 升级 `RUST_VERIFIED`，33 golden 逐命令 exit 码 + Python 0/非零语义对照）；§3 为基线 28 条 `RUST_PARITY_PENDING` 加 post-baseline 3 条（1 个 `RUST_VERIFIED` + 2 个 `RUST_PARTIAL`）；§2 因 `init_demo/project_init`、`project_info/project_validate` 两行拆分（`INTENTIONALLY_REMOVED_V2` + `RUST_VERIFIED` / `RUST_VERIFIED` + `RUST_PARTIAL`）由 39 行变 41 行；§14 `seattrellis-desktop` 与 `desktop` 并入 PD-D15 移除登记。校验时只计数 §1–§14 明细表，不计本汇总表和文字中出现的状态名。

---

## 16. 差距总览（迁移工作清单来源）

> **注**：本节为 2026-08-09 审计时的差距清单快照，未随 §19 各轮整改回写；
> 条目状态一律以 §19（尤其 §19.30/§19.31/§19.32）的最新记录为准。已被
> 关闭的条目见 §19.32「已升级行」清单（A.1 rotation、B.4/B.5 报告、
> D.10/D.11 CLI 面、D.16 schema-migrate 等）。

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

## 18. INTENTIONALLY_REMOVED_V2 登记

> 登记日期：2026-08-10（批 2 决策 `PD-D15-LEGACYCMDS`，见
> `docs/product-decisions/2026-08-10-batch2-export-wrapup.md`）。变更随
> 引入该决策记录的 commit 提交（`git log -- docs/product-decisions/2026-08-10-batch2-export-wrapup.md`）。

| 条目 | 理由 | 迁移方案 | 用户影响 | 状态 |
|---|---|---|---|---|
| `init-demo`（CLI） | 示例价值由 D10 内嵌示例名单承接（内嵌资产 + 隔离标记，优于命令行生成）；v2 以 UI 引导形态呈现 | CLI 用户经「临时工作台 + 示例名单」达到同等效果；示例资产不依赖命令 | 命令行一键示例入口消失（低频） | `INTENTIONALLY_REMOVED_V2` |
| `presets`（list/show/export，CLI） | 规则模板价值由 D3 句式模板承接；预设本质是规则 JSON，用户可直接维护规则文件 | UI 用户走 D3 句式模板；CLI 自动化用户直接用规则 JSON（现有 solve 输入契约） | CLI 预设模板命令消失；规则模板以 UI 句式模板形式保留 | `INTENTIONALLY_REMOVED_V2` |
| `workspace`（CLI） | 由 Rust app server（loopback HTTP）替代，架构已完全不同（§14） | 启动方式改为 Rust app server / Tauri 桌面应用 | 无（功能由等价启动器承接） | `INTENTIONALLY_REMOVED_V2` |
| `desktop`（CLI） | 由 Tauri 2 壳替代；pywebview 是 v2 final 移除红线 | 桌面入口改为 Tauri 应用 | 无 | `INTENTIONALLY_REMOVED_V2` |
| `seattrellis-desktop`（独立 argparse CLI，desktop_app.py:12-60） | 与 `desktop` 命令同一 pywebview 桌面壳的独立启动入口，同属 pywebview 移除红线（PD-D15 理由覆盖）；v2 由 Tauri 壳替代 | 桌面入口改为 Tauri 应用 | 无（`seattrellis-desktop` 为 pywebview 时代独立入口，v2 无对应） | `INTENTIONALLY_REMOVED_V2` |

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
3. **`recent_occurrences` 窗口语义（§19.33 实现修正，golden 待验）**：原 Rust 取全局最后 4 个 snapshot，与 Python 按 pair 自身最后 lookback 条记录不同。现已改为 pair-record lookback，并用“pair 仅在早期出现、最近 4 个全局 snapshot 缺席”回归测试锁定数值；在新边界 Python↔Rust golden 差分登记前，相关报告条目保持 `RUST_PARITY_PENDING`。
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
   `crates/seattrellis-core/tests/candidates_gate.rs`，覆盖
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

### 19.16 2026-08-10：M3 剩余证据收口（§6.5/§6.6/§17.3）

1. **Audit/explanation UI 消费字段（§6.5/§6.7 item 2）**：`audit_report_json`
   新增四个可消费分组（向后兼容，新增键）：
   - `hard_constraint_summary`：`all_satisfied` / `checked_rule_count` /
     `violation_count` / `witnesses`（UI 无需重推导规则即可渲染总览；
     完整合法 assignment 无违规，witness 列表恒空，字段为部分/非法
     plan 审计预留）；
   - `missing_data`：缺 score/height/vision/needs 的学生计数（§6.5
     "缺失数据说明"）；
   - `history`：`snapshot_count` / `has_history`（§6.5 "历史影响"）；
   - `suggested_actions`：本地化 `message_key` + `suggested_action` +
     `args`（可操作建议：history_recommended / missing_height /
     missing_vision / missing_score / ready，按启用规则与缺失数据
     派生）。新增单测
     `audit_report_carries_ui_consumption_fields`。
2. **性能回归门槛（§6.6 item 7）**：新增 `scripts/bench_solver.py`，
   对 planted-feasible 实例（n=40/50/60/80，与 Rust long-run gate 同构）
   测 release CLI solve 的中位墙钟；`--record` 登记
   `benchmarks/solver-baseline.json`（n=40: 0.10s / n=50: 0.22s /
   n=60: 0.47s / n=80: 1.40s，交互级），`--check` 断言 ≤ 基准×1.10
   且 ≤ 绝对上限（n=80 ≤ 6s）。CI long-run-gates job 已接
   `--check`。
3. **编辑长跑峰值内存（§19.8）**：`editing_long_run` 的 1000 命令测试
   增加 RSS 采样（每 100 步 + 起始/峰值），断言增长 < 64 MiB。
4. **盘点确认（§17.3 已覆盖项）**：exact differential（n≤8 暴力枚举 +
   false-ProvenInfeasible 断言）、planted known-feasible corpus
   （20/40/50/60/80 × 20 = 100 实例）、取消延迟上限（<5s）、500 次
   solve RSS 稳定——均在 `long_run_gate.rs`/`exact_differential.rs` 且
   CI long-run-gates job 常跑。
5. **known-feasible 100% 收紧（§6.6）**：planted corpus 构造保证可行
   （每条约束都由 planted assignment 满足派生），实测 **100/100 solved
   （rate 1.0000）、false-ProvenInfeasible=0**；断言从 ≥99.5% 收紧为
   solved==total（99.5% 保留作未来非保证语料的兜底）。

**M3 Exit Gate（修订版 §6.7）2026-08-10 证据评估：通过。** 六项全部闭合
（§19.14–§19.16 证据链）：七状态语义、feasibility report UI 消费字段、
hard-search/soft 分离、official parity corpus 全绿（464/0）、rule
registry 生效、OR-Tools 不再依赖。质量 gate：regret PASS + cost-vs-
fallback 34/34 + known-feasible 100% + 随机 ≥99.5% + 性能回归门槛 +
长跑内存。剩余登记项（非阻断）：React 第二套规则真相（M6 删除）、
official corpus 官方来源扩充、CLI stdout 字节级 golden、M4 Decision
Backlog（需产品输入，修订版 §7.1）。

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
  文件级与项目级 rotation → `RUST_VERIFIED`（34/34 语义差分：每期学生
  全集完整、状态、结构一致）。relation_totals/category_totals 为"被占用
  座位对"计数：**全占满布局（31 case）解无关，逐键严格相等**；含空座
  布局（3 case）随解变化，按语义比较并登记说明（p20-rect-extra-sparse
  等）。同时登记 oracle 缺陷：v1 生成器逐期复用同一 seed 导致两期座位
  表完全相同；Rust 按 `seed + period - 1` 逐期推进，差分断言 Rust 两期
  必须不同；
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

### 19.17 2026-08-10：M4 批 2 决策与 ledger 状态变更（§1/§12/§14/§15/§18）

批 2 决策（`docs/product-decisions/2026-08-10-batch2-export-wrapup.md`，
方向冻结、版式与 UI 细节待研究）对账本的变更：

- **§1.1/§1.2**：`workspace`、`desktop`、`init-demo`、`presets`×3
  共 6 条 → `INTENTIONALLY_REMOVED_V2`（§18 登记：理由/迁移/用户影响）。
- **§12.1**：print-html（PD-D11 恢复独立版式）、PNG（PD-D13 渲染姓名）、
  PDF（PD-D12 系统字体智能引用）三条保持 `RUST_PARTIAL`，注明 M5 实现计划。
- **§14**：原生文件对话框（PD-D14 三入口融合：拖拽 + Tauri 对话框 +
  可信根内路径输入，安全红线）保持 `PYTHON_ONLY`，注明 M5 实现计划。
- **§15 计数**：`PYTHON_ONLY` 11→7、`RUST_PARTIAL` 69→67、
  `INTENTIONALLY_REMOVED_V2` 0→6。

**注意**：以上为决策状态变更，不是 parity 证据；print-html/PNG/PDF/
原生对话框的实现与 golden 属 M5 阶段工作，不得因"决策已定"宣称 parity。

---

### 19.18 2026-08-10：技术线收尾（架构收敛/模块化/DTO/property/fuzz/CLI golden/repair 空座锁）

技术收尾批次（计划 §1.1/§1.2/§4.2/§11.3/§11.4/§5.5），全部经 workspace 测试
（480 通过，除 PyO3 本地解释器问题）与 clippy -D warnings 验证：

1. **目录收敛（§1.1）**：`native/seattrellis_core`、`native/seattrellis_cli`
   迁入 `crates/seattrellis-core`、`crates/seattrellis-cli`；全部路径引用
   （workspace members、五 crate path 依赖、CI、scripts、文档）更新；
   PyO3 crate 保持隔离。commit `8477272`。
2. **lib.rs 单体拆分（§1.2）**：6595 行拆为 evaluation/solver/engine/
   scoring/precheck/audit/repair/reports/candidates 九个模块，公共 API
   crate 根 re-export 不变；测试跟随 lib.rs（crate 路径引用）。
3. **typed DTO（§4.2）**：`dto::candidate_set` + `dto::plan_comparison`
   （deny_unknown_fields、交叉不变量）；xtask 生成
   `candidate-set.v2.schema.json` / `plan-comparison-report.v2.schema.json`
   （共 8 个）；**oracle golden 全量解析 0 失败**
   （`candidate_dto_fixtures.rs`：全部 `goldens/*/candidates.json`）。
   §4 两条目 PYTHON_ONLY → `RUST_VERIFIED`。
4. **property-based（§11.3）**：solver 4 门（Solved⇒validator 0 违规+唯一、
   seed 确定性、加空座不破 fixed、输入重排语义不变——planted 生成）、
   editing 4 门（undo/redo 恢复、stale 拒绝、failed batch 无 partial、
   revision 单调）、migration 2 门（round-trip、envelope 契约、字段保全）。
5. **fuzz targets（§11.4）**：proptest 随机字节轰炸 22 个入口（core 9 /
   schema 6 / io CSV 3 / domain editor 2 / export 2），断言不 panic、不
   OOM、不目录穿越；环境无 nightly/libFuzzer，cargo-fuzz 迁移路径保留。
6. **CLI stdout golden（§5.5/§1）**：`fixtures/cli-goldens/` 13 命令契约
   （help/version/solve/validate/precheck/audit/candidates/history-report/
   pair-report/repair/schema-*）；harness `--cli-golden` 13/13 0 mismatch
   （JSON 规范化 + tmp 路径剥离 + Python exit 语义对照）；CI 差分 job
   已接入。§1 六条目 RUST_PARTIAL → `RUST_PARITY_PENDING`。
7. **repair 空座锁（§16 D.11）**：镜像 Python `reserved_empty_seats`——
   空座锁定后重解保持空、占用座仍为锚点、与 hard fixed 规则冲突拒绝
   （`repair_empty_seat_lock.rs` 3 测试）。
8. **roster 表头别名镜像（§16 D.15）**：Rust `aliases_for` 与 Python
   `COLUMN_ALIASES` 逐项一致（`roster_alias_mirror.rs` 2 测试，
   含归一化规则对照）。

**收尾轮（同批次继续）**：
- **repair saved locks（§16 D.11）**：镜像 Python `reuse_saved_locks`
  默认语义——snapshot metadata 的 `lock_state`（容忍 `manual_edit`/
  `repair` 旧键）持久锁自动合并进锚点，显式参数优先、去重；
  affected ∩ saved-locked 冲突拒绝（`repair_empty_seat_lock.rs` 共 5 测试）。
- **rotation 逐期 validator**：确认硬闭合——每期经 `solve_core`，
  feasible 时强制 `validate_solve_response`（class_generation.rs:164-169），
  rotation_gate.rs 断言每期 assignments 非空 + 独立 validator 路径。
- **属性补强（§11.3）**：io backup/restore property（随机内容 → in-place
  迁移产生 `.bak` → restore 语义恢复原文档，`property_backup_restore.rs`）；
  canonical 规范化幂等 property（schema）。
- **cargo-fuzz（§11.4）**：nightly 工具链 + cargo-fuzz；6 个 libFuzzer
  targets（solve_request/dto_parsers/csv_importer/editor_commands/
  export_options/migration）构建通过、2000+ runs 无 panic/崩溃；
  CI 新增 `fuzz-targets` job（nightly + build + bounded runs）。
- **CLI golden 扩展（§5.5）**：13 → 21 命令（+project-init/info/validate/
  solve/export/rotate/privacy/pack 全生命周期），21/21 0 mismatch。

**全部未闭合项已闭合**（§19.18 原登记四项：saved-locks ✓、rotation 逐期
validator ✓、cargo-fuzz ✓、CLI 全参数 golden → 21 命令代表性集 + project
全生命周期 ✓）。剩余边界：CLI 参数组合的全量枚举（当前为代表性集）、
fuzz corpus 的长期积累（CI 短跑 + 本地长跑）。

### 19.19 2026-08-12：M5 阶段 A 实现（导出/打印/字体/PNG/默认值/示例/registry）

M5 阶段 A（计划 §8.1 前置的 Rust 能力补齐）第一批，全部独立于 Python
导出实现（Python 导出作废，不作参照——产品负责人 2026-08-12 确认）：

1. **A1 导出选项统一化（§12.3）**：`paper_size`（a4/a3/letter）与
   `margin_mm`（clamp 5–25）新增到 ExportRequest 并作用于 PDF；
   DOCX 支持 orientation（pgSz 交换）；print-html 支持全页面选项。
   6 个选项 gate（含拒绝规则）。
2. **A2 print-html 独立版式（D11）**：恢复独立格式（归一化移除）；
   按打印版式规范（2026-08-12 修订：横版默认、一页最大化、字号按最长
   姓名统一、只放姓名、讲台/过道/窗/门文字标注、页脚溯源 seed）。
   5 个版式 gate + server 集成测试。
3. **A3 PDF 系统字体引用（D12）**：`fonts.rs` 字体发现（PingFang →
   Noto CJK → YaHei → SimSun 优先级链）；PDF Type0/UniGB-UCS2-H 引用、
   UTF-16BE hex 文本编码、CJK 姓名/标题渲染、质量警告（Fallback 字体
   页内提示）。测试字体环境自适应。
4. **A4 PNG 姓名渲染（D13）**：fontdue 光栅化（纯 Rust）、字号自适应、
   alpha 混合、无字体优雅降级；像素级 gate。CI Linux 安装
   fonts-noto-cjk 以覆盖 CJK 路径。
5. **A5 导出默认值记忆（D9）**：`io::export_defaults`（用户配置目录
   原子写、坏文件忽略）；导出流程缺省字段应用记忆 + 成功后记住。
   班级级覆盖随项目上下文（M5-B）落地。
6. **A6 示例名单（D10）**：20 人静态资产（常见姓名、属性覆盖）、
   构建期校验（9 字段映射/唯一 id/leader/vision 计数/性别均衡/CJK 名/
   身高成绩范围）。隔离命名空间随班级工作流（M5-B）。
7. **A7 规则 registry 消费（§4.3/M6 前置）**：React 新增
   `domain/ruleRegistry.ts` 消费 seam（findRule/requireRule/分类）；
   gate 断言工作台全部规则 id 在 Rust registry 内。generation.ts
   自编译逻辑保持 M6 删除登记，未扩展。

**状态变更**：§12.1 print-html/PNG/PDF、§12.3 orientation/page_scale/
margin_mm/paper_size 共 7 条 `RUST_PARTIAL` → `RUST_PARITY_PENDING`；
§15 计数 RUST_PARTIAL 61→54、RUST_PARITY_PENDING 99→106。
验证：workspace 504 测试、React 67 测试 + typecheck、clippy -D
warnings、340-case 导出差分 0 mismatch、21 CLI golden 0 mismatch。
剩余 M5 阶段 A：print-html/PDF 的独立 reader 全量验证（可并入
--exports 扩展）、导出默认值 dogfood 冻结（G-4）、班级级默认值覆盖。


### 19.20 2026-08-12：M5 阶段 B 实现（批 1 融合形态 B1–B8）

M5 阶段 B（计划 §4，批 1 八项融合形态，每项 = Rust contract 确认 →
React 实现 → E2E，对照 `docs/prototypes/decisions/` 目标形态）：

1. **B1 导航（D1）**：侧栏（我的班级/班级内容/任务）+ 上下文操作条
   （下一步引导链）+ 首次任务清单（用过即收，localStorage 持久）；
   临时工作台/班级上下文切换（脏稿确认）；另存为班级（G-5，会话级）。
   班级数据持久化到本地项目服务登记为 alpha.1 后续项。新组件
   Sidebar/ContextBar/FirstRunChecklist/HistoryRotationPanel 等。
2. **B2 画布（D2）**：drag-lift（整块跟随、悬停高亮、松手 swap）、
   框选批量（rubber band + 原子 lock/unlock 命令）、批量移动
   （原生 `batch_move`，planBatchMove 纯函数 + 环安全）、表格视图
   （同 draft，move/unseat，重复拒绝仅限目标被他人占用）、共享
   undo/redo（工具栏 + ⌘Z/⌘⇧Z/⌘Y）、指针→viewBox 缩放映射。
3. **B3 规则（D3）**：新 Rust API —— `seattrellis-rules::sentence`
   （7 个句式模板 + 槽位→参数路径绑定）、`POST /api/v1/rules/compile`
   （结构化 422）；React 句式构建器（槽位编辑器）+ 规则卡片
   （启停/编辑/删除）+ 高级编辑 + 只读 JSON 视图（PD-D3-ADJ-1）。
   约束/分组新增 `enabled` 过滤。401 会话重引导（服务重启不再掉
   demo 模式）。
4. **B4 快速/高级（D4）**：生成视图 = 3 问（规则集/候选数/本轮优先）
   + 历史行默认可见（不折叠）+ 高级折叠（种子/预算/后端/去重说明/
   自定义规则/轮换）；候选数默认 5（G-4 冻结待 dogfood）。
5. **B5 候选（D5）**：新端点 `GET /api/v1/editing/drafts/{id}/audit`
   （PlanScore 七维 + 硬约束摘要，与 D6 共用）；`generate_class`
   接入候选引擎（options.candidate_count > 1 生成去重候选集，耗尽
   映射 Unknown 领域结果；count=1 路径与七状态语义不变）；React
   候选面板（理由卡 + A/B 差异高亮 + 分数明细/逐规则 + 复现折叠 +
   选用切换）。G-1 术语映射表落地（auditTerms）。
6. **B6 诊断（D6）**：core `diagnostics_report_json`（评估但不拒绝：
   违反硬规则的完整 assignment 产出 witnesses + 建议修复座；结构
   非法仍 422，M3-06 不放松）；诊断面板（严重级列表 + 修复按钮 +
   一键修复全部，走 Rust editing + 重新 audit 复核可见）；画布内联
   徽章（放大命中区）+ 双向联动。
7. **B7 历史/轮换（D7）**：历史回顾（时间线 + 恢复此版本，脏稿确认）
   + 轮换计划（周期卡片并排、点击载入该期）双视图切换；恢复走
   snapshot 纯函数（按座位 id 匹配当前教室、锁定座保留）。
8. **B8 导入（D8）**：在既有三步导入（选择/映射/确认）上补齐原子
   确认栏（原子应用/失败回滚/可审计文案）+ 同屏预览行级冲突徽章
   与空单元格提示。

**关键 Rust 契约新增**：`sentence_templates/compile_sentence`（B3）、
`draft audit` 端点（B5/B6）、`diagnostics_report_json`（B6）、候选
引擎接入 server 生成路径（B5，§6.3 实现路径补齐）。全部走独立
validator 复核（editing 命令 + audit 重算），无硬编码 feasible。

**验证**：React 132 vitest（+45）、typecheck、vite build；Rust
core/rules/server/application 全绿（server 79 含多候选与 audit 契约
测试）、clippy -D warnings 0；浏览器 E2E 覆盖 B1–B7 关键交互
（生成/拖拽交换/框选锁定/表格编辑/句式编译/候选选用/诊断修复闭环/
历史双视图，均对本地 Rust server）。
**已知后续**：班级项目持久化（alpha.1 默认路径）、G-4 默认值 dogfood 冻结。
（候选 n=1/5/20 与导出独立 reader 的 golden 扩展已于 §19.31 完成。）

### 19.22 2026-08-12：M5 阶段 C 实现（桌面与平台，计划 §5）

**C1 三入口文件选择（PD-D14）**：`tauri-plugin-dialog` 接入壳
（dialog:default 权限 + `read_user_file`/`write_user_file` 两个 IPC
命令，8MB 上限）；后端新增 `POST /api/v1/files/read`（可信根内
**相对路径**读取：绝对路径/`..`/NUL/反斜杠拒绝，canonical 包含性
防 symlink 逃逸，8MB→413；M1-05 中间件自动生效）+ `GET
/api/v1/files/root`（暴露可信根供 UI 提示）；`ServerConfig.trusted_root`
（默认 cwd，壳在 Finder 启动 cwd=/ 时回退 HOME）。React
`FilePicker` 组件三入口融合（① 系统对话框=Tauri ② 拖拽=语义色边框
高亮 ③ 路径输入=相对路径+客户端/服务端双重校验），浏览器保留
input[type=file] 兜底；名单导入与导出保存均切换至新桥（v1
pywebview 桥不再被 UI 消费，删除留 M6）。壳 devUrl 指向 vite dev
server，`SEATTRELLIS_PORT` 支持 dev 环回。服务端 86 测试（+7
files/read/root 契约）、Web 146 测试全绿。
**C2 平台自适应**：`isMacOS`/`platformModifierLabel`（⌘/Ctrl）驱动
画布帮助文案（含 SVG aria 描述）；全局 Cmd/Ctrl+Z 不再吞掉表单
控件的原生文本撤销（`isEditableTarget` 守卫）；`prefers-reduced-motion`
全局 CSS 已存在并验收（动画/过渡/滚动 kill-switch，无 JS 装饰动画）；
SF Symbols 替换（设计方向 §5 可选项）**评估后暂缓**——线性单套图标
与苹果视觉语言已一致，手写 SF path 数据有字形错误风险，列入 M7
打磨候选。
**C3 触控 Decision Gate（§7.2）**：决策记录
`docs/product-decisions/2026-08-12-touch-decision-gate.md`——触控
**不作为 v2 final 必须项**（桌面优先），降级可用（Pointer Events
tap/拖拽 + `touch-action:none` + Ctrl+滚轮缩放=macOS trackpad 捏合
同一事件）；双指捏合/触控消歧/44px 目标审计/真机验证列 M7 候选，
不阻断 alpha/beta。
**阶段 C 退出**：C1/C2/C3 各自验收（Rust 测试+clippy 0、Web 146
测试、typecheck/build、浏览器 E2E 覆盖路径读取闭环与绝对路径拒
绝）；提交 591e95f / fd5d147 / 411e229。

### 19.23 2026-08-12：主题收敛 + Anthropic 风格视觉重做（决策记录
### `2026-08-12-single-theme-anthropic.md`）

产品负责人决策：**5 套主题 → 单一主题**（删除主题选择器，浅/深色
跟随系统），风格基准改为 **Anthropic 设计系统**（swatch 命名
slate/ivory/clay/cloud/olive/coral/sky/heather + 衬线标题/无衬线
正文，色值与字体经抓取 anthropic.com 生产 CSS 实证）：

1. tokens.css 重写为单一主题：暖白底 #faf9f5、石板墨字 #141413、
   陶土强调 #c6613f、云灰边框；语义色映射 olive(满足)/kraft(建议)/
   sky(提示)/coral 深(违规)/heather 深(锁定，画布锁定色从 warning
   切换落实"紫=锁定")；深色为同系反转；圆角收敛 6/10/14px。
2. 主操作按钮改墨色（slate-dark），陶土仅做链接/激活/聚焦——与
   Anthropic 官网 CTA 一致，并避免与违规红撞色。
3. 标题字体栈改衬线：Tiempos Text→Georgia→Times New Roman +
   中文 Songti SC/Noto Serif SC/SimSun（不打包字体，延续 D12）；
   负字距放宽至 -0.01em。
4. 删除 theme/theme.ts、AppHeader 主题选择器、i18n theme.* keys、
   app.css 全部 `data-theme` 特定段（77 行）。
5. 验证：146 vitest、typecheck、build 全绿；浏览器确认页头仅剩
   语言选择器；像素值待 G-4 dogfood 冻结。

### 19.24 2026-08-12：导出功能实测修复（dogfood 反馈）

> **后续实证修订**：本节第 1 项 Identity-H 结论已被 §19.26 推翻并替代；
> 保留原记录用于解释回归来源，不再代表当前实现或验收结论。

产品负责人实测反馈：导出"几乎全部不可用"——文字不显示 / HTML 表格
巨大占满屏幕且字小。逐格式实测（8 格式全部导出 + reader 检查）：

1. **PDF 文字不显示（根因修复）**：手写 PDF 使用 `/Encoding
   /UniGB-UCS2-H` + UTF-16BE，依赖查看器内置 CMap 文件；poppler
   及多个查看器没有该 CMap，全部文字拒绝渲染（`No font in show`）。
   改为 **`/Encoding /Identity-H` + `/CIDToGIDMap /Identity`**，文本
   编码为**字形索引（GID）的 2 字节 hex**（fontdue 解析系统字体 cmap
   查 GID，无 BOM）——Identity-H 是全部 PDF 查看器内置能力，不再有
   CMap 依赖。验证：poppler 报错消失、sips/CoreGraphics 渲染有文字
   墨迹（对比基线）；字体替换场景按 D12 决策接受（同平台查看器有
   被引用字体）。新增契约测试（无 UniGB、有 Identity-H/CIDToGIDMap、
   无 BOM）。
2. **HTML 表格溢出 + 字小（修复）**：基础 html 原为固定 92px 单元格
   + 12px 字（9px 详情），小屏/投影下溢出且不可读。改为
   `table-layout: fixed` + `width:100%` + `max-width:1000px` 响应式
   均分，字号升至 15px 姓名 / 11px 详情 / 22px 标题，640px 以下
   再降档。
3. **逐格式复查（非问题）**：SVG（51 个 text 元素含中文名）、PNG
   （fontdue 光栅化墨迹正常；长姓名自动缩字属设计）、print-html
   （字号算法正确：最长姓名→统一字号、24pt 上限、8pt 下限、截断
   提示；mm 布局为打印资产，屏幕打开显示 mm 版式属正常）、
   XLSX（inlineStr 含名）、DOCX（w:t 含名）、PPTX（a:t 含名）。
4. 验证：export 51 测试（+2：Identity-H 契约）、server 86 测试全绿；
   8 格式产物经 reader/渲染实测。

### 19.25 2026-08-12：工作台 UI 第二轮重设计与真实主流程修复

对照 M4 已冻结 D1–D8、M5 阶段 B 与
`2026-08-12-single-theme-anthropic.md`，用真实 Rust 本地服务在浏览器走通
名单 → 教室 → 规则 → 生成（5 候选）→ 座位编辑，修复以下展示/可用性问题：

1. 页头删除硬编码班级名，当前上下文只由 ContextBar 呈现；修复本地最近
   班级为空时侧栏无内容，并隐藏侧栏项目绝对路径。
2. 名单默认只显示姓名/学号；排座资料与表格导入渐进披露，减少非核心决定；
   能力和原子导入事务不变。
3. 侧栏区分“工作上下文选中”和“当前页面选中”；新手引导按有效名单自动
   完成第一步并强化当前步骤。
4. 修复 canvas grid 自动放置：候选比较原先抢占整行、导致画布跌出首屏；
   现在微调/画布/诊断首行同屏，候选比较第二行，且无水平溢出。
5. 参考边界明确：Anthropic 官网用于品牌色与留白，Claude.ai 产品壳用于
   任务聚焦，Apple HIG 用于层级/反馈/可访问性；不复制营销站衬线正文到
   名单与控件。

**证据**：React 148 vitest、typecheck、production build 全绿；浏览器真实
生成后画布首屏可见。仅改 React 展示与输入级状态，未新增 TS 规则语义、未改
Rust/schema/API contract，**不据此提升任何 parity 状态**。
### 19.21 2026-08-12：UI 视觉打磨首轮（设计方向 §8.2 像素 token 制定）

产品负责人决策（记录：`docs/product-decisions/2026-08-12-ui-visual-polish.md`）：
**不引入液态玻璃/毛玻璃**（§7 反模式红线维持），**单一跨平台主题**；
对阶段 B 视觉质量不满意 → 按设计方向 §8.2 启动像素级 token 首轮制定
（信息架构与交互不变）：

1. **字号刻度**：tokens.css 新增 `--text-2xs…--text-2xl`（11/12/13/14/15/
   16/18px）；app.css 全量规范化——原 0.58–0.85rem（≈9.3–13.6px）约
   120 处收敛为 11/13/14px 三级；UI 文案下限 11px（10px 仅保留画布
   viewBox 字形）；字重 750→650、760/800→700。
2. **默认主题色板精修**（深色同步）：文字/边框加深（muted
   #69707d→#565d69）、强调色 #4361ee→#3457d5（去 SaaS 霓虹）、
   radius-large 18→16px。
3. **控件与布局**：输入/选择高度统一 36px；侧栏项 38→40px；面板
   留白加大；内容列 1080→1200px。

**验证**：132 vitest 全绿、typecheck 通过、vite HMR 无错、DOM 结构
完整；浏览器实时目检交付产品负责人。**未冻结**：像素 token 待 G-4
dogfood 目检（含深色主题）后按 §8.2 正式冻结，数值可随目检修订。

### 19.26 2026-08-12：座位主视图与导出 correctness 整改

产品负责人 dogfood 报告：座位画布/姓名过小，PDF 姓名显示为点，其他导出
也难以正常使用。复现与整改如下：

1. **PDF 根因及 D12 修订**：上一轮 `/Identity-H` +
   `/CIDToGIDMap /Identity` 写入的是生成端系统字体 GID，却未嵌入该字体；
   查看器替换字体后 GID 不等价。Poppler 实测报告
   `Unknown character collection Adobe-UCS`，渲染为点/方框/错误字母。
   已移除 Type0/GID 路径，改为导出端用系统字体完成整页排版，以 144 DPI
   RGB Image XObject + ASCIIHex/FlateDecode 无损封装（编码异常时回退
   RunLengthDecode）。当前取舍为跨查看器
   可读优先、文字暂不可搜索；未来可搜索 PDF 只允许字体子集嵌入。
2. **PNG 完整性**：原 PNG 虽绘制姓名，但标题/讲台/空座缺失且只有 1x，
   群聊和投影中明显过小。现为 2x 输出，包含标题、副标题、讲台方向、姓名、
   座位号和空座标签，与 PDF 共用系统字体光栅化。
3. **前端导出默认值**：修复不存在的默认 format `print` → 契约值
   `print-html`；随后按 dogfood 合并用途重复的 HTML/打印版，快速导出目录
   显示 SVG/PNG/PDF/print-html/XLSX/DOCX/PPTX 七项；底层 `html` 仍兼容。
4. **座位画布**：修复 holder 固定在 430px 的内在宽度和三列挤压；画布改为
   第一行全宽主舞台，新增一键专注视图（遮罩/Esc 可退出），编辑状态与 undo
   栈仍为同一份。

**自动证据**：`seattrellis-export` 51 单测 + 6 export-options + 2 fuzz-style
测试 + doctest 全绿；`cargo clippy --all-targets -p seattrellis-export --
-D warnings` 全绿；PDF 经 `pdftoppm -r 144` 独立渲染并人工确认文字/中文标签
可见，示例由错误版 5.6KB / 无文字变为 87KB / 可读（Flate 与 RunLength
页面渲染逐字节一致）；PNG 独立解码为 976×712 并人工确认标题/姓名/座位号。React 专注视图
新增测试，typecheck 通过；真实 Rust 服务完成名单→5 候选→画布，常规与专注
视图浏览器目检通过。D12 冻结记录已在
`2026-08-10-batch2-export-wrapup.md` 以 R2 实证修订。

该整改修复已登记实现的导出路径，不新增 oracle 等价 golden，故不改变 ledger
parity 状态计数；PDF 可搜索性与无系统 CJK 字体的失败策略继续登记为边界。

### 19.27 2026-08-12：全格式姓名缺失与 HTML 入口收敛

产品负责人提供的 SVG/print-html/PNG/XLSX/DOCX/PPTX 产物经逐一解包检查：
每个已占座位均写入固定文本“学生”（示例 18 处），PPTX/DOCX/XLSX 的 OOXML
内部同样如此。根因不是七个 renderer 同时丢字，而是首次导出模板错误设为
`public`，Rust `render_export` 在格式分发前统一执行 `anonymize_grid`；同时
前端预览此前只检查 `privacy.anonymize`，没有展示 public 模板会匿名，造成
预览与下载不一致。

整改：Rust 缺省模板、Rust 导出设置记忆和 React 首次模板统一改为
`teacher`，真实姓名默认保留；`public` 仍强制匿名且界面明确写明“只显示
学生”。预览与后端采用相同匿名条件。Office OOXML 增补中文语言、东亚字体、
DOCX styles/fontTable/relationships，并去除表格 XML 中的字面反斜线。
用户目录合并基础 HTML 与打印版，仅展示 `print-html`；后端仍接受基础
`html` 以维持 8 格式契约兼容。

**自动/独立证据**：新增缺省模板保留 `Alice`/`张伟`、public 隐藏、预览
一致性及三种 Office 包内 `林晓雨` 回归；`seattrellis-export` 53 测试、
React 153 测试与 typecheck 通过。35 个中文姓名的真实产物经 Poppler 验证
PDF、PNG 解码和 PowerPoint 独立渲染均可见；XLSX/DOCX 由独立 reader 解包
确认姓名及字体声明。当前 LibreOffice 验证环境连最小 `python-docx` 中文
文档也无法渲染中文，故不将其空白结果作为 SeatTrellis renderer 失败证据，
后续 Windows Word/Excel dogfood 仍保留为发布前平台验收项。

### 19.29 2026-08-12：阶段 D2/D3/D4 收口（dogfood 冻结 + parity 升级 + alpha 退出对照）

**D2 dogfood（G-4，`2026-08-12-dogfood-closure.md`）**：导出默认值
冻结（默认模板 teacher、默认格式 print-html、A4 横向自动缩放、public
强制匿名，证据=E2E 断言 + §19.26/19.27 实测）；打印字号算法验证
（最长姓名→统一字号、24pt 上限/8pt 下限/截断）；**补齐 D10 示例
名单一键使用入口**（空名单空状态 + 按钮，StudentRosterEditor
`onUseDemo`，154 vitest）；B1–B8 关键交互目检矩阵（E2E 证据为主）。
**D3 parity 升级**（§19.18 CLI golden 21/21 0 mismatch 为证据）：
`pair-report`、`schema list`、`schema export` 由 RUST_PARITY_PENDING
→ `RUST_VERIFIED`（golden 文件已登记）。
**D4 §8.3 对照（`2026-08-12-alpha-exit-check.md`）**：条件 2（Rust-only
E2E 4/4）、3（schema round-trip）、4（Python 仅 oracle/test）**达成**；
条件 1（无 RUST_PARTIAL 必须项）**未达**——CLI doctor/validate/edit/
repair/history-report 与 project-* 的缺口登记为 **alpha.2**（§8.2
parity gap 清单 + 发布前平台验收项）。**alpha.1 默认路径收口达成**。

### 19.28 2026-08-12：阶段 D1（alpha.1 主流程 E2E 扩展 + 暴露的三处真实 bug 修复）

NO_PYTHON_RUNTIME E2E 扩展为完整主流程（import → solve → edit →
rotation 保存 → 项目重开 → rotation 加载 → 导出默认值），4/4 全绿
（真实 Chromium vs Rust 二进制），并暴露/修复三处真实 bug：

1. **座位点击失效（回归）**：pointer capture 在 pointerdown 时于
   stage 容器上激活，浏览器合成 click 被重定向到容器，座位 g 的
   onClick 永远收不到。修复：capture 推迟到首次真实拖拽移动时激活；
   纯按压正常点击、拖拽仍捕获（SeatingCanvas）。
2. **generate 视图轮换设置不可达**：panel-content 缺 min-height:0，
   flex 子项拒绝收缩，内容被卡片 min-height + overflow:hidden 裁剪且
   无法滚动（轮换设置在折叠区下方不可见）。修复：panel-content
   min-height:0 + overflow-y:auto。
3. **轮换加载崩溃**：load 响应缺 editor/period_editors（M2 声称接好
   但只在 generate 路径构造了 drafts），handleRotationLoad 在
   period_editors.length 上崩溃（M2 的 E2E 因画布仍显示旧方案而假
   绿）。修复：服务端 load 时从项目 roster + layout 重建每期 editable
   draft（candidate_id period-N，与 generate wiring 一致）。

E2E 同步更新（B7/D1 后的 UI 变化）：catalogs 断言改 7 格式（html
隐藏）、向导由上下文操作条驱动（footer 移除）、导入渐进披露、
项目工具在历史视图折叠区内、项目 layout fixture 与 standard-30 过道
几何一致（draft 可重建）；新增导出默认值测试（print-html 保真名 +
PDF 光栅页，锁 public 匿名回归）。验证：E2E 4/4、React 153、
server 107 + export 53 + io、clippy -D warnings 全绿。

### 19.30 2026-08-12：CLI parity 缺口关闭（doctor/validate/edit/repair/history-report/project-* 输出契约与参数对齐）

关闭 ledger §1 表中 RUST_PARTIAL / RUST_PARITY_PENDING 的 CLI 输出契约
缺口：`scripts/rust_python_diff.py --cli-golden` 由 21 命令扩到 **33
命令，33/33 0 mismatch**（golden 全部由 harness 真实运行记录，非手写）；
Python 侧按"有对应命令即对照 exit 语义（0 vs 非零）"参与。验证：
`cargo test -p seattrellis_core -p seattrellis-io -p seattrellis_cli`
全绿（core 107、cli 32 含新增单测）、`clippy -D warnings` 全绿、
新增文件 rustfmt 干净（工作区整体 `cargo fmt --check` 被其他在改的
objectives/reports/scoring/input_boundary 未格式化内容阻塞，未触碰）。

1. **doctor**：harness 新增 doctor case（Python `doctor` exit 语义对照，
   双 0）；golden `fixtures/cli-goldens/doctor.json`。stdout 的
   temp-dir 探测行经 harness 规范化（系统 tempdir → `<tmp>`）保证
   跨主机可重放；usage 补齐 DOCTOR 段与缺失命令清单。
2. **validate 警告语义**（对照 `run_validate` service.py:944 →
   validation.py）：Rust CLI `validate` 新增 `--preset/--history/--strict`；
   新 `crates/seattrellis-cli/src/presets.rs` 镜像 presets.py 14 个
   preset 的 requirements 与 `_DEGRADATION_NOTES`（`preset_catalog_mirrors_
   oracle`、`preset_context_warning_message_matches_oracle` 单测锁定目录
   与消息文本逐字节一致）；能力警告镜像 `_add_rule_capability_warnings`
   （score_distribution group-scope 缺 group_id）。goldens：
   `validate-warnings/strict/group-scope.json` 3 个新 + `validate.json`；
   Python exit 对照（validate 双 0、--strict 双非零）。剩余边界：
   `--history-dir` 未镜像（validate 只数 `--history` 文件）；
   "N students without student_id" 警告在 CoreSolveRequest 形态不可
   表示（key 已在编译期折叠为 student_id or name）。
3. **edit 子命令**：Rust CLI 新增 `edit`（对照 Python `edit_snapshot`
   cli.py:451-505）。字符串操作语法镜像 `_parse_edit_operation`
   （9 种 kind + 别名、batch-move `STUDENT=SEAT` 解析、错误消息），
   `--operations-file`（JSON list/object）、`--candidate`（candidate set
   按 `recommended_candidate_id` 解析，"recommended" 为默认）、`--strict`
   （独立 validator 拒绝）、`--output` 默认 outputs/edited.snapshot.json。
   artifact（快照或 candidate set）内嵌 students/layout/rules 经 io 新
   共享编译路径 `compile_solve_request_from_json`（自
   `build_project_solve_request` 提取，两者共用同一规则编译，避免
   双份语义漂移）编译成 CoreSolveRequest 后过独立 validator；摘要镜像
   `_format_edit_summary`。`project-edit` 的 `--operation` 同步改为
   Python 字符串语法（smoke_cli.py 即传 `swap:STU001:STU002`）。
   golden `edit.json`：Rust 与 Python 用同一 hist-short 快照文件，
   双 exit 0。§1 `edit` 行由"Rust CLI 无"升级。
4. **repair saved-lock**（对照 `repair_snapshot` + `--ignore-saved-locks`）：
   core 新增 `repair_json_with_options(..., reuse_saved_locks)`
   （`repair_json` 委托默认 true，旧签名不变）；CLI `repair` 与
   `project-repair` 新增 `--ignore-saved-locks`；repair 摘要
   `locked_students/locked_seats` 改为有效（saved+explicit）计数，对齐
   Python 摘要语义。goldens：`repair.json`（真实成功路径，原 golden
   记录的是 CoreSolveResponse 无 assignments 的失败）、
   `repair-saved-locks.json` vs `repair-ignore-saved-locks.json`
   （计划差异为自动证据：saved=1 有效锁+10 名学生移动，ignored=0/0）；
   core 新增 2 个单测（`repair_empty_seat_lock.rs`：
   ignore 时 affected∩saved-locked 不再冲突、显式锁仍生效）。
5. **history-report/pair-report**：新增 `--history-dir`（默认 glob
   `*.snapshot.json`，对齐 Python `load_history_snapshots`）与
   history-report `--output`；harness 修正为传 snapshot 文件并把 Python
   侧参数改为有效形式（--students/--layout/--history <file>）→ golden
   为真实报告（含 unknown student/seat 警告），不再记录目录读取错误；
   Python exit 对照恢复有意义（此前两侧都是参数错误退出）。
6. **project-\***：harness 新增 `project-list`（modified_at 时间戳经
   harness JSON 归一化为 `<timestamp>`，保证跨运行可重放）、
   `project-restore`（Python 侧用**同一 Rust 打包 bundle** restore
   成功 → bundle 双向互操作证据）、`project-privacy --no-include-outputs`
   （io 新增 `project_privacy_with_options`/`project_privacy_json_with_
   options`）、`project-edit`、`project-repair` 五个 golden；
   `project-pack` 新增 `--force`（Python 语义：拒绝已存在 bundle，
   main.rs 单测覆盖）。
7. **harness 规范化增强**：`_strip_tmp_paths` 追加系统 tempdir 剥离；
   `_normalize_cli_output` 增加 `modified_at/created_at` 时间戳 JSON
   归一（`_canonicalize_json`）；record 模式写归一化 stdout（golden
   文件字节稳定）。
8. **golden 结果**：33 命令 0 mismatch（连续两次独立运行一致）。
   §1 升级 14 行（doctor/validate/edit/repair/history-report/
   project-init/list/privacy/pack/restore/info/edit/repair +
   schema-migrate → RUST_VERIFIED，证据见上）。**保留 RUST_PARTIAL 的
   剩余边界**：`project-validate --strict`/warning 语义、`project-solve
   --candidates/--report`、`project-export --candidate`/其余格式 ——
   均为已登记选项级差距，golden 已覆盖其默认路径，不虚报。

### 19.31 2026-08-12：candidates golden 矩阵扩展（n=50/60/80）与导出 reader 全量验证

关闭修订版 §6.3/§6.6 登记的 candidates golden 证据缺口（原"仅 n≤40"）：

1. **candidates golden 矩阵**（`scripts/gen_parity_fixtures.py` 新增
   `CANDIDATES_GOLDEN_COUNTS`）：p50-custom-adj-sparse × {1,5,20}、
   p60-rect-exact-dense × {1,5}、p80-rect-exact-dense × {1} 提交字节稳定
   golden（`candidates-cNN.json` + `plan-report-cNN.json` +
   `objective-breakdown-cNN.json`；`--candidates 1` 与 snapshot 求解命令
   相同，按脚本既定策略存为 snapshot 副本，不重复求解）；corpus_version
   1.0.0 → 1.1.0，MANIFEST 重新生成（golden_hashes 覆盖新增文件，含
   SHA-256）。`gen_goldens` 先 rmtree 再重生成，保证 verify 不见陈旧
   golden 文件。
2. **确定性预算护栏（实测 2026-08-12，Apple silicon；CI parity-oracle
   重放约 1.85x 慢，37 min / 90 min job）**：每个候选数 = 一次完整
   deterministic fallback 求解（attempts = max(40, n*12)）——p50 ~30s、
   p60 ~90s、p80 ~270s/次。超 CI 重放余量的组合（p60×20 ~65min、
   p80×5 ~46min、p80×20 ~180min on CI）**不提交**字节 golden，由
   §19.6 `--candidates` live 差分（15 combos 0 mismatch）与 Rust
   candidates gate 测试（§19.20）覆盖——预算与覆盖策略写死在脚本注释，
   不虚报为 golden 已提交。
3. **导出独立 reader 全量验证（374 行 0 mismatch）**：`--exports` 对 34
   个合法 fixture case × 11 行 = 374 行（XLSX/DOCX/PPTX/PNG/PDF teacher
   模板 5 行 + public 模板隐私 5 行 + print-html 结构 reader 1 行），
   XLSX/DOCX/PPTX 由 openpyxl/python-docx/python-pptx 重开校验语义
   （行号连续、两 sheet、标题+座位表格、形状可编辑）、PNG/PDF 由
   Pillow/pypdf 校验（A4-ish 边界 + Image XObject 存在，§19.26 光栅页）、
   **print-html 由独立结构 reader 校验**（`_verify_print_html_export` 经
   CLI project 流程导出，`_verify_print_html` 用标准库 html.parser 断言：
   页骨架、`.grid-row` 座位网格行数、姓名齐全（容忍截断）、窗/门/过道
   结构标注、页脚 seed 溯源、无 height/vision 明细泄漏）——`print-html`/
   PDF 两行 §5.6 由此获得"独立 reader"证据。**print-html public 匿名渲染
   无 CLI 路径**（`export` 命令格式面不含 print-html；`project-export`
   固定 teacher/orientation=portrait/show_student_ids=true），public 匿名
   由 render_export 共享网格层（格式分发前 anonymize_grid，§19.27）对 5
   格式验证覆盖，登记为 CLI 侧待办（另一 agent 域），不虚报为 print-html
   public 已验。复跑：`.venv/bin/python scripts/rust_python_diff.py
   --exports`（系统 python3 缺 pptx 等 reader 包会假 mismatch，必须用
   venv 解释器）。
4. **复跑确认（2026-08-12，本条目工作机）**：① 全量
   `gen_parity_fixtures.py verify` EXIT 0——41 case 的 inputs+goldens 逐
   字节复现，0 DIFF / 0 MISSING / 0 SKIP（含新提交的 p50/p60/p80
   candidates golden 与 p80 snapshot 副本）；②
   `rust_python_diff.py --candidates` 15/15 combos（20/40/50/60/80 ×
   1/5/20）0 mismatch——含 n=50/60/80 的全部 1/5/20 组合（状态类 +
   生成数量两侧一致）；③ `--exports` 374 行 0 mismatch；④ 反向有效性：
   用 §19.26 之前的旧 release 二进制重跑 `--exports` 出现 36 项
   PDF/print-html 假 mismatch（"PDF page carries no image content"），
   以当前源码重建后 0——证明 reader 实际检出内容差异而非恒真。

### 19.32 2026-08-12：ledger 全表对账（§8.3 alpha.2 退出条件前置）

> 本对账**只更新账本状态与证据引用，不新增实现**。逐行核对 §1–§14 全部
> `RUST_PARTIAL` / `PYTHON_ONLY` 行与 §1 表、§19 证据（33 个 CLI goldens
> 0 mismatch、parity corpus v1.1.0、§19.1–§19.31 记录）及代码复核
> （`crates/seattrellis-cli/src/main.rs`、`seattrellis-io/src/{projects,roster}.rs`、
> `seattrellis-schema/src/dto/`、`seattrellis-application/src/rotation.rs`、
> `seattrellis-core/src/{models,reports}.rs`、`seattrellis-domain/src/goal_rules.rs`、
> `seattrellis-export/src`）的一致性。原则：`RUST_VERIFIED` 只来自自动证据
> （golden 文件、0-mismatch 差分、已登记的等价自动化对照），"路径存在"不升级。

**自动证据支持升级 `RUST_VERIFIED`（18 行）**：

| 行 | 证据 |
|---|---|
| §1.4 错误/退出码契约 | `--cli-golden` 33 命令逐条记录 exit 码（含 audit exit 1、project-export/schema-migrate/validate-strict exit 2）+ Python 0 vs 非零语义对照，33/33 0 mismatch（§19.30）；v2 冻结退出码 0/2/3/4/5/70/130（main.rs:1458-1469） |
| §2.1 `solve` / `solve_with_report`（文件级+报告） | 41 fixtures 七状态差分 0 mismatch（§19.5）+ golden `solve.json`（§19.30）；`--report` 由 audit/score/precheck 子命令承担（§19.8/§19.16） |
| §2.5 `compute_edit` / `edit_snapshot` / `project_edit` | golden `edit.json`/`project-edit.json`（§19.30）+ §19.12 project-edit 生命周期测试 |
| §2.5 `compute_repair` / `repair_snapshot` / `project_repair` | §19.18 空座锁 + saved locks + §19.30 `--ignore-saved-locks`；goldens repair/repair-saved-locks/repair-ignore-saved-locks/project-repair |
| §2.6 `compute_history_report` / `run_history_report` | §19.1 结构对齐 + §19.30 `--history-dir`（默认 glob）`/--output`；golden `history-report.json` |
| §2.6 `compute_project_info` / `project_info` | golden `project-info.json`（§19.18/§19.30，33/33） |
| §2.7 `project_init`（拆行） | golden `project-init.json`（§19.18/§19.30） |
| §2.7 `project_info`（拆行） | golden `project-info.json`（§19.18/§19.30） |
| §2.7 `run_doctor` | golden `doctor.json`（§19.30）+ Python exit 语义对照 |
| §2.7 隐私扫描/打包/恢复 | §19.30 goldens project-privacy/project-privacy-no-outputs/project-pack/project-restore + Python 用同一 Rust bundle restore 成功（双向互操作）+ §19.13 atomic restore 故障注入 |
| §3.2 #29 `POST /api/v1/classes/rotation` | §19.14 rotation 差分 34/34 + §19.7 rotation_gate（逐期 validator、不可行期诚实领域结果）+ §19.1 validator 接线；plan 文档 schema_version 差异登记于 §4.1 rotation-plan 行 |
| §4.1 `seating-snapshot.schema.json` | `dto::snapshot::SeatingSnapshotArtifact` 全字段镜像 + candidate-set oracle golden 内嵌 snapshot 全量解析 0 失败（candidate_dto_fixtures.rs，§19.18） |
| §4.1 `student.schema.json` | `dto::student_roster::RosterStudent` 全 10 字段镜像（含 gender/notes/attributes）+ golden 内嵌 students 解析 0 失败（§19.18） |
| §4.1 `classroom-layout.schema.json` | `dto::classroom_layout` 全镜像 + core `Seat` 已含 zone/group_id/near_*（models.rs:86-96）+ golden 内嵌 layout 解析 0 失败（§19.18） |
| §9 构建 `load_history_snapshots`/`build_seat_history`/`build_pair_history` | core reports 路径 + goldens（§19.18/§19.30）+ relation_totals 构建语义 rotation 差分 34/34（§19.14） |
| §11 文件级编辑 `edit_snapshot` / `project_edit`（CLI） | golden `edit.json`/`project-edit.json`（§19.30） |
| §11 受约束重解 `compute_repair` / `repair_snapshot` / `project_repair` | §19.18 + §19.30 goldens repair×3/project-repair |
| §13 迁移 CLI（schema migrate） | golden `schema-migrate.json`（§19.18/§19.30）+ §19.12 StudentRoster/ClassroomLayout 类型化迁移 |

**状态变更至 `INTENTIONALLY_REMOVED_V2` / `RUST_PARITY_PENDING`（4 行 + §18 登记扩展）**：

- §1.3 与 §14 `seattrellis-desktop`（独立 argparse CLI）、§14 `seattrellis desktop`
  （pywebview 桌面壳）→ `INTENTIONALLY_REMOVED_V2`：与 `desktop` 同一 pywebview
  桌面壳，PD-D15 移除红线覆盖（§18 登记表新增 1 行，理由/迁移/影响同 `desktop`）。
- §14 Tauri 壳能力、§14 原生文件打开/保存对话框 → `RUST_PARITY_PENDING`：§19.22 C1
  已实现（2 个 `#[tauri::command]` read_user_file/write_user_file + tauri-plugin-dialog
  + `POST /api/v1/files/read` 可信根相对路径 + `GET /api/v1/files/root`；server 86
  测试 + E2E 路径读取闭环与绝对路径拒绝）；PD-D14 三入口融合落地，无 Python golden
  （pywebview 桥 v1 专属，形态不同）。

**拆分行（§2.7，+2 行）**：`init_demo`/`project_init` → `init_demo`（`INTENTIONALLY_REMOVED_V2`，
§18）+ `project_init`（`RUST_VERIFIED`）；`project_info`/`project_validate` →
`project_info`（`RUST_VERIFIED`）+ `project_validate`（`RUST_PARTIAL`）。

**仍为 `RUST_PARTIAL`（24 行）逐行判定**（§8.3 条件 1：无 `RUST_PARTIAL` 的 v2
必须项；必须项 = v2 产品/CLI/工作台实际消费且无"双侧同近似/已决策降级"证据的能力，
须 alpha 退出前补自动证据）：

| 行 | 剩余缺口 | alpha.2 判定 |
|---|---|---|
| §2.7 产物对比 artifacts/compare | io `compare_artifacts_json` 字段子集（projects.rs:1786-1877），React 工作台在用（ProjectWorkspacePanel.tsx:411），server 契约测试已过；无 Python golden | **必须项**（补 golden 或经产品决策降级） |
| §2.7 产物恢复 artifacts/restore | `restore_artifact_json` 实现 + §19.13 rollback 已验收；revision/provenance 全契约无 Python golden；React 在用（ProjectWorkspacePanel.tsx:429-435） | **必须项** |
| §3.2 #30 `POST /api/v1/projects/artifacts/compare` | 同 §2.7 compare | **必须项** |
| §3.2 #31 `POST /api/v1/projects/artifacts/restore` | 同 §2.7 restore | **必须项** |
| §13 项目产物对比/恢复（artifacts compare/restore） | 同上（合并行） | **必须项** |
| §2.10 `suggest_roster_mapping` / §5 自动推断列映射（2 行） | `suggest_mapping`/`looks_like_identifier`/`looks_like_person_name` 实现存在（roster.rs:452/:675/:689），表头别名 roster_alias_mirror 锁死（§19.18）；**启发式判定结果无 Python 差分**；B8 导入映射为 v2 核心路径 | **必须项**（补代表性差分语料） |
| §1.1/§2.3 `validate`/`run_validate`（2 行） | `--history-dir` 未镜像；缺 student_id 警告在当前 CoreSolveRequest 形态不可表示 | 选项级/契约形态差距；有已知缺口故不得 verified（§19.33） |
| §1.1/§2.1/§2.3 `project-validate`/`project-solve`/`project-export`（4 行） | 默认路径已 golden（§19.30）；`--strict`/`--candidates`/`--report`/`--candidate` 选项面未镜像（parse_project_command main.rs:729-786） | 选项级（§19.30 已登记"选项级差距，golden 已覆盖默认路径"） |
| §2.3 `list_teacher_goals`/`get_teacher_goal`/`resolve_teacher_goal` | 4 goal vs Python 15 preset（goal_rules.rs:14-19）；goal JSON 不含 hard/groups（goal_rules.rs:6-9 有意省略）；§19.15 capability 对账为 Rust 侧自洽 | 选项级/非必须项（v2 规则编辑走 D3 句式模板 + 规则 JSON，presets 已移除 PD-D15） |
| §4.1 `project.schema.json` | io `ProjectFile` 仍为字段子集（projects.rs:179-190）；schema 层 `SeatTrellisProjectArtifact` 全镜像 DTO 已存在但无运行时消费；project kind 的 schema-migrate CLI 迁移仍报错（migration.rs:285-290） | 选项级（项目生命周期主路径已全绿；default_* 字段运行时读写属 M5-B 班级级默认值增量） |
| §5 映射校验/模板生成/模板应用 | `mapping_issues` 校验存在；模板生成/模板应用无对应 | 非必须项（v1 导入辅助语义，v2 导入流程无模板入口） |
| §5 `roster_fingerprint` | 无对应（roster.rs/server.rs 无 fingerprint 实现） | 非必须项（防无变更提交为 v1 服务语义；v2 预览/应用事务已有冲突检测） |
| §8 `cooling` | 两语言同为近似（`cooling_period`→`lookback`），无法表达"冷却期内完全禁配"强语义 | 选项级/决策项（双侧语义一致，强化需 v2 产品决策，§19 已登记） |
| §12.2 模板默认 report | §19.27 缺省模板统一 teacher（public 强制匿名）；report 与 teacher 渲染仍无差异（renderer 不画分数，"显分数"无法表达） | 选项级（渲染能力差异；若产品要求 report 模板显分数需新增渲染） |
| §12.2 public「🔒 班级公示版」徽标 | print_html.rs 无该徽标（print-html 版式含页脚溯源 seed 标注，§19.19 A2） | 选项级（视觉元素差异） |
| §12.3 locale（zh/en） | 仅影响匿名占位符（export.rs:441）与 print-html lang（print_html.rs:119） | 选项级（v2 UI 语言由前端 i18n 承担） |

**仍为 `PYTHON_ONLY`（2 行）**：§10 计划比较报告生成、§12.1 候选集比较报告——
无报告生成/渲染对应（typed DTO 解析已 `RUST_VERIFIED`，§4.1/§19.18）；v2 候选比较由
B5 候选面板 UI（/audit 端点 + plan_score，§19.20）承担，可打印报告为 v1 专属、无移除
决策 → **选项级/非必须项**（不阻断 alpha；如需 v2 保留可打印报告，属新功能设计）。

**结论（已被 §19.33/§19.34 后续证据更新）**：升级 18 行 `RUST_VERIFIED`、6 行 `RUST_PARITY_PENDING`、2 行
`INTENTIONALLY_REMOVED_V2`（§18 登记扩展 1 行），拆分行 +2（§2 由 39 → 41 行）。
剩余 **25 行**（23 `RUST_PARTIAL` + 2 `PYTHON_ONLY`）中，**v2 必须项 7 行**
（artifacts compare/restore ×5、suggest_roster_mapping 启发式 ×2），均为“缺自动等价
证据”型；rotation-plan schema_version 字段保真已于 §19.34 关闭；
**选项级/非必须项 18 行**。§15 计数随本对账更新为 200 行：
`PYTHON_ONLY=2`、`RUST_PARTIAL=23`、`RUST_PARITY_PENDING=102`、
`RUST_VERIFIED=64`、`INTENTIONALLY_REMOVED_V2=9`。§19.33 关闭 pair-report lookback，
§19.34 关闭 rotation-plan schema_version；仍有 7 行 `RUST_PARTIAL` v2 必须项，因此计划
§8.2“所有 v2 必须项 `RUST_VERIFIED`”**未达成**；CLI 面
（§19.30）与候选/导出 golden（§19.31）两项 alpha.2 清单项已关闭。

### 19.33 2026-08-12：CLI 参数全量枚举（279 用例）与三类真 bug 修复；pair-report lookback 语义对齐

1. **CLI 参数组合全量枚举**（`crates/seattrellis-cli/tests/cli_arg_sweep.rs`，3 个
   集成测试，279 用例，覆盖全部 28 个命令）：no-args/`--help`/未知 flag/
   缺值 flag/重复 flag/二进制垃圾输入/最小合法调用 + 每命令专属用例
   （坏 `--time-limit`/`--seed`/`--count`/`--periods`/`--top`、`--in-place
   --dry-run` 互斥、`--format bmp`、`=` 语法、位置参数、不存在路径、
   `--strict`、`--ignore-saved-locks`、非 UTF-8 argv 等）。断言：用法错误
   exit 2 + stderr 带 `error:`（M1-03 冻结表）；合法调用 exit 0 + stderr
   空；垃圾输入不 panic、exit 在冻结集 {0,2,3,4,5,70,130} 内。运行时长
   ~6s，用例失败聚合为一个断言输出。
2. **修复 1：用法错误退出码 1 → 2**。`main.rs` 解析失败原返回
   `ExitCode::FAILURE`（=1），1 不在冻结集内——107/279 用例红。改
   `ExitCode::from(2)`（InvalidInput）。`fixtures/cli-goldens/audit.json`
   重录（exit 1→2），33/33 复验 0 mismatch。
3. **修复 2：solve/project-solve 输入错误误分类 InternalError(70)**。
   `classify_solve_error`（solver.rs:162-179）不认识 CLI/io 表面消息
   （"cannot read"/"not valid JSON"/"project file not found"/"no such
   file"/"could not read"），垃圾输入 exit 70。INVALID_TOKENS 扩到 18
   个，与 `validate` 一致 exit 2；垃圾二进制输入实测 exit 2。
4. **修复 3：repair/project-repair 拒绝 `solve --output` 的
   CoreSolveResponse 形状**（"missing assignments"）。新增
   `parse_repair_snapshot_assignments` 双形状解析：editor 风格
   `assignments`（对象数组，history 报告共用原函数）+ `assignment`
   （`[student_index, seat_index]` 对，按 request 解析）；sweep 用例
   `project-repair:solve-output-snapshot` 升级为 Kind::Valid 锁定回归。
   实测：`solve --output` → `repair --snapshot` 同输出成功。
5. **pair-report `recent_occurrences` 窗口语义对齐（§19.3.3 关闭）**：
   Python oracle `StudentPairHistory.recent_occurrence_count`
   （models/history.py:138）对 **pair 自身 records** 取 `[-lookback:]`
   并 cap；旧 Rust 用全局快照窗口（`snapshot_index >= len-lookback+1`），
   pair 缺席最近窗口时数值错误。改为
   `records.len().min(PAIR_REPORT_RECENT_LOOKBACK)`，新增边界回归
   `crates/seattrellis-core/tests/pair_report_lookback.rs`（2 测试：仅旧
   快照出现 1 次的 pair → recent=1；6 次全现 → cap=4）。**live 差分**
   （6 快照边界用例，Python CLI `pair-report` vs Rust CLI，同输入）：
   pair totals 1/5/6 与 Python 逐项相等，recent_occurrences 遵循
   Python 约定；41 fixtures 七状态差分 0 mismatch、CLI golden 33/33
   0 mismatch（含 pair-report.json）。
6. **并发观察（登记，未修）**：两个并发 CLI 事务共享同一 journal 目录时，
   一方的恢复扫描可能回滚另一方已 stage 的 temp（sweep 并行线程下
   `export` 偶发 exit 70 "cannot publish temp file"）。sweep 已按测试
   划分独立 journal 目录规避；事务层并发互斥列入 M7 候选。
7. **状态变更**：`validate`/`run_validate` 仍有 `--history-dir` 与
   student-id 警告形态缺口，回退为 `RUST_PARTIAL`；§2.6
   `compute_pair_report`/`run_pair_report`（行 163）、
   §9 报告两行（行 379/380）由 §19.32 暂挂的 `RUST_PARITY_PENDING`
   回 `RUST_VERIFIED`（证据：边界单测 + live 差分 + 41/33 全绿）。
   计数更新：200 行 `PYTHON_ONLY=2`、`RUST_PARTIAL=24`、
   `RUST_PARITY_PENDING=102`、`RUST_VERIFIED=63`、
   `INTENTIONALLY_REMOVED_V2=9`。
8. **no-Python runtime gate 参数**：`--binary`/`--archive` 改为 argparse
   `extend + nargs="+"`，同时支持一次传多路径和重复 flag，避免后一组
   覆盖前一组；tree/binary gate 三种调用形态均通过。

验收使用仓库锁定的 Rust 1.88（本机 `/usr/local/bin` 默认 rustc 1.85
不作为代码失败证据）。已运行 `cargo fmt --all -- --check`、
`cargo test --locked -p seattrellis_core -p seattrellis_cli`、两 crate `--all-targets`
clippy `-D warnings`、CLI 参数扫描、no-Python tree/binary gate（含重复 flag）与
tracked-files 仓库卫生检查，全绿；未运行本轮无关的 release-only
长跑/ignored gates。

### 19.34 2026-08-12：rotation-plan schema_version 对齐 oracle（"0.2.2" → "1.0"）

§19.32 对账发现并修复：Rust 旋转工件写 `schema_version: "0.2.2"`
（application/rotation.rs，自 candidate-set 契约误拷贝），而 Python oracle
写 `ROTATION_PLAN_SCHEMA_VERSION = "1.0"`（schema.py:14）、v1
`rotation-plan.schema.json` 亦声明 default "1.0"——Rust 写出的 rotation-plan
对 oracle schema 无效（Python `require_schema_version` 会拒绝）。修复：
rotation.rs 写 "1.0"；读取侧（io/server）无版本强校验，不受影响。验证：
rotation_gate 非 ignored 测试同时锁定 `kind="rotation_plan"` 和版本；
`scripts/rust_python_diff.py` 的 rotation comparator 新增 durable artifact
`kind`/`schema_version` 强制对比，`--rotation` 复跑 **34/34、0 mismatch**。
golden `rotation-3-periods/rotation-plan.json`（Python 生成）本就是 "1.0"，无需重录。

§4.1 `rotation-plan.schema.json` 由 `RUST_PARTIAL` 升为 `RUST_VERIFIED`；
§15 可复算计数更新为 200 行：`PYTHON_ONLY=2`、`RUST_PARTIAL=23`、
`RUST_PARITY_PENDING=102`、`RUST_VERIFIED=64`、`INTENTIONALLY_REMOVED_V2=9`。
alpha.2 仍有 7 行 v2 必须项未 verified（artifacts compare/restore ×5、
roster mapping ×2），不宣称 §8.2 Exit Gate 通过。遗留：schema DTO 测试文档中的
"0.2.2" 字符串为测试数据（`schema_version` 为 String 字段不校验常量），不影响契约。

### 19.36 2026-08-12：roster-mapping 启发式差分 corpus（10 case 全等）

关闭 §19.32 判定必须项中 suggest_roster_mapping 启发式的证据缺口：

- 新增 `fixtures/roster-mapping/`（10 个 CSV + `expected.json`）：
  normal-headers / chinese-aliases / headerless-numeric-id /
  headerless-short-id / duplicate-alias / fuzzy-headers（"StudentID"
  可识别、"Full Name" 不在别名表）/ missing-identity /
  mixed-alias-prefix / headerless-name-only / needs-plus-tags。
- `expected.json` 由 Python oracle `suggest_roster_mapping` 记录
  （assignments `{field: column_index}` + issues code/field/indices）。
- Rust 侧：`crates/seattrellis-io/tests/roster_mapping_parity.rs` 对
  每个 case 跑 `parse_roster_csv` → 建议映射，与 golden 逐字段全等
  （含 issue 顺序与 column_indices）——**10/10 0 差异**。
- Python 侧：`tests/test_roster_mapping_parity.py` 守卫 golden 与
  oracle 同步（实现变更必须重录 golden 并两侧同验）。
- 状态变更：§2.10 `suggest_roster_mapping` 与 §5 自动推断列映射
  两行 `RUST_PARTIAL` → `RUST_VERIFIED`。计数：200 行
  `PYTHON_ONLY=2`、`RUST_PARTIAL=21`、`RUST_PARITY_PENDING=102`、
  `RUST_VERIFIED=66`、`INTENTIONALLY_REMOVED_V2=9`。

## 附：M0 收口——oracle golden corpus 与差分 harness（2026-08-08）

### corpus 状态

- `fixtures/parity/`：41 个 case（34 合法 + 7 invalid），inputs 148 文件、
  goldens 192 文件（2026-08-12 §19.31 candidates golden 扩展后 inputs 149、
  goldens 195）；`MANIFEST.json` 记录每个文件的 SHA-256 与字节数、生成
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
- candidates golden：n=50/60/80 的 1/5/20 组合已扩展（§19.31）——
  p50×{1,5,20}、p60×{1,5}、p80×{1} 提交字节稳定 golden（candidates-cNN.json +
  plan-report/objective-breakdown）；p60×20 与 p80×5/20 因确定性预算超出 CI
  parity-oracle 重放时限（实测成本见 §19.31），由 `--candidates` live 差分
  （§19.6，15 combos）与 Rust candidates gate 测试覆盖，未提交 golden。

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
