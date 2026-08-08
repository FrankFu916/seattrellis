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

## 已知陷阱

- `app/src-tauri/rust-toolchain.toml` 锁定 1.88.0（Tauri 依赖要求），其余 crate 声明 MSRV 1.83——两者不一致是已知问题，按计划 M1 统一为 1.88。
- `scripts/rust_python_diff.py` 做差分；**任何 Python error 都不能记为 INFEASIBLE**（M0-03 已冻结此语义），mismatch 必须非零退出。
- `outputs/`、`dist/`、`site/`、`node_modules/`、`target/` 是构建产物。**禁止提交真实学生数据/名单/成绩**（README 明确要求）。
- 本地 loopback API 目前无 session/token/Host 校验（P0 风险，M1-05 修复）；新增写路径时不要绕过安全中间件设计。
- **M1-04 已落地**：HTTP 层为 axum/hyper/tokio（`app/src/http.rs` 适配层 → `server::route` 分发，52 个路由测试原样通过）；body 限制 64MiB（413，旧 411 怪癖已废弃）、并发上限 64、SIGINT/SIGTERM/Tauri 退出均可优雅停机；multipart 解析与路径防护逻辑保留。`Server::serve` 仍是阻塞签名（内部 tokio runtime）。
- 新文件、新命令或行为变更后，检查是否需要同步更新 `docs/` 与 parity ledger，CI 会跑 `scripts/check_repository_hygiene.py` 等检查。
