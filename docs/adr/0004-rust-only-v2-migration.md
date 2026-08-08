# ADR-0004：v2.0.0 采用 Rust-only 生产运行时

- 状态：接受（supersede ADR-0001 与 ADR-0002）
- 日期：2026-08-08

## 背景

ADR-0001（Rust 原生内核 + PyO3 接入，不进行全量重写）与 ADR-0002（v1.x 保留
Python OR-Tools 后端）定义了 v1 时期的架构：Python 是主运行时，Rust 作为
可选 backend。审计基线 `main@282fd99` 的盘点（《SeatTrellis_v2.0.0_开发与
发布总计划_修订版》，2026-08-08）表明：当前 Rust 实现只能视为迁移原型，领域
模型、状态语义、数据兼容和安全边界尚未冻结；但 v2 的长期目标（紧凑离线桌面
分发、单一语义真相、可证明的求解状态）在 Python 运行时上无法达成。

## 决策

- v2.0.0 的生产运行时完全由 Rust（core/solver/validator/rules/schema/
  planning/storage/export/server/CLI）+ React/TypeScript（仅展示）+ Tauri
  （仅桌面壳）构成；Node 仅用于构建。
- v2.0.0 final 的生产包不得包含 Python、Pydantic、FastAPI、Streamlit、
  OR-Tools、PyO3、pywebview 或 Node runtime。
- Python v1.x 保留为**行为 oracle**：只存在于仓库测试/oracle 路径，是
  parity ledger 与 golden corpus 的基准；在 parity ledger 全部 v2 必需项
  `RUST_VERIFIED`、migration/备份/回滚矩阵全绿、三平台 clean-machine E2E
  全绿之后，从 v2 main 删除（时点与前置条件见计划书 §九"Python retirement
  的准确时点"）。
- 稳定线在 v2 达到门槛前继续使用 `v1.9.x`；v2 依次经历
  `2.0.0-alpha.N → beta.N → rc.N → final`，不给 final 设日期。
- OR-Tools 不进入 v2：v2 求解质量门槛（见计划书 §六 M4/M7）由 Rust 原生
  solver 达成，Python/OR-Tools 仅作为差分对照的 oracle 参考。

## 影响

- supersede ADR-0001 中"迁移采用可选 backend 和差分测试，不进行全量重写"
  与"Python 实现至少保留到 Rust backend 通过三平台构建、行为一致性和性能
  验收"的表述：全量迁移到 Rust-only 是 v2 的正式方向；Python 的保留期以
  oracle 角色为限，并设有明确删除 gate。
- supersede ADR-0002 中"保留 Python OR-Tools 后端"作为产品运行时的决策：
  OR-Tools 降级为 oracle 对照参考，不进入 v2 产品。
- ADR-0003（Rust-first compact desktop runtime）与 v2 方向一致，继续有效。
- 已正式发布的能力默认全部迁移；任何删除必须经过能力审计、ADR、迁移说明
  和明确批准（计划书 §一发布政策）。

## 参照

- 《SeatTrellis_v2.0.0_开发与发布总计划_修订版》（docs/，§一、§四、§九）
- parity ledger（docs/v2-parity-ledger.md，§0 状态字典与维护约定）
- ADR-0001、ADR-0002（被 supersede）、ADR-0003（继续有效）
