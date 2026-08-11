# SeatTrellis (席序) — Workspace Guide

本地优先的课堂排座工具。当前处于 **v1.9.x 稳定线 → v2.0.0 Rust-only 迁移**阶段：
Python 是 oracle（行为基准），Rust 是 v2 目标实现，React 仅是展示层。**所有迁移工作必须服从《v2.0.0 开发与发布总计划》，不允许偏离。**

## 必读文档（改动敏感区域前必须读）

- **`docs/SeatTrellis_v2.0.0_开发与发布总计划_修订版.md`** — 最高优先级计划书：里程碑 M0–M7、发布门禁、冻结契约、质量门槛。涉及 solver / schema / migration / privacy / security / parity 的任何改动先查对应里程碑。
- **`docs/v2-parity-ledger.md`** — Python↔Rust 能力对账本（M0 基线 `282fd99`）。状态只能是 `PYTHON_ONLY / RUST_PARTIAL / RUST_PARITY_PENDING / RUST_VERIFIED / INTENTIONALLY_REMOVED_V2`；改动状态必须更新本文件并注明 commit。禁止把"路径存在"当"parity 完成"。
- **`docs/adr/`** — 架构决策记录（含 supersede 关系，如 0001 已被 v2 计划 supersede）。

## 目录结构

| 路径 | 内容 | 角色 |
|---|---|---|
| `Cargo.toml` / `Cargo.lock` | 单一 Rust workspace、单一 lockfile、MSRV 1.88；PyO3 crate 不在 `default-members` | v2 生产构建根 |
| `crates/` | `schema/rules/domain/application/io/export/server` 分层 crate | v2 应用与传输主路径 |
| `src/seattrellis/` | Python v1.x | **oracle，只读参考**；不为其扩展 v2 功能 |
| `native/` | `seattrellis_core`、`seattrellis_cli`、PyO3 临时兼容 crate `seattrellis_native` | v2 core/CLI 与迁移兼容 |
| `app/` | 薄 `seattrellis_app` facade，复用 `seattrellis-server` | v2 服务启动器 |
| `app/src-tauri/` | 单一 workspace 成员中的 Tauri 2 薄壳，toolchain 锁定 1.88.0 | v2 桌面 |
| `clients/web/` | React 19 + Vite + vitest | 展示层 |
| `schemas/` | v1 oracle schema + `xtask` 由 Rust DTO 生成的 `*.v2.schema.json` | 契约 |
| `fixtures/parity/` | golden parity corpus（`MANIFEST.json` + `inputs/` + `goldens/`，由 `scripts/gen_parity_fixtures.py` 生成） | 验证 |
| `e2e/` `e2e-rust/` | Streamlit 浏览器验收；NO_PYTHON_RUNTIME 工作台 E2E（`web-e2e-rust` CI job，Python 仅作 runner，不安装包） | 验证 |
| `docs/` `scripts/` `tests/` | 文档、dev/benchmark/diff 脚本、Python pytest 套件 | 支撑 |

## 构建与测试

```bash
# 在根 workspace 执行（三平台 CI 用 --locked）
cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis_cli
cargo clippy --all-targets -p seattrellis_core -p seattrellis_cli -- -D warnings

# Rust App server
cargo test --locked -p seattrellis_app
cargo clippy --all-targets -p seattrellis_app -- -D warnings

# Tauri 壳（需要 1.88 工具链；普通稳定版可能不够）
cargo build --locked -p seattrellis_desktop

# React（在 clients/web/ 下）
npm test          # vitest
npm run typecheck # tsc -b
npm run build     # tsc -b && vite build

# Python oracle（tests/ 下 pytest；安装：pip install -e ".[web]"）
python -m pytest
```

注意：根 workspace 已统一；不要恢复 `native/`、`app/`、`app/src-tauri/` 的独立 workspace/lockfile，也不要在迁移期顺手升级 edition。PyO3 只是 oracle 兼容层，v2 final 前必须删除。

## 架构与分层规则（违反即偏离计划）

- **Rust 是唯一语义真相**：规则编译/合法性、编辑状态机、migration、privacy、求解状态只由 Rust 决定。React（`clients/web/src/domain/generation.ts`、`ruleDiagnostics.ts`、`workflow.ts`）目前自编译规则、自判合法性，是 M6 要删除的违规；**不要扩展这些 TS 逻辑**，只能加展示和输入级检查。
- transport/UI 不得反向进入 domain/rules/solver；`serde_json::Value` 只允许出现在 migration tree、扩展 namespace 和 transport 边界。
- **Solver 状态七元组**：`Solved / ProvenInfeasible / Timeout / Unknown / InvalidInput / Cancelled / InternalError`。贪心等启发式耗尽只能是 `Unknown`，绝不能伪装成 `ProvenInfeasible`；有合法 incumbent 时即使超时也是 `Solved`。CLI v2 退出码冻结：0/2/3/4/5/70/130。
  实现路径已有 core `SolveStatus` / `CoreSolveResponse.status`、CLI 退出码和 server 错误映射；仍需按 v2 契约验收 `ProvenInfeasible/Timeout/Unknown` 作为正常领域结果，不得因“响应里有 status”就宣称 API parity。
- 所有 solve/edit/repair/rotation/export 产物必须经**独立 validator** 复核；禁止硬编码 `feasible=true`。
- 发布红线：v2 final 生产包不得含 Python、Pydantic、FastAPI、Streamlit、OR-Tools、PyO3、pywebview、Node runtime；安装包 5–20MB 是 release gate，不得靠砍功能达标。

# Oracle parity corpus（M0 收口完成 2026-08-08）

```bash
python scripts/gen_parity_fixtures.py all      # 重新生成 inputs + goldens（约 20 分钟）
python scripts/gen_parity_fixtures.py verify   # 临时目录重生成并逐字节比对（CI parity-oracle job 跑这个）
python scripts/rust_python_diff.py --fixtures  # Python oracle vs Rust CLI 七状态差分（mismatch 非零退出）
```

- 41 个 case（34 合法 + 7 invalid），`MANIFEST.json` 含逐文件 SHA-256；确定性预算为
  `--time-limit 1800`（墙钟截止的 solve 不稳定，短预算会产出不可重放的 golden；
  verify 对截止命中的运行显式 SKIP）。
- 导出 golden：**无任何格式字节稳定**（时间戳嵌入/zlib/zip mtime），只记录语义契约，
  格式与隐私 parity 属修订版 §5.6/M2 未通过项。candidates golden 仅 n≤40；n=50/60/80 与 1/5/20 候选组合是修订版 §6.3/§6.6 M3 Exit Gate 阻断，不得推迟成 M4 任务。
- 账本最后记录的 diff 仍有 unknown rule/soft field 和 bad adjacency 差距；如代码已修复，必须重跑自动差分并在 ledger 登记 fixture 证据，不得用单元测试代替 `RUST_VERIFIED`。

## M1 实现盘点（修订版 §4，2026-08-09）

| 项 | 状态 | 说明 |
|---|---|---|
| §4.1 workspace 收敛 | 路径存在 | 根 Cargo.toml、单一 lockfile、MSRV 1.88、PyO3 隔离出 default-members |
| §4.2 DTO/domain 分离 | 路径存在 | `schema/domain/application/io/export/server` 分层落地；仍以 contract/round-trip 证据判定 parity |
| §4.3 Capability API | 部分实现 | catalog/capability 路径存在；必须与实际 import/export/rule/editor/desktop 能力逐项对账 |
| §4.4 error taxonomy | 部分实现 | 七状态与错误映射存在；`/api/v2` 领域结果与 envelope 仍需契约验收 |
| HTTP/安全基础 | 路径存在 | axum、token/Host/Origin/Bearer/CSP、body/并发限制和优雅停机有测试路径 |

M1 “实现路径存在”不等于自动 parity 或 Final Gate；验收以修订版 §4.5 为准。

## M2 验收状态（修订版 §5，2026-08-09）

| 项 | 状态 | 说明 |
|---|---|---|
| §5.1 Student/roster | `RUST_PARTIAL` | CSV/XLSX、mapping/update 路径存在；模糊表头、增量/覆盖、原子 apply 尚无全量 golden |
| §5.2 Layout | `RUST_PARITY_PENDING` | Rust command/store 路径存在；尚无全量 layout/schema golden |
| §5.3 Project/migration/backup | `RUST_PARTIAL` | registry/schema/transaction/bundle/privacy/compare/restore 路径存在；全 artifact 迁移、全写路径 rollback 与项目生命周期未验收 |
| §5.4 Editing/repair | `RUST_PARTIAL` | editor 与 repair 路径存在；repair 有空座锁等已知差距，且未证明所有产物都走独立 validator |
| §5.5 CLI | `RUST_PARTIAL` | 当前 13 个 Rust 子命令 + help/version，不等于 Python 30 命令契约或完整项目生命周期 |
| §5.6 Export | `RUST_PARTIAL` | SVG/HTML/PNG/PDF 路径存在；Office、print HTML、CJK PDF、页面/隐私语义与独立结构验证未完成 |

`0057a7b` 为上述多个能力增加了实现路径，但未提供逐项 Python↔Rust golden 等价证据；M2 Exit Gate（修订版 §5.7）**未通过**。不得再使用“全关”、“生命周期闭环”或“完成”代替 ledger 证据。

## M3 验收状态（修订版 §6，2026-08-09）

| 项 | 状态 | 说明 |
|---|---|---|
| §6.1 Hard constraints/search | 实现路径存在 | 静态冲突、candidate domain、matching、MRV/backtracking 已实现；取消延迟、exact differential、official/random corpus 尚未完整验收 |
| §6.2 Soft optimization | 实现路径存在 | local search 已实现；三平台确定性、Python fixed-assignment scoring 和性能回归门槛未证明 |
| §6.3 Candidate engine | `RUST_PARTIAL` | 重复求解、去重、distance/seed/recommendation 路径存在；n=50/60/80、1/5/20、PlanScore/stability 与 golden parity 未完成 |
| §6.4 Rule metadata/DSL | `RUST_PARTIAL` | Rust registry 路径存在；React 仍有第二套规则真相，UI 未由 capability/metadata 完整驱动 |
| §6.5 Audit/explanation | `RUST_PARTIAL` | hard status + soft breakdown 路径存在；witness、可操作建议、本地化 key、最大贡献者与缺失数据说明未完整验收 |
| §6.6 Quality gate | **未通过** | 6 个 40/50/60 light/dense 样本的 regret 不等于全量 Gate；还缺 official corpus、随机 ≥99.5%、false-infeasible=0、scoring parity、性能与长跑证据 |

**M3 “实现阶段完成”声明已撤回。** PR #94–#98 只证明若干实现路径和局部测量存在；修订版 §6.7 M3 Exit Gate 未通过。在 M2 §5.7 和 M3 §6.7 的自动证据补齐前，**不得开始修订版 M4 Product Decision/UX 正式实现**，更不得进入 alpha。

### 2026-08-09 post-merge acceptance audit

- `0057a7b` 合并了 repair/reports/project/rotation/privacy 等实现路径；`320b68f` 只更改 ledger 状态，未增加 golden 等价证据。
- 详细阻断项、逐领域所需证据和可复算状态计数见 `docs/v2-parity-ledger.md`。
  2026-08-10 ledger 批量提升后 `RUST_VERIFIED = 25`（328 项自动差分 0 mismatch，§19.14）。
- repair、project lifecycle、history/pair reports、rotation、candidates、privacy/export 全部仍是 `RUST_PARTIAL`/`RUST_PARITY_PENDING` 或 `PYTHON_ONLY`，不得称已完成。

### 2026-08-09 acceptance-fix round（ledger §19）

审计后整改已合入（详见 `docs/v2-parity-ledger.md` §19）：solver 取消协议与
local-search 预算、candidates seed/exclusion/distance 对齐、history/pair
report 字段与 warnings、独立 validator 全产物接线、事务层重写（唯一备份/
路径逃逸防御/父目录 fsync/崩溃恢复）、bundle restore 原子化、CLI
project-export 改为导出已保存 plan（不再重复求解）、`/api/v2/solve` 领域
结果 HTTP 200、契约生成补 rotation requestBody 与 compare/restore、CI
权限收紧 + 完整 Rust↔Python 差分 job、差分 harness 的 3 个 invalid 差距
改为 case 级文档化。

**这些修复不改变 Gate 结论**：M2 §5.7 与 M3 §6.7 仍未通过，`RUST_VERIFIED`
仍为 0。缺口是 golden/长跑/端到端证据（§17.4/§19.4），不是实现路径。
在自动证据补齐前不得开始修订版 M4 正式实现，更不得进入 alpha。

## 已知陷阱

- 根 workspace 已统一 `rust-version = 1.88`；不得在文档或命令中继续假定 `native/app/Tauri` 是独立 workspace。
- `scripts/rust_python_diff.py` 做差分；**任何 Python error 都不能记为 INFEASIBLE**（M0-03 已冻结此语义），mismatch 必须非零退出。
- `outputs/`、`dist/`、`site/`、`node_modules/`、`target/` 是构建产物。**禁止提交真实学生数据/名单/成绩**（README 明确要求）。
- 本地 loopback API 安全边界（M1-05 已落地，勿绕过）：`/api/*` 全部要求 `Authorization: Bearer <token>`（/api/v1/session 引导端点除外）；Host 必须为 loopback 名 + 绑定端口（防 DNS rebinding）；Origin 存在时必须同源（防 CSRF）；响应含 CSP/X-Frame-Options: DENY/Referrer-Policy: no-referrer。token 由 Server 启动时生成（256-bit），Tauri 用 initialization_script 注入 `window.__SEATTRELLIS_SESSION__`，浏览器工作台经 GET /api/v1/session 引导获取。新增写路径时不要绕过这些中间件。
- HTTP 层位于 `crates/seattrellis-server/src/http.rs` / `server.rs`；body 限制、并发上限和优雅停机均不得被新路径绕过。
- 修订版阶段顺序固定为 M3 Exit → M4 Product Decision/UX → M5 alpha → M6 Python retirement/beta → M7 RC。附件中的 M4 candidate/M6 UX 编号不得再写回本指南。
- 新文件、新命令或行为变更后，检查是否需要同步更新 `docs/` 与 parity ledger，CI 会跑 `scripts/check_repository_hygiene.py` 等检查。
