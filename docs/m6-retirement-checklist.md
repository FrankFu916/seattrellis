# SeatTrellis v2 — M6 beta.1 Python Retirement 执行清单（2026-08-13 草案）

> 状态：**草案**——v1.9.0 tag 与 v1.x-maintenance 分支建立后方可执行
> （计划 §9.1 顺序：先保护 v1 线，再删 Python）。
> 依据：`SeatTrellis_v2.0.0_开发与发布总计划_修订版.md` §9.1、
> ledger §19.18（M6 前置解耦已确认 Rust 侧无 Python 依赖）。

## 0. 前置（已完成，2026-08-12/13）

- [x] server/desktop 与 Python 树解耦（resolve_web_root 仅 clients/web/dist，
      commit c4ba074/后续）
- [x] M6 dry-run 验证：删除 src/seattrellis 后构建成功（e28bf18）
- [x] no-python-runtime CI 门禁（`--expect-retired` 为 M6 硬门禁，
      check_no_python_runtime.py）
- [x] 迁移指南按真实 Rust 实现重写（docs/migration-v1-to-v2.zh.md）
- [x] golden provenance 体系（fixtures/GOLDEN_PROVENANCE.json + guards）

## 1. 建立 v1 保护线（v1.9.0 冻结后）

- [ ] annotated tag `v1.9.0`（同一 commit 上）
- [ ] 分支 `v1.x-maintenance`（同一 commit）
- [ ] GitHub branch protection：v1.x-maintenance 要求 PR + 状态检查
      （Rust/Tests/hygiene），禁止直接 push
- [ ] `final_v1_reference` 在 GOLDEN_PROVENANCE 中解析为实际 commit/tag
- [ ] 记录 tag 指纹（`git rev-parse v1.9.0^{}`）到 docs/versioning.md

## 2. Python 删除面（v2 主线，M6 beta.1）

| 路径/文件 | 内容 | 验证 |
|---|---|---|
| `src/seattrellis/` | Python oracle 包 | 删除后 `cargo build --release` 全绿；no-python `--expect-retired` 通过 |
| `native/` | PyO3 兼容 crate（seattrellis_native） | 从 workspace members 移除 + Cargo.lock 更新 |
| `pyproject.toml` | Python packaging | 删除；CI tests.yml Python 各 job 停用或改 oracle-only |
| `tests/` | Python pytest 套件 | 迁移到 `oracle-tests/` 或随维护线保留（决策：v2 主线删除，oracle 证据以 fixtures/goldens 固化） |
| `e2e/` | Streamlit 浏览器验收 | 删除（v2 用 e2e-rust） |
| `scripts/*.py` | 差分/生成脚本 | **保留**（dev 工具，用 venv Python 运行 oracle 差分；CI parity job 依赖） |
| `.github/workflows/tests.yml` | Python 测试矩阵 | 缩减为 parity-oracle + differential job（Python 仅作 runner/测试对象身份） |
| `.github/workflows/desktop.yml` | 旧 Python 桌面构建 | 删除 |
| `benchmarks/` 内 Python 脚本 | 基准 | 保留（oracle 基准已固化 JSON） |
| Streamlit/pywebview/OR-Tools 引用 | 代码/文档 | grep 清零（生产树） |

## 3. 删除顺序与验收

1. **先**建 tag/分支/保护（§1）——任何删除发生在 v1 保护线之后
2. 逐个删除（每步 `git commit` 独立，便于回滚审查）
3. 每步后跑：`cargo test --workspace`、`clippy -D warnings`、
   `check_no_python_runtime.py --tree --expect-retired`、
   `cargo run -p xtask -- contract check`
4. 删除完成后跑完整 release gate：Rust 三平台 + Tests
   （parity-oracle 用 venv 运行，不装包）+ E2E + 打包验证
5. `scripts/check_repository_hygiene.py` 增加断言：生产树无
   `src/seattrellis`/`native`/`pyproject.toml`（M6 后成为硬性检查）

## 4. 明确保留（非删除）

- `fixtures/`（oracle golden corpus——差分证据，不依赖运行时 Python）
- `scripts/gen_parity_fixtures.py`、`rust_python_diff.py`（dev 工具）
- `docs/` 中 v1 历史说明（计划 §9.1：允许文档保留历史说明）
- `e2e-rust/`、`clients/web/`、`crates/`、`app/`、`schemas/`

## 5. beta.2 稳定化（§9.2）

删除完成后进入严格 feature freeze：只允许 bug/性能/安全/可访问性/
migration-export 正确性修复与文档修正；禁止新规则/新插件/新数据模型/
新大 UI/大规模重写。每个 fix 必须带 regression test（RC 冻结要求 §10.1）。
