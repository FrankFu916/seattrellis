# SeatTrellis v2 — 候选比较报告生成/打印能力移除决定（2026-08-13）

> decision_id：`PD-D16-CANDIDATE-REPORT-RETIREMENT`
> 状态：冻结；适用于 v2.0.0
> 依据：总计划 §0.5/§7.9/§8.3、`PD-D5-CANDIDATES` 已冻结的候选比较形态。

## 决定

v2 不再生成或导出 v1 的独立 `PlanComparisonReport` HTML 打印报告。
候选比较产品能力由已实现的 D5 交互式候选面板承担：推荐理由、硬约束结果、
方案差异高亮、七维评分/逐规则明细以及复现信息均直接来自 Rust
`PlanScore`、audit 与 candidate distance。

保留 `plan-comparison-report` 的严格 Rust DTO、schema 与只读解析能力，确保
历史 v1 工件仍可识别、校验和迁移评估；不保留新的报告生成器或打印 renderer。

## 理由与取舍

- D5 已按教师实际问题冻结为交互式四视图融合，独立静态报告会形成第二套
  推荐解释与本地化文案真相，并增加两套行为长期漂移的风险。
- v2 的常用分享/打印任务由座位表 public/teacher 导出承担；候选比较是生成时的
  决策辅助，不是最终班级公示工件。
- 保留只读 DTO/schema，避免通过删除格式识别来规避历史兼容责任。

未选择的方案是移植 v1 HTML renderer。其优点是保留低频打印入口；代价是
在 M5 feature-complete 后新增生成与本地化表面，且与 D5 冻结交互重复，因此
不纳入 v2.0.0。

## 迁移方案与用户影响

| v1 使用方式 | v2 替代路径 | 用户影响 |
|---|---|---|
| 生成 `PlanComparisonReport` JSON | 工作台候选面板查看相同决策信息；历史 JSON 仍可由 Rust DTO 校验 | 自动化脚本不能再生成新的 report JSON |
| 导出候选比较 HTML | 在工作台完成候选选择后，导出最终座位表；需留档时使用系统打印/截图工作台比较视图 | 独立候选比较打印模板消失（低频） |

这两项在 parity ledger 中由 `PYTHON_ONLY` 改为
`INTENTIONALLY_REMOVED_V2`；理由、替代路径和影响以本记录为准。若未来要恢复，
必须作为 v2.x 新能力重新经过 Product Decision，并由 Rust application 层生成
唯一解释契约。
