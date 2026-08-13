# SeatTrellis v2 — M5 收口决策批（PD-D17 系列：退休与语义决策，2026-08-13）

> 状态：冻结（主 agent 复核并发产出后批准，2026-08-13）
> 依据：总计划 §0.5/§8.3、§18 移除登记格式；本批决策对应的 ledger 行
> 从 `RUST_PARTIAL` 转为 `INTENTIONALLY_REMOVED_V2` 或 `RUST_VERIFIED`，
> 理由/迁移/影响以下表为准。

## D17-1 teacher_goals 15-preset 面退休（§2.3 行）

- **决定**：v2 不移植 Python `teacher_goals` 的 15 个 preset 面；v2 产品面
  是 Rust `/catalogs` 的 4 个 goal（GOAL_IDS，goal_rules.rs:14-19），与
  PD-D 系列冻结的 goal 集一致（G-1 去术语化 + 批 1/批 2 决策）。
- **理由**：v1 的 `list_teacher_goals`/`get_teacher_goal`/`resolve_teacher_goal`
  （application/teacher_goals.py:98/:104/:117 + api/handlers.py:211）服务于
  v1 工作台的目标选择；v2 工作台直接消费 Rust catalogs（§19.15 capability
  对账确认 Rust 侧自洽）。15 preset 的规则组合由 v2 规则编辑器（D3 句式
  模板 + 规则 JSON）承担。
- **迁移**：v1 项目中的 `goal_id` 引用映射到 4 个 v2 goal 之一
  （daily-rotation/vision-front/score-balance/exam-style 等，以 catalogs
  为准）；未知 goal_id 由 Rust 显式报错。
- **用户影响**：v1 中"选 preset 即得整套规则"的入口消失；v2 中用户选 goal
  后再按需调整规则。低频深度定制用户改用规则 JSON。

## D17-2 roster 映射模板生成/应用退休（§5 行）

- **决定**：v2 不移植 `create_roster_mapping_template`/`apply_roster_mapping_template`
  （application/roster_mapping.py:394/:407）。
- **理由**：模板功能是 v1 导入助手的复用便利（保存/重放列映射）；v2 导入
  流程为 draft 预览 + 逐列映射确认（server roster.rs），无模板入口；
  该功能未暴露为 v1 HTTP API（handlers 无对应路由），仅 service 内部。
- **迁移**：无自动迁移；用户重映射时按表头别名自动建议（Rust
  suggest_mapping 与 oracle 差分全等，§19.36）。
- **用户影响**：跨文件复用列映射需手动重选（建议通常一次命中）。

## D17-3 roster_fingerprint 退休（§5 行）

- **决定**：v2 不移植 `roster_fingerprint`（application/roster_update.py:119）
  的"无变更拒绝"提交语义。
- **理由**：fingerprint 是 v1 preview/apply 冲突检测的内部机制
  （roster_update.py:358/:392）；v2 的 roster draft 以 revision + 冲突
  检测（duplicate/ambiguous/name-mismatch issues）承担同等防护，且 UI 有
  "无变更"按钮禁用（client 侧检查）。该机制未暴露为独立 API。
- **迁移**：无。
- **用户影响**：无——v2 提交路径已拒绝无变更提交（UI 禁用 + 服务端
  issue 校验）。

## D17-4 report 模板"显分数"渲染退休（§12.2 行）

- **决定**：v2 座位表渲染不显示成绩/分数；`PrivacyOptions.for_template()`
  的 report 模板（显分数隐其余）不移植。
- **理由**：§19.27 决策已统一缺省模板为 teacher（真实姓名默认保留、public
  强制匿名）；成绩渲染与 G-3（隐私红线：禁止提交真实成绩）与教师打印
  场景冲突；v1 report 模板从未在 v2 打印版式规范中出现
  （print-layout-spec.md 冻结的版式不含分数）。
- **迁移**：无；v2 导出默认值矩阵已冻结（teacher/print-html/A4 横向/
  public 强制匿名，G-4 dogfood 冻结）。
- **用户影响**：不能再打印带分数的座位表；分数管理不在 v2 scope。

## D17-5 public「🔒 班级公示版」徽标退休（§12.2 行）

- **决定**：v2 print-html 版式不渲染该徽标。
- **理由**：print-layout-spec.md（2026-08-10 修订）冻结的版式以页脚溯源
  （seed/生成信息标注，§19.19 A2）替代；徽标是 v1 视觉元素，v2 版式
  无公示场景专用标记。
- **迁移**：无。
- **用户影响**：公示场景通过 public 模板匿名 + 页脚溯源实现同等可信度。

## D17-6 export 文案 locale 退休（§12.3 行）

- **决定**：v2 导出工件文案不做 zh/en 本地化；`locale` 仅保留现有语义
  （匿名占位符 export.rs:441 + print-html `lang` 属性 print_html.rs:119）。
- **理由**：v2 导出为图形/表格（SVG/PNG/PDF 光栅/XLSX/DOCX/PPTX），
  文案极少且为结构化标注；界面语言由前端 i18n 承担（zh-CN/en 双语言
  UI，React messages）。v1 的打印报告文案本地化随 D17-4 的 report 模板
  一并退出。
- **迁移**：无。
- **用户影响**：打印版式标注为固定中文/双语结构（版式规范冻结），UI 语言
  不受影响。

## D18 cooling 语义决策（§8 行 → RUST_VERIFIED）

- **决定**：v2 保持与 Python oracle 完全一致的 lookback 近似语义
  （`cooling_period` → avoid_recent_neighbors `lookback`，两侧相同），
  "冷却期内完全禁配"的强语义不实现。
- **理由**：parity 定义 = Rust 与 Python 行为一致（§19.14 soft objectives
  差分 34/34 已证）；强语义是两语言共同的 v1 近似，非 parity 缺口。
  实现强语义需要新的规则表达能力，属 v2.x 产品决策，不阻塞 v2.0.0。
- **迁移**：无（两侧行为不变）。
- **用户影响**：无——与 v1 行为一致。

## 生效范围

- D17-1 至 D17-6 对应 ledger §2.3/§5/§12 共 6 行 → `INTENTIONALLY_REMOVED_V2`
  （§18 登记扩展，引用本文件）。
- D18 对应 §8 `cooling` 行 → `RUST_VERIFIED`（差分证据 + 本决策）。
