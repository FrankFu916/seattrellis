# SeatTrellis v2 — 阶段 D4：§8.3 Alpha 退出条件对照（2026-08-12）

> 日期：2026-08-12
> 状态：**alpha.1 收口达成；alpha 完整退出条件未达（条件 1 需 alpha.2）**
> 依据：总计划 §8.3（Alpha 退出条件）、M5 计划 §6 D4。

## 逐项对照

| §8.3 条件 | 状态 | 证据 |
|---|---|---|
| 1. 无 `PYTHON_ONLY` / `RUST_PARTIAL` 的 v2 必须项 | **未达成** | ledger 仍有 RUST_PARTIAL：CLI `doctor`/`validate`（preset/history 警告语义）、`edit`（CLI 形态）、`repair`（saved-lock 语义）、`history-report`（warning 细节）、`project-*`（参数/输出 golden 未全量对齐）。属 §8.2 alpha.2 关闭范围 |
| 2. Rust-only E2E 全绿 | **达成** | e2e-rust 4/4（真实 Chromium vs Rust 二进制，CI web-e2e-rust job；NO_PYTHON_RUNTIME fixture 断言 native 可执行） |
| 3. 所有正式 schema 有 Rust round-trip | **达成** | schema-migrate CLI + canonical 幂等 property 12 门（§19.18）；schema-list/export golden 21/21 |
| 4. Python 只剩 oracle/test reference 身份 | **达成** | Web/桌面/CLI 全部 Rust；Python 仅 oracle 差分（rust_python_diff.py）与 pytest/Playwright runner |

## alpha.1 收口声明

- **默认路径**：开发/Web/Desktop/CLI 全部运行 Rust；React 不再调用
  Python；v1 项目 migration 全走 Rust（§8.1 目标达成）。
- **验证**：E2E 4/4、React 154、Rust 各 crate 全绿、clippy/fmt/diff
  全绿（CI 修复 4918a19）。
- **dogfood**：导出默认值冻结（G-4，2026-08-12-dogfood-closure.md）；
  B1–B8 关键交互实测通过。

## alpha.2 剩余工作（§8.2 parity gap 清单，进入下一里程碑）

1. CLI `doctor`/`validate`/`edit`/`repair`/`history-report` 的
   RUST_PARTIAL 缺口（preset/history 警告语义、saved-lock、CLI edit
   形态、warning 细节 golden）；
2. `project-*` 命令参数/输出 golden 全量对齐；
3. 候选 n=1/5/20 golden 扩展（§19.20/§19.28 已登记）；
4. 导出独立 reader 的 golden 扩展（print-html/PDF，§19.20）；
5. 发布前平台验收：Windows Word/Excel 中文渲染、真实打印机输出。
