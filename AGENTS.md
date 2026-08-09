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
| `src/seattrellis/` | Python v1.x | **oracle，只读参考**；不为其扩展 v2 功能 |
| `native/` | Rust workspace：`seattrellis_core`（solver/evaluator）、`seattrellis_cli`（validate/solve/export）、`seattrellis_native`（PyO3 临时兼容） | v2 核心 |
| `app/` | Rust loopback HTTP server（`seattrellis_app`，单体 `server.rs` >5000 行，M1 将拆分） | v2 服务 |
| `app/src-tauri/` | Tauri 2 壳（自有 workspace，`rust-toolchain.toml` 锁定 1.88.0） | v2 桌面 |
| `clients/web/` | React 19 + Vite + vitest | 展示层 |
| `schemas/` | 现存 JSON Schema（Python 来源，M2 起改由 Rust 生成） | 契约 |
| `fixtures/parity/` | golden parity corpus（`MANIFEST.json` + `inputs/` + `goldens/`，由 `scripts/gen_parity_fixtures.py` 生成） | 验证 |
| `docs/` `scripts/` `tests/` | 文档、dev/benchmark/diff 脚本、Python pytest 套件 | 支撑 |

## 构建与测试

```bash
# Rust core / CLI（三平台 CI 用 --locked）
cargo test --locked --manifest-path native/Cargo.toml -p seattrellis_core
cargo test --locked --manifest-path native/Cargo.toml -p seattrellis_cli
cargo clippy --all-targets --manifest-path native/Cargo.toml -p seattrellis_core -p seattrellis_cli -- -D warnings

# Rust App server
cargo test --manifest-path app/Cargo.toml
cargo clippy --all-targets --manifest-path app/Cargo.toml -- -D warnings

# Tauri 壳（需要 1.88 工具链；普通稳定版可能不够）
cargo build --manifest-path app/src-tauri/Cargo.toml

# React（在 clients/web/ 下）
npm test          # vitest
npm run typecheck # tsc -b
npm run build     # tsc -b && vite build

# Python oracle（tests/ 下 pytest；安装：pip install -e ".[web]"）
python -m pytest
```

注意：`native/`、`app/`、`app/src-tauri/` 是**三个独立 workspace**（M1-01 才会统一）；`app` 通过 path 依赖 `native/seattrellis_core`，不要随手调整 workspace 边界或 edition（计划要求迁移期间保持当前 edition）。

## 架构与分层规则（违反即偏离计划）

- **Rust 是唯一语义真相**：规则编译/合法性、编辑状态机、migration、privacy、求解状态只由 Rust 决定。React（`clients/web/src/domain/generation.ts`、`ruleDiagnostics.ts`、`workflow.ts`）目前自编译规则、自判合法性，是 M6 要删除的违规；**不要扩展这些 TS 逻辑**，只能加展示和输入级检查。
- transport/UI 不得反向进入 domain/rules/solver；`serde_json::Value` 只允许出现在 migration tree、扩展 namespace 和 transport 边界。
- **Solver 状态七元组**：`Solved / ProvenInfeasible / Timeout / Unknown / InvalidInput / Cancelled / InternalError`。贪心等启发式耗尽只能是 `Unknown`，绝不能伪装成 `ProvenInfeasible`；有合法 incumbent 时即使超时也是 `Solved`。CLI v2 退出码冻结：0/2/3/4/5/70/130。
  **M1-03 已落地**：core `SolveStatus` + `CoreSolveResponse.status`、`classify_solve_error()`、CLI 退出码表与 server 409/400 的 status 字段均已实现并有契约测试；完整 /api/v2 error envelope 待 M1-06。
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
  字节稳定性是 M5-04 项。candidates golden 仅 n≤40（M4-03 补大班候选引擎）。
- 已知 Rust 差距（diff harness 暴露）：无时间预算（M3-04）、不校验重复学生 key、
  忽略未知规则字段、CLI 无法表达坏邻接布局——均纳入 M2/M3 修复清单。

## M1 里程碑状态（2026-08-08）

| 项 | 状态 | 说明 |
|---|---|---|
| M1-01 统一 workspace | ✅ 合并 | 根 Cargo.toml、单一 lockfile、MSRV 1.88、PyO3 隔离出 default-members |
| M1-03 SolveOutcome | ✅ 合并 | 七状态 enum + CLI 退出码表 + server status 字段（详见上文） |
| M1-04 axum | ✅ 合并 | 适配层 `app/src/http.rs`，52 路由测试零改动；413/并发 64/优雅停机 |
| M1-05 安全边界 | ✅ 合并 | token/Host/Origin/Bearer/CSP（详见上文） |
| M1-06 契约生成链 | ✅ 合并 | xtask + OpenAPI/TS codegen + CI drift |
| M1-02 crate 拆分 | ✅ 完成 | 9 个 crate 分层落地：server(transport)→application→{domain,io,export,core}；app 为薄 facade，Tauri 壳直连 seattrellis-server（PR #92/#93） |

## M2 里程碑状态（2026-08-08）

| 项 | 状态 | 说明 |
|---|---|---|
| M2-01 Artifact Registry | ✅ | `crates/seattrellis-schema`：v2 envelope {kind, schema_version, data, extensions} + 12 种 artifact kind + 严格解析（未知字段拒绝） |
| M2-02 Rust 生成 JSON Schema | ✅ | `xtask contract schemas` → `schemas/*.v2.schema.json`，metaschema 校验 + CI drift |
| M2-03 v1→v2 migration graph | ✅ | typed 步骤 + lossless 契约 + 源/目标 SHA-256；未知旧字段阻断迁移 |
| M2-04 事务性仓库 | ✅ 核心 | `app/src/transaction.rs` journaled 多文件事务 + 故障注入测试（接入 batch apply 为后续） |
| M2-05 Bundle Manifest v2 | ✅ | 每文件 size/sha256/kind/version + 路径安全校验（zip corpus 为后续） |
| M2-06 统一 privacy policy | ✅ | Safe/Unsafe/Indeterminate + 敏感字段单一来源（敏感键与 Python oracle 对齐） |

M2 follow-up 已完成（PR #99-#103）：migration batch 接入 FileTransaction；zip-bomb/symlink corpus；RuleSet/Snapshot/Project DTO + 6 个 v2 Schema；rotation 生成器（A.1）；artifact compare/restore（A.2/A.3）；history-report/pair-report（B.5）；repair（D.11）；project-info/validate/solve/export（D.12，项目生命周期闭环）；export 隐私粒度（C.8）；CI cargo fmt + cargo-deny（E.18）；pyi 补 solve_problem（E.17）。ledger A 类全关、B 类主项全关。

M1 Exit Gate 核查：workspace 统一 ✅ / PyO3 隔离 ✅ / 未认证、DNS rebinding、恶意 Origin、超大 body 全部拒绝并有测试 ✅ / 退出无残留 ✅。

## M3 里程碑状态（2026-08-09）

| 项 | 状态 | 说明 |
|---|---|---|
| M3-01 RuleSpec registry | ✅ | `crates/seattrellis-rules`：15 条官方规则规格（params/spec/registry JSON + schema） |
| M3-02 静态冲突 + 候选域 | ✅ | core validate 镜像 Python 严格编译：重复固定座位/must∩cannot/固定违反 pair 规则全在搜索前拒绝；`build_candidate_domains` 输出每学生候选域 + 排除原因；空域 = sound ProvenInfeasible |
| M3-03 匹配预检 | ✅ | Kuhn 二分图最大匹配；匹配 < 学生数 = sound ProvenInfeasible（Hall）；匹配满不证可解（归 M3-04） |
| M3-04 hard search | ✅ | MRV + degree tie-break + forward checking 回溯（200k 节点预算）；Found→Solved / 全枚举→ProvenInfeasible / 预算耗尽→Unknown（honest）；greedy 优先、失败才搜索 |
| M3-05 独立 validator | ✅ | `validate_assignment` 在两条 Solved 路径出口复核 uniqueness/边界/全部硬规则；违规 → InternalError 而非静默 feasible |
| M3-06 feasibility report | ✅ | `precheck_report_json` + CLI `precheck` 子命令：候选域/排除原因/最紧张学生/匹配大小/clean|infeasible+原因（PR #94） |

M3 已合并 PR #94（预检四层+validator+report）、#95（时间预算，TIMEOUT gap 关闭）、#96（6.2 local search，40 人 case cost 改进 31.7%）、#97（6.5 audit：hard 规则状态 + soft breakdown + score_balance 显式化，CLI `audit` 子命令）。
M3 全部完成（PR #94/#95/#96/#97/#98 + 6.6 测量）：6.3 candidate engine 已合并（PR #98，seeded 重复求解 + 精确 assignment 排除 + distance_to_best/seed 派生/recommendation）。
6.6 质量门槛测量（scripts/measure_rust_quality.py，OR-Tools 30s 预算）：6 个 case（40/50/60 × light/dense）Rust 全部优于 OR-Tools，regret 中位数 **-13.84%**、P95 **-9.28%**（门槛 中位数≤5%、P95≤15%，PASS；负值 = Rust 更优）。

## 已知陷阱

- `app/src-tauri/rust-toolchain.toml` 锁定 1.88.0（Tauri 依赖要求），其余 crate 声明 MSRV 1.83——两者不一致是已知问题，按计划 M1 统一为 1.88。
- `scripts/rust_python_diff.py` 做差分；**任何 Python error 都不能记为 INFEASIBLE**（M0-03 已冻结此语义），mismatch 必须非零退出。
- `outputs/`、`dist/`、`site/`、`node_modules/`、`target/` 是构建产物。**禁止提交真实学生数据/名单/成绩**（README 明确要求）。
- 本地 loopback API 安全边界（M1-05 已落地，勿绕过）：`/api/*` 全部要求 `Authorization: Bearer <token>`（/api/v1/session 引导端点除外）；Host 必须为 loopback 名 + 绑定端口（防 DNS rebinding）；Origin 存在时必须同源（防 CSRF）；响应含 CSP/X-Frame-Options: DENY/Referrer-Policy: no-referrer。token 由 Server 启动时生成（256-bit），Tauri 用 initialization_script 注入 `window.__SEATTRELLIS_SESSION__`，浏览器工作台经 GET /api/v1/session 引导获取。新增写路径时不要绕过这些中间件。
- **M1-04 已落地**：HTTP 层为 axum/hyper/tokio（`app/src/http.rs` 适配层 → `server::route` 分发，52 个路由测试原样通过）；body 限制 64MiB（413，旧 411 怪癖已废弃）、并发上限 64、SIGINT/SIGTERM/Tauri 退出均可优雅停机；multipart 解析与路径防护逻辑保留。`Server::serve` 仍是阻塞签名（内部 tokio runtime）。
- 新文件、新命令或行为变更后，检查是否需要同步更新 `docs/` 与 parity ledger，CI 会跑 `scripts/check_repository_hygiene.py` 等检查。
