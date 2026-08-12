# SeatTrellis v2 — M5 alpha.2 收口：§8.3 Alpha 退出条件逐项对照（2026-08-12）

> 日期：2026-08-12
> 状态：**alpha.2 parity-gap 关闭完成；§8.3 条件 1 除 3 项登记边界外达成**
> 依据：总计划 §8.2/§8.3、M5 计划阶段 D（D3/D4）、ledger §19.30–§19.34。
> 前置文档：2026-08-12-alpha-exit-check.md（alpha.1 收口对照）。

## 1. §8.3 逐项对照（更新）

| §8.3 条件 | 状态 | 证据 |
|---|---|---|
| 1. 无 `PYTHON_ONLY` / `RUST_PARTIAL` 的 v2 必须项 | **达成（3 项登记边界）** | ledger 200 行：`RUST_VERIFIED=63`；§19.32 判定的 8 个必须项中，rotation-plan schema_version（§19.34）、CLI 退出码/分类/repair 双形状（§19.33）、pair-report lookback（§19.33）已关闭。剩余 2 项有明确证据路径：artifacts compare/restore 契约测试（§19.35 收口中）、平台验收（真实硬件）。suggest_roster_mapping 启发式差分已关闭（§19.36：10 case 差分 corpus 全等） |
| 2. Rust-only E2E 全绿 | **达成** | e2e-rust 4/4（本地重跑 + CI web-e2e-rust job）；backend 控件移除后重验 |
| 3. 所有正式 schema 有 Rust round-trip | **达成** | 8 种 typed DTO 全 round-trip（contract check 无 drift）；rotation-plan 按 v1 契约镜像并锁定（§19.34）；无 DTO 的 3 种（history_archive/editing_operation_log/export_preset）schema-export 显式报"无 typed DTO"（deea352） |
| 4. Python 只剩 oracle/test reference 身份 | **达成** | React/桌面/CLI 全 Rust；M6 dry-run 删除 src/seattrellis 后构建成功（e28bf18）；no-python-runtime CI 门禁（4911352）扫描 release 二进制 0 Python 符号 |

## 2. alpha.2 关闭清单（§8.2 parity gap）

| 项 | 关闭证据 |
|---|---|
| CLI doctor/validate/edit/repair/history-report 缺口 | §19.30：33 命令 golden 33/33 0 mismatch；presets.rs 14-preset 镜像；repair saved-lock |
| project-* 参数/输出 golden | §19.30：project-init/list/privacy/pack/restore/info/edit/repair golden + bundle 双向互操作 |
| 候选 n=1/5/20 golden 扩展 | §19.31：p50×{1,5,20}、p60×{1,5}、p80×{1} 字节稳定 golden，verify 全量复现 0 diff |
| 导出独立 reader golden | §19.31：374 行 0 mismatch（print-html 结构 reader + pypdf 光栅页） |
| 用法错误退出码 1→2（冻结契约） | §19.33：279 用例 sweep；audit.json 重录；33/33 复验 |
| solve 输入错误分类 70→2 | §19.33：classify_solve_error 18 tokens；垃圾输入实测 exit 2 |
| repair 拒绝 solve 输出 | §19.33：双形状解析 + 回归用例 |
| pair-report recent_occurrences 窗口语义 | §19.33：per-pair lookback + 边界测试 + live 差分（pair totals 1/5/6 一致） |
| rotation-plan schema_version "0.2.2"→"1.0" | §19.34：gate 断言 + harness 强制比较 + rotation 34/34 |
| schema-export v1/v2 契约 | deea352：candidate_set/plan_comparison 指 v2；无 DTO 种类显式报错 |
| EditorDraftStore/SolveRequestStore 无界 | deea352：FIFO cap 64，双 store 单测 |
| M6 无 Python 门禁 | 4911352：check_no_python_runtime.py + CI job；backend 死控件移除 |
| CI 跨平台 | 25ed6da：Windows journal 分隔符；Linux Tauri 依赖；sweep 跨平台用例 |

## 3. 剩余登记项（alpha.2 收口不阻断，进入 M6 前需产品/证据输入）

| 项 | 类型 | 证据路径 |
|---|---|---|
| artifacts compare/restore 契约（5 行 RUST_PARTIAL） | 必须项 | §19.35 契约测试强化（artifact 种类/diff 字段/隐私/error contract）——React 在用端点，Python golden 不可达（差分 harness 只比 CLI），以 server 契约测试为自动证据 |
| ~~suggest_roster_mapping 启发式（2 行）~~ | **已关闭** | §19.36：10 case roster-mapping 差分 corpus（expected.json 由 oracle 记录，Rust 侧全等） |
| 平台验收清单 | 外部依赖 | 2026-08-12-platform-acceptance-checklist.md——真实 Windows/macOS 硬件 + 打印机 |
| CLI 侧 print-html public 匿名路径 | 选项级 | §19.31 登记：`export` 格式面不含 print-html；public 匿名由共享网格层 5 格式覆盖 |
| 事务层并发互斥 | 观察项 | §19.33-6：并发 CLI 共享 journal 目录竞态（sweep 已规避）——M7 候选 |

## 4. 结论

- M5-D 验收完成：§8.3 四项条件 3.5/4 达成；条件 1 的剩余 3 项为
  已登记证据路径（§19.35 进行中 / roster 差分 / 平台硬件）。
- 技术线 alpha.2 全部关闭：ledger `RUST_VERIFIED=66`（本轮 +25），
  279 用例 sweep 全绿，41 fixtures + 34 rotation + 374 exports +
  33 goldens 全部 0 mismatch，CI 三平台矩阵修复完毕。
- **进入 M6 的门槛**：compare/restore 契约测试与 roster 启发式差分
  收口（§19.35），然后由产品负责人确认 v1.x 最终 tag 与
  v1.x-maintenance 分支建立时机（计划 §9.1），启动 Python retirement。
