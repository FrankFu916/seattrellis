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
| `crates/` | `schema/rules/domain/application/io/export/server/core/cli` 分层 crate（core=solver/evaluator/validation/candidates/audit 宿主；cli=CLI adapter） | v2 应用与传输主路径 |
| `src/seattrellis/` | Python v1.x | **oracle，只读参考**；不为其扩展 v2 功能 |
| `native/` | 仅 PyO3 临时兼容 crate `seattrellis_native` | 迁移兼容（v2 final 前必须删除） |
| `app/` | 薄 `seattrellis_app` facade，复用 `seattrellis-server` | v2 服务启动器 |
| `app/src-tauri/` | 单一 workspace 成员中的 Tauri 2 薄壳，toolchain 锁定 1.88.0 | v2 桌面 |
| `clients/web/` | React 19 + Vite + vitest | 展示层 |
| `schemas/` | v1 oracle schema + `xtask` 由 Rust DTO 生成的 `*.v2.schema.json` | 契约 |
| `fixtures/parity/` | golden parity corpus（`MANIFEST.json` + `inputs/` + `goldens/`，由 `scripts/gen_parity_fixtures.py` 生成） | 验证 |
| `e2e/` `e2e-rust/` | Streamlit 浏览器验收；NO_PYTHON_RUNTIME 工作台 E2E（`web-e2e-rust` CI job，Python 仅作 runner，不安装包） | 验证 |
| `docs/` `scripts/` `tests/` | 文档、dev/benchmark/diff 脚本、Python pytest 套件 | 支撑 |

## 构建与测试

```bash
# 前端必须先构建：server 的 build.rs 内嵌 clients/web/dist（M6 解耦），
# 任何拉入 seattrellis-server 的 workspace 级构建（test/clippy/app）都会因缺失而 panic
cd clients/web && npm ci && npm run build && cd ../..

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
  格式与隐私 parity 属修订版 §5.6/M2 未通过项。candidates golden 已扩展至 n=50/60/80 的
  1/5/20 组合（ledger §19.31：p50×{1,5,20}、p60×{1,5}、p80×{1} 提交字节稳定 golden；
  p60×20/p80×5/p80×20 因确定性预算超出 CI 重放时限，由 live 差分 15 combos 与 Rust
  candidates gate 测试覆盖）。
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

## M2 验收状态（修订版 §5，2026-08-10 更新）

| 项 | 状态 | 说明 |
|---|---|---|
| §5.1 Student/roster | `RUST_PARITY_PENDING` | CSV 导入、字段映射、差异预览、增量/覆盖路径存在；模糊表头/拼音等增强项无 golden（M4 候选） |
| §5.2 Layout | `RUST_PARITY_PENDING` | Rust command/store 路径存在；尚无全量 layout/schema golden |
| §5.3 Project/migration/backup | `RUST_PARITY_PENDING` | 全写路径故障注入 rollback golden 已验收（§17.2.4，11 测试）；migration single/batch/bundle/artifact/rotation 全覆盖 |
| §5.4 Editing/repair | `RUST_PARTIAL` | editor/repair 路径存在，产物过独立 validator；repair 空座锁等已知差距；project-edit/repair CLI 已实现 |
| §5.5 CLI | `RUST_PARITY_PENDING` | **26 个子命令** + help/version；init→info/validate→solve→rotate→edit→repair→export→privacy→pack/restore 生命周期集成测试全绿；CLI stdout 契约无 golden |
| §5.6 Export | `RUST_PARITY_PENDING` | SVG/HTML/PNG/PDF/XLSX/DOCX/PPTX 全格式 + 独立 reader 验证（openpyxl/python-docx/python-pptx/pypdf/Pillow，340 项 0 mismatch）；print-html 归一化与 PDF CJK fallback 为登记分歧（M4 决策） |

**M2 Exit Gate（修订版 §5.7）：2026-08-10 证据评估通过**——ledger 主流程
全部 ≥ `RUST_PARITY_PENDING`、React 绕 Python E2E 全绿、CLI 生命周期闭环、
全写操作 rollback 故障注入验收、Python 仅作 oracle。剩余均为已登记的
选项级差距与 M4 决策项（见 ledger §19.10–19.15）。

## M3 验收状态（修订版 §6，2026-08-10 更新）

| 项 | 状态 | 说明 |
|---|---|---|
| §6.1 Hard constraints/search | 实现路径存在 | 静态冲突、candidate domain、matching、MRV/backtracking + exact differential（n≤8 暴力枚举）；取消延迟有测试 |
| §6.2 Soft optimization | `RUST_VERIFIED` | local search + 贪心停滞早退；fixed-assignment scoring 差分 34/34；cost-vs-fallback 基线 34/34 |
| §6.3 Candidate engine | `RUST_VERIFIED` | 20/40/50/60/80 × 1/5/20 差分 15 combos 0 mismatch；推荐 = max plan_score total；stability 激活路径已接（`--latest-snapshot`） |
| §6.4 Rule metadata/DSL | `RUST_PARTIAL` | Rust registry（goal_rules.rs/RuleSet）被 solve/candidates/score 消费；React 第二套规则真相为 M6 删除项，不扩展 |
| §6.5 Audit/explanation | `RUST_VERIFIED` | hard status + soft breakdown + top_contributors + `hard_constraint_summary`/`missing_data`/`history`/`suggested_actions`（本地化 key + 可操作建议），单测覆盖 |
| §6.6 Quality gate | **通过** | 6 样本 regret PASS + 464 项差分 0 mismatch + planted known-feasible corpus **100/100 solved、false-infeasible=0** + 性能回归门槛（基准×1.10 + 绝对上限，CI 常跑）+ 500 次 solve/edit RSS 稳定 |

**M3 Exit Gate（修订版 §6.7）：通过（2026-08-10 证据评估）。** 六项全部
闭合：七状态语义（41 fixtures）、feasibility report 可被 UI 消费
（audit 字段齐全，§19.16）、hard-search/soft 分离、official parity
corpus 全绿（464/0）、rule registry 生效（Rust 侧消费）、OR-Tools 不再
依赖。质量 gate：planted known-feasible corpus 100/100 solved +
false-infeasible=0、随机可行 ≥99.5%、性能回归门槛与长跑内存均常跑。
**剩余登记项（非 gate 阻断）**：React 第二套规则真相为 M6 删除项、
official corpus 的"官方来源"扩充、CLI stdout 字节级 golden。按修订版
可进入 M4 Product Decision/UX（§7.1 Decision Backlog 需产品输入）。

## M4 进度（2026-08-10）

- 原型画廊 `docs/prototypes/`（纯静态 HTML，真实 parity corpus 数据）承载
  §7.1 Decision Backlog 批 1（D1–D8 核心工作流）的变体对比与融合形态确认。
- 批 1 决策已由产品负责人逐项选择/融合，**冻结记录见
  `docs/product-decisions/2026-08-10-batch1-core-workflow.md`**（含 G-1 去术语化
  文案资产、G-2 多视图共享编辑状态与 undo 栈、G-3 复现信息可查、G-4 默认值
  证据要求、G-5 临时工作台语义；PD-D3-ADJ-1 规则 JSON 仅只读）。
- 批 2 决策已由产品负责人逐项选择/融合（**方向冻结**，含 G-6 UI 设计准则：
  专业简洁易用、无 AI 味、克制动效、颜色语义化；PD-D12 PDF 系统字体智能引用
  （无嵌入无 fallback）；PD-D14 文件选择三入口融合，手动路径仅限可信根内——
  安全红线），记录见 `docs/product-decisions/2026-08-10-batch2-export-wrapup.md`。
  **版式格式细节与 UI 交互/视觉细节明确为"待进一步研究讨论"，不冻结**。
- 批 1 仍为"交互契约草案"：待目标形态页在真实浏览器确认与 dogfood 验证后
  才算冻结（§7.9）。批 2 方向已冻结，版式/UI 细节待研究；全部 15 项决策
  完成后进入 M5 实现（决策项逐一过 Rust contract + React E2E）。
- **UI 设计方向草案（2026-08-10）**：`docs/product-decisions/
  2026-08-10-ui-design-direction.md`——Apple HIG 四支柱（清晰/遵从/克制/
  深度）× 跨平台中性实现 + G-6 强化；语义色系统、系统字体栈、动效仅做
  状态反馈（≤200ms + 尊重减少动效）、平台自适应策略、无障碍/键盘要求、
  反模式红线（无 AI 味）。M5 组件实现必须逐条对照（§8 约束条款）。
- **M4 Gate 收口（2026-08-12）**：`docs/product-decisions/
  2026-08-10-m4-gate-closure.md`——§7.9 六项退出条件逐项对照达成；
  批 1 融合形态页浏览器确认；打印版式规范按实际需求修订（一页纸/大字
  姓名/字号按最长姓名自适应/讲台过道窗门标注）；导出默认值矩阵与示例
  名单出草案（dogfood 后冻结 G-4）。可进入 M5 规划（alpha.1 前提见
  收口文档 §4）。
- **技术线收尾（2026-08-10，ledger §19.18）已完成**：native→crates 目录
  收敛（§1.1）、lib.rs 单体拆分 9 模块（§1.2）、candidate-set /
  plan-comparison typed DTO + oracle golden 解析（§4.2）、property-based
  12 门（§11.3 全清单：solver/editing/migration + backup-restore +
  canonical 幂等）、fuzz-style 22 入口 + **cargo-fuzz 6 个 libFuzzer
  targets（§11.4，CI nightly job）**、CLI stdout golden 21 命令含 project
  全生命周期（§5.5）、repair 空座锁 + saved locks、roster 别名镜像。
  §19.18 登记的四项未闭合项全部闭合。剩余边界：CLI 参数组合全量枚举、
  fuzz corpus 长期积累。

## M5 进度（2026-08-12）

- **阶段 A（导出/打印/字体/PNG/默认值/示例/registry）已完成**（ledger
  §19.19）：导出选项统一、print-html 独立版式、PDF 系统字体引用（后经
  §19.26 重做为光栅页）、PNG 2x 文字渲染、默认值矩阵、示例名单资产、
  规则 registry 消费。
- **阶段 B（批 1 融合形态 B1–B8）已完成**（ledger §19.20）：导航/画布/
  规则/快速高级/候选/诊断/历史轮换/导入，浏览器 E2E 全绿。
- **阶段 C（桌面与平台）已完成**（ledger §19.22）：D14 三入口文件选择
  （Tauri dialog + 拖拽 + 可信根路径）、平台自适应（⌘/Ctrl、减动效）、
  触控 Decision Gate（触控非 v2 final 必须项，降级可用）。
- **阶段 D（alpha.1 收口）已完成**（ledger §19.28–19.29）：
  - 全流程 NO_PYTHON_RUNTIME E2E 4/4（import→solve→edit→rotation
    保存→重开→加载→导出默认值），修复 3 处真实 bug（座位点击 pointer
    capture 回归、generate 视图滚动裁剪、rotation load 缺 period_editors）；
  - dogfood 冻结（G-4，`docs/product-decisions/2026-08-12-dogfood-closure.md`）：
    导出默认值（teacher/print-html/A4 横向/public 强制匿名）、打印字号
    算法、示例名单一键使用（D10 补全）；
  - §8.3 退出条件对照（`2026-08-12-alpha-exit-check.md`）：条件 2/3/4
    达成，条件 1（无 RUST_PARTIAL 必须项）未达 → **alpha.2**；
  - parity 升级：pair-report/schema list/export → RUST_VERIFIED（§19.18
    golden 证据，§19.29）。
- **审计轮（2026-08-12）**：后端（`docs/audit-2026-08-12-backend.md`，
  M1-05/事务/路径全过，undo 栈加 100 步上限）、web
  （`docs/audit-2026-08-12-web.md`，生成竞态/401 引导/pointer 泄漏/死代码
  清理）、export（`docs/audit-2026-08-12-export.md`，print-html 网格从未
  生效的阻断修复、坐标溢出护栏、匿名泄漏学号修复）、core/schema（进行中）。
- **M6 前置（2026-08-12，c4ba074）**：桌面壳与服务器已解耦 Python 树——
  frontendDist/resolve_web_root 指向 `clients/web/dist`（发布流程加 npm
  build）；M6 删除面（src/seattrellis、native、e2e、pyproject、desktop.yml）
  已确认 Rust 侧无依赖。
- 发布前平台验收清单（Windows Office 中文渲染/真实打印机/真实班级数据/
  桌面壳）：`docs/product-decisions/2026-08-12-platform-acceptance-checklist.md`。

## M5 alpha.2 收口（2026-08-12，ledger §19.30–§19.36）

- **§8.3 Alpha 退出条件全部达成**（`docs/product-decisions/2026-08-12-alpha2-closure.md`）：
  无 v2 必须项残留 `RUST_PARTIAL`（ledger 200 行 `RUST_VERIFIED=71`）；
  Rust-only E2E 全绿；正式 schema 全 round-trip；Python 只剩 oracle 身份。
- **CLI parity 缺口关闭**（§19.30）：33 命令 golden 33/33 0 mismatch；
  doctor/validate(--preset/--history/--strict)/edit/repair(--ignore-saved-locks)/
  history-report/project-* 镜像 Python 表面；presets.rs 14-preset 镜像。
- **candidates golden 矩阵**（§19.31）：p50×{1,5,20}、p60×{1,5}、p80×{1}
  字节稳定 golden（corpus v1.1.0）；exports 独立 reader 374 行 0 mismatch。
- **CLI 参数全量枚举**（§19.33）：279 用例 sweep（`cli_arg_sweep.rs`）发现并
  修复 3 类真 bug——用法错误退出码 1→2（冻结契约）、solve 输入错误分类
  70→2、repair 拒绝 solve 输出（双形状解析）。
- **pair-report lookback 语义**（§19.33）：`recent_occurrences` 按 pair 自身
  records 计（Python `records[-lookback:]`），边界回归 + live 差分锁定。
- **rotation-plan schema_version**（§19.34）："0.2.2"→"1.0"（oracle 冻结值），
  gate 断言 + harness 强制比较。
- **artifacts compare/restore**（§19.35）：21 项 server 契约测试全绿 +
  **M1-05 workspace containment 修复**（artifact 必须位于项目 history/
  outputs 内，canonicalize 后判含，镜像 Python）；登记分歧：`restored_at`
  形状、candidate_set kind 保留、错误 envelope 与 React reader 不匹配
  （M5 待产品）。
- **roster-mapping 启发式差分**（§19.36）：10 case corpus（`fixtures/
  roster-mapping/`）+ expected.json（oracle 记录），Rust 侧全等 + Python
  守卫测试。
- **M6 无 Python 门禁**（4911352）：`scripts/check_no_python_runtime.py`
  + CI job 扫描 release 二进制 0 Python 符号；workbench 移除死 backend
  控件（OR-Tools 文案清除）。
- **CI 三平台矩阵修复**：Tauri 系统依赖（Linux）、journal 路径分隔符
  （Windows）、sweep 跨平台用例 + unused_mut（Windows clippy）。
- **进入 M6 门槛**：产品负责人确认 v1.x 最终 tag 与 v1.x-maintenance
  分支建立时机（计划 §9.1），然后执行 Python retirement（删除面已确认
  无 Rust 依赖，c4ba074）。

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
