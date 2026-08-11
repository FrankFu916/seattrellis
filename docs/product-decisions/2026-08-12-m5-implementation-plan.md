# SeatTrellis v2 — M5 实现计划（alpha.1 → alpha.2）

> 日期：2026-08-12
> 状态：**计划草案**——M4 收口后进入 M5（计划 §8）。执行顺序按依赖链
> 组织；每项含 Rust / React / E2E 子任务与验收标准。里程碑不绑定日期
> （§0.4：质量里程碑，Exit Gate 为准）。
> 依据：计划 §8.1（alpha.1 切默认路径）、§8.2（alpha.2 关 parity gap）、
> §8.3（alpha 退出条件）、M4 决策 D1–D15、UI 设计方向草案、打印版式
> 规范（修订版）、导出默认值矩阵（候选）、ledger §19.18。

## 1. 目标

**alpha.1**：默认开发/Web/Desktop/CLI 全部运行 Rust；Python 不再被 React
调用；干净机器（无 Python/pip/OR-Tools）完成 import → solve → adjust →
save → export 全流程；v1 项目 migration 全走 Rust（§8.1）。
**alpha.2**：关闭 parity gap（§8.2 清单），ledger 全部 v2 必须项
`RUST_VERIFIED`。

## 2. 阶段与依赖链

```text
阶段 A：Rust 能力补齐（M4 决策的 Rust 实现）
   A1 导出选项统一化（§12.3）
   A2 print-html 独立版式（打印规范）
   A3 PDF 系统字体引用（D12）
   A4 PNG 文字渲染（D13）
   A5 导出默认值矩阵 + 上次设置记忆（D9）
   A6 示例名单资产（D10）
   A7 规则 registry 消费切换（M6 前置）
        ↓
阶段 B：批 1 融合形态实现（每项 = Rust contract 确认 → React → E2E）
   B1 导航（D1）  B2 画布（D2）  B3 规则（D3）  B4 快速/高级（D4）
   B5 候选（D5）  B6 诊断（D6）  B7 历史/轮换（D7）  B8 导入（D8）
        ↓
阶段 C：桌面与平台（可与 B 并行）
   C1 Tauri 三入口文件选择（D14）  C2 平台自适应  C3 触控 Decision Gate
        ↓
阶段 D：验收与收口
   D1 alpha.1 默认路径（NO_PYTHON_RUNTIME E2E 扩展）
   D2 导出/打印 dogfood（G-4）  D3 ledger 逐项升级  D4 §8.3 退出条件
```

## 3. 阶段 A：Rust 能力补齐

### A1 导出选项统一化（计划 §12.3，RUST_PARTIAL 补齐）

- **现状**：orientation/page_scale 仅 PDF 生效；margin_mm/paper_size 无对应；
  locale 仅影响匿名占位符。
- **任务**：
  1. print-html：orientation（横/竖）、paper_size（A4/A3/Letter）、
     page_scale、margin_mm、locale 全部生效；
  2. PDF：补齐 paper_size / margin_mm（现缺失）；
  3. DOCX：orientation 生效（Python 契约：page 对 DOCX 生效）；
  4. 全部格式的选项拒绝规则与 Python `export_extension` 白名单对齐
     （基础 HTML/Excel/PNG 不接受 template/privacy/page/locale 的规则
     保持或按 D9 决策修订——D9 默认值矩阵决定）。
- **验收**：与 Python 导出选项拒绝/接受矩阵差分（新增 harness class）；
  `RUST_PARTIAL` 页面选项条目逐项升级。

### A2 print-html 独立版式（打印版式规范，2026-08-12 修订版）

- 恢复独立 print 渲染路径（当前归一化为 html，D11 决策）；
- 实现规范全部条款：**默认横版 A4**、一页内最大化（座位=可用面积÷网格）、
  字号算法（最长姓名 → 统一字号，24pt 上限+截断提示）、只放姓名（学号
  可配置）、结构标注（讲台/过道/窗/门，文字）、页眉一行+页脚溯源、
  可配置项 §4.5 全部（方向/纸张/缩放/字号/学号/标注/页眉/网格/颜色/
  模板/隐私/locale）；
- 独立 validator 结构验证（页面结构断言：行列数、姓名存在性、隐私过滤）。
- **验收**：规范 §8 验收清单全项；与网页 HTML 差异矩阵对照；
  `RUST_PARTIAL` print-html 条目升级。

### A3 PDF 系统字体智能引用（D12）

- 系统字体枚举（macOS 字体目录 / Windows Fonts / Linux fontconfig 探测）；
- 质量优先级链：PingFang SC → Noto Sans CJK SC → Microsoft YaHei →
  SimSun → 任意 CJK 字体；
- PDF 按字体名引用（不嵌入）；选中质量偏低时导出警告（文本提示）；
- 与 A4（PNG 光栅化）共用字体发现模块。
- **验收**：主要查看器（Acrobat/预览/Edge/手机）字体替换实测矩阵；
  PDF 中文姓名可读；无嵌入（体积不变）。

### A4 PNG 文字渲染（D13）

- 文本光栅化（fontdue/ttf-parser 类，纯 Rust 无依赖），字体来自 A3
  字体发现；
- 字号自适应（长姓名）、2x 分辨率；public+anonymize 打码「学生A/B/C」。
- **验收**：60 座级渲染性能；打码零真实姓名泄漏（独立 validator）。

### A5 导出默认值矩阵 +「上次设置」记忆（D9）

- 按导出默认值矩阵（候选）实现：8 格式默认模板/隐私/页面/文件名
  （PDF/打印 HTML 默认**横版**+自动缩放）；
- 「上次设置」全局记忆 + 班级覆盖，Rust 端存储（项目元数据/用户配置），
  非浏览器 localStorage；「恢复默认」入口。
- **验收**：默认值矩阵 dogfood 计划执行（G-4）后冻结取值。

### A6 示例名单资产（D10）

- 20 人静态资产（常见姓名、属性覆盖）；构建期校验（字段完整/无重复）；
- 「示例」隔离命名空间 + 一键删除（§7.4）；不提供 CLI 命令（D15）。
- **验收**：新手 5 分钟完整流程（dogfood）；误删真实数据概率 0。

### A7 规则 registry 消费切换（M6 前置）

- React 规则控件改为消费 `ruleRegistry.generated.ts`（已由 xtask 生成，
  M3-01）；删除 React 自编译规则逻辑的**使用**（删除动作本体在 M6
  beta.1——M5 只做"UI 由 registry 驱动"，不扩展第二套真相）。
- **验收**：规则页（B3）全部控件由 registry 渲染；无新增 TS 规则逻辑。

## 4. 阶段 B：批 1 融合形态实现

统一方法：每项 = ① Rust contract 确认/补齐 → ② React 组件实现（按 UI
设计方向草案 + 决策记录）→ ③ React E2E（vitest + 浏览器 E2E 扩展）→
④ 融合形态页对照（docs/prototypes 目标形态是验收参照）。

| 项 | Rust 侧 | React 侧 | E2E 验收 |
|---|---|---|---|
| B1 导航 D1 | 现有 API 路由盘点；临时工作台上下文（无项目写路径） | 侧栏 + 上下文操作条 + 首次任务清单（用过即收）+ 临时工作台入口 | 班级/临时工作台切换；任务清单消失时机 |
| B2 画布 D2 | editing 协议（已有：swap/move/lock/undo/redo） | drag-lift 画布 + 框选批量 + 表格视图切换（同一 draft，G-2） | 拖拽交换、框选锁定、表格↔画布同步、undo/redo 共享 |
| B3 规则 D3 | Rust metadata 暴露"句式模板 → 参数槽"映射（新 API） | 句式构建器 + 卡片管理 + 高级表单 + JSON 只读视图 | 句式编译（走 Rust）、卡片启停、高级表单字段来自 registry |
| B4 快速/高级 D4 | 默认参数（candidates=5、历史 3 期待 G-4 冻结） | 3 问面板 + 历史行可见 + 折叠区（≤120ms 淡入） | 默认值、折叠展开、复现信息详情（G-3） |
| B5 候选 D5 | 推荐理由卡数据（audit 报告 → 通俗文案 i18n 资产，G-1） | 理由卡 + 差异高亮 + 明细切换 + 详情折叠 | 术语映射（无 fair_rotation 等）、推荐一致性、复现详情 |
| B6 诊断 D6 | audit 消费组件（与 D5 共用） | 徽章 + 列表双向联动 + 一键修复（Rust editing + validator 复核） | 联动定位、修复后 validator 结果可见 |
| B7 历史/轮换 D7 | rotation 产物 + snapshot 恢复入口 | 双视图切换 + 恢复确认（未保存修改提示） | 视图切换、恢复语义 |
| B8 导入 D8 | 导入事务（已有：parse/apply 分离、原子、回滚） | 步骤条 + 同屏映射/预览 + 冲突处理 + 原子确认栏 | 冲突处理、回滚可见、与 D1 上下文一致 |

## 5. 阶段 C：桌面与平台（可与 B 并行）

- **C1 Tauri 三入口文件选择（D14）**：tauri-plugin-dialog + 拖拽 +
  可信根内路径输入；能力探测（Web 回退 input）；安全边界（绝对路径
  禁止，io 层校验复用）。
- **C2 平台自适应**：Cmd/Ctrl 快捷键映射、`prefers-reduced-motion`/
  macOS 减动效、macOS 图标可换 SF Symbols、窗口行为（Tauri 原生）。
- **C3 触控 Decision Gate（§7.2）**：画布触控原型测试（拖拽/缩放手势），
  决定触控是否 v2 final 必须；记录决策。

## 6. 阶段 D：验收与收口

- **D1 alpha.1 默认路径**：现有 `NO_PYTHON_RUNTIME` E2E（e2e-rust/）
  扩展为完整主流程断言（import → solve → adjust → save → export 全走
  Rust，无 Python 探测）；CI 干净机器 job 保持。
- **D2 dogfood（G-4）**：导出默认值矩阵、打印字号算法（横版默认、最长
  姓名、一页最大化）、示例名单流程、B1–B8 关键交互；结果记录后冻结
  默认值。
- **D3 ledger 升级**：随每项实现登记证据，逐项 `RUST_PARTIAL/PENDING →
  RUST_VERIFIED`（§8.2：alpha.2 末全部 v2 必须项 VERIFIED）。
- **D4 §8.3 退出条件**：无 PYTHON_ONLY/RUST_PARTIAL 的 v2 必须项；
  Rust-only E2E 全绿；正式 schema 全 Rust round-trip；Python 只剩
  oracle/test 身份。

## 7. 里程碑（质量驱动，非日期）

| 里程碑 | 内容 | Exit |
|---|---|---|
| M5-A | 阶段 A 全部（导出/打印/字体/PNG/默认值/示例/registry） | A1–A7 各自验收 + CI 绿 |
| M5-B | 阶段 B 八项融合形态 | 每项 E2E 绿 + 与原型目标形态对照 |
| M5-C | 桌面/平台/触控决策 | C1/C2 验收 + C3 决策记录 |
| M5-D | alpha.1 全流程 + dogfood + ledger 升级 | §8.3 退出条件逐项 |

## 8. 风险与依赖

- **A2/A3/A4 依赖字体与渲染库选型**（fontdue/ttf-parser）——最小依赖
  政策（§5.6）约束；选型需 clippy/审计通过。
- **B5 术语 i18n 资产**是 G-1 关键路径——Rust audit message_key 与
  文案表对齐先行（B5 前置）。
- **B2 拖拽/框选动效**按设计方向（≤200ms、reduce-motion）——性能
  （60 座）需基准。
- **D2 dogfood 依赖真实教师使用**——样本不足时以原型任务测试替代并
  记录（§0.5 流程允许）。
- alpha.2 的 parity gap 清单以 alpha.1 dogfood 收集为准（§8.2）。

## 9. 明确不做（M5 范围外）

- Python 删除（M6 beta.1）；React 规则逻辑删除动作（M6）；
- 插件/脚本扩展、云同步、账号、协作（§7.8 不在 v2 scope）；
- 安装包/签名/notarization（M7 RC）；
- 性能压测与长跑基线扩展（已有 gate 保持常跑）。
