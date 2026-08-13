# SeatTrellis v2 — M5 alpha.2 收口：§8.3 Alpha 退出条件逐项对照（2026-08-12）

> 日期：2026-08-12
> 状态：**通过（2026-08-13 复核闭环）**——12/12 typed DTO + 共享 oracle golden + 9 项 CLI/schema 缺口关闭，M5 四门全绿，可进入 v1.9.0 冻结与 M6
> 依据：总计划 §8.2/§8.3、M5 计划阶段 D（D3/D4）、ledger §19.30–§19.36。
> 前置文档：2026-08-12-alpha-exit-check.md（alpha.1 收口对照）。

## 1. §8.3 逐项对照（更新）

| §8.3 条件 | 状态 | 证据 |
|---|---|---|
| 1. 无 `PYTHON_ONLY` / `RUST_PARTIAL` 的 v2 必须项 | **达成** | ledger 200 行：`PYTHON_ONLY=0`、`RUST_PARTIAL=0`、`RUST_VERIFIED=81`。两项 Python-only 能力经 PD-D16 书面退休（§18 登记）；compare/restore 经 §19.37 共享 oracle golden 关闭；CLI 9 行与 project.schema 经 §19.38 关闭 |
| 2. Rust-only E2E 全绿 | **达成** | e2e-rust 4/4（本地重跑 + CI web-e2e-rust job）；backend 控件移除后重验 |
| 3. 所有正式 schema 有 Rust round-trip | **达成** | registry 12 种 ArtifactKind 全部有 typed DTO（含 history_archive/editing_operation_log/export_preset/rotation_plan/editor_protocol）+ 12 个生成 .v2. schema + round-trip/unknown-field 测试（schema crate 71 绿）；contract check 无 drift；CLI schema-export 12 种全通 |
| 4. Python 只剩 oracle/test reference 身份 | **达成** | server 已移除全部 Python web-static 回退（resolve_web_root 仅 env/launch-dir/embedded/build-time）；no-python 门禁 --tree 显式报告 oracle 树存在（非假 clean），--expect-retired 为 M6 硬门禁 |

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

## 3. 阻断与后续登记项

| 项 | 类型 | 证据路径 |
|---|---|---|
| artifacts compare/restore 契约（5 行） | **重新打开** | §19.35 的 Rust 自契约测试不能证明已登记的 `restored_at`、candidate_set restore kind 与错误 envelope 差异已达 oracle parity |
| ~~suggest_roster_mapping 启发式（2 行）~~ | **已关闭** | §19.36：10 case roster-mapping 差分 corpus（expected.json 由 oracle 记录，Rust 侧全等） |
| 平台验收清单 | 外部依赖 | 2026-08-12-platform-acceptance-checklist.md——真实 Windows/macOS 硬件 + 打印机 |
| CLI 侧 print-html public 匿名路径 | 选项级 | §19.31 登记：`export` 格式面不含 print-html；public 匿名由共享网格层 5 格式覆盖 |
| 事务层并发互斥 | 观察项 | §19.33-6：并发 CLI 共享 journal 目录竞态（sweep 已规避）——M7 候选 |

## 4. 结论

- M5-D 暂未验收：§8.3 条件 2 已达成；条件 1/3 未达，条件 4 需收紧证据。
- 技术线已完成的大部分证据仍有效：279 用例 sweep 全绿，41 fixtures +
  34 rotation + 374 exports +
  33 goldens 全部 0 mismatch，CI 三平台矩阵修复完毕。
- **进入 M6 的门槛**：补齐全部正式 schema 的 Rust round-trip；为两项
  Python-only 能力完成实现或 §18 移除决策；修正 compare/restore parity
  与 React error envelope；重新复算 ledger 后再由产品负责人确认最终
  v1.x tag 与 `v1.x-maintenance` 分支。
