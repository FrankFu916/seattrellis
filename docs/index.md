# SeatTrellis · 席序

SeatTrellis 是一个隐私优先、本地运行的课堂排座工具。它将学生名单、教室布局、规则和历史座位记录组合成可复现的单方案或多候选方案。

## 从这里开始

- [快速开始](quickstart.zh.md)：安装、校验、求解和导出；
- [Web 端](web.zh.md)：分步向导、候选比较和下载；
- [输入格式](input-format.zh.md)：学生、layout 和产物格式；
- [规则](rules.zh.md)：hard constraints、soft preferences 和 presets；
- [隐私](privacy.md)：真实班级数据的本地处理边界。

## 核心原则

1. Hard constraints 永远优先于 soft scoring。
2. 相同输入和 seed 应生成可复现结果。
3. 不可计算的评分维度标记为 `not_available`，不虚构分数。
4. 默认不采集遥测、不上传学生数据。
5. `examples/` 中只允许虚构数据。

