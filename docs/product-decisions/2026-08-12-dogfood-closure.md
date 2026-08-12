# SeatTrellis v2 — 阶段 D2 dogfood 收口（G-4 默认值冻结）

> 日期：2026-08-12
> 状态：**已记录并冻结本批默认值**；残余目检项登记（不阻塞）
> 依据：M5 计划 §6 D2（dogfood 后冻结默认值）、D9 导出默认值矩阵
> （2026-08-10-export-defaults.md）、D10 示例名单、打印版式规范。

## 1. 导出默认值冻结（D9 矩阵确认）

| 默认项 | 冻结值 | dogfood 证据 |
|---|---|---|
| 默认模板 | **teacher**（保真名） | E2E `test_export_defaults_carry_real_names`：print-html 产物含 `Student001`、无匿名占位（§19.27 回归锁） |
| 用户目录格式 | **7 项**（svg/png/pdf/print-html/xlsx/docx/pptx） | catalogs 契约测试 + E2E 断言（html 隐藏，后端兼容） |
| 默认格式 | **print-html**（打印/查看一体） | E2E 下载断言 `seat-plan.print.html` |
| 默认页面 | **A4 横向** + 自动缩放 | 打印版式规范 §4 + PDF/print-html 产物实测（§19.26/§19.27） |
| PDF 呈现 | 144 DPI 光栅页（Image XObject） | pdftoppm 独立渲染 + E2E 大小断言；文字不可搜索为已登记边界（§19.26） |
| PNG | 2x，含标题/讲台/姓名/座位号 | 独立解码 976×712 目检（§19.26） |
| public 模板 | 强制匿名（`anonymize: true`）+ 预览一致 | 前后端同条件测试（§19.27） |

## 2. 打印字号算法验证（打印版式规范 §4）

最长姓名 → 统一字号（24pt 上限、8pt 下限、截断提示）在
`print_html::compute_layout` 实测：3–4 字姓名得 24pt 上限、超长姓名
正确钳制到 8pt 并在页脚列出截断名单；`name_width_em` 中西文分别
计宽。E2E 全流程走通（生成 → 导出 print-html）。

## 3. 示例名单流程（D10）

- 前端默认加载示例名单（demoStudents），首次任务卡第 1 步按有效
  名单自动完成（§19.25）；
- **本轮补齐"一键使用示例名单"**：名单为空时编辑器显示空状态 +
  按钮（`studentEditor.emptyHint` / `action.useDemo`，StudentRosterEditor
  `onUseDemo`，154 vitest 含新测试）；
- 示例数据仅存于前端内存/会话（无持久化），清空名单即"删除"，
  真实数据误删概率 0（§7.4 隔离语义）；
- Rust 侧 `sample_roster.rs`（20 人资产 + 构建期校验）为 oracle 对照，
  前端 demo 名单为展示层实现。

## 4. B1–B8 关键交互目检结论（真实 Rust 服务 + Chromium）

| 批 1 项 | 结论 | 证据 |
|---|---|---|
| B1 导航/任务卡 | 通过 | E2E 全流程（上下文操作条驱动向导） |
| B2 画布 | 通过（含本轮座位点击回归修复） | E2E lock/swap/undo + 框选/表格单测 |
| B3 规则 | 通过 | vitest 规则构建器/卡片（154 项含） |
| B4 快速/高级 | 通过（轮换设置可达性修复） | E2E rotation 设置展开流程 |
| B5 候选 | 通过 | E2E 生成 5 候选 + 面板单测 |
| B6 诊断 | 通过 | audit/diagnostics 单测 + 浏览器目检（§19.25） |
| B7 历史/轮换 | 通过（load 崩溃修复） | E2E 保存→重开→加载全链 |
| B8 导入 | 通过 | E2E 上传→映射→确认 + 披露交互 |

## 5. 残余目检项（不阻塞，登记）

- Windows Word/Excel 中文渲染（LibreOffice 环境无法作为判据，
  §19.27 保留为发布前平台验收项）；
- 真实教室打印机输出效果（纸张/墨色）需硬件环境；
- 候选 n=1/5/20 golden 扩展（§19.20 已登记）。
