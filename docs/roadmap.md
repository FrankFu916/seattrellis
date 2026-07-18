# 开发路线图

本路线图描述 v1.3–v1.8 的公开方向。具体范围以对应 GitHub Milestone 和 Issue
为准；未进入 Milestone 的构想不构成发布承诺。

| 版本 | 主题 | 主要交付 |
|---|---|---|
| [v1.3.0](https://github.com/FrankFu916/seattrellis/milestone/1) | 导出与隐私 | 统一导出请求、CLI/Web 隐私选项、A4 页面设置、中英文导出 |
| [v1.4.0](https://github.com/FrankFu916/seattrellis/milestone/2) | 内核边界与基准 | SolverBackend、规则编译、候选集报告、40/50/60 人基准、Rust spike |
| [v1.5.0](https://github.com/FrankFu916/seattrellis/milestone/3) | 人工调整 | 锁定、交换、未入座区、撤销/重做、局部修复、操作日志 |
| [v1.6.0](https://github.com/FrankFu916/seattrellis/milestone/4) | 可视化编辑与导入 | layout 编辑器、Excel 字段映射、增量更新、浏览器 E2E |
| [v1.7.0](https://github.com/FrankFu916/seattrellis/milestone/5) | 新规则与工作台 | 成绩位置/分布、师徒结对、多期轮换、历史与项目包 |
| [v1.8.0](https://github.com/FrankFu916/seattrellis/milestone/6) | 桌面与高级导出 | pywebview 原型、安装包、SVG/PPTX、原生文件对话框 |

## 跨版本工程要求

- v1.x 保持既有 CLI、service API 和文件格式兼容；
- v1.4 起 solver backend 必须可显式选择，并清楚区分超时、未知状态和真正无解；
- RuleSet 在增加复杂规则前获得独立 schema version 和迁移命令；
- Python 与 Rust 后端通过同一契约测试；
- 任何默认 backend 变更必须由真实基准和结果质量数据支持；
- 每个版本都必须通过三平台测试、包内容检查和隐私检查。

## 当前阶段

当前并行推进 v1.4 的内核边界、基准与可选 Rust 试验，以及 v1.5 编辑能力所需的
稳定领域和前端协议。原生内核继续作为可切换实验后端，不替换 Python，也不阻塞
导出、隐私和交互功能交付。
