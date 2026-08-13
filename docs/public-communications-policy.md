# SeatTrellis 对外口径（Public Communications Policy，2026-08-13）

> 本文档定义所有**对外发布内容**（GitHub Release、公告、issue 回复、用户
> 文档）的口径与边界。内部规划文档（总计划、ledger、产品决策、审计、
> 迁移指南等）**不得**被引用或泄漏到对外内容中。
> 对外语言：**英文**（用户要求：release log / changelog 等技术信息英文；
> README 等用户文档提供中英双语）。

## 1. 保密边界（绝不外泄）

以下内容属于内部，不出现在 Release notes、公告、sdist/wheel 包、
桌面安装包或任何对外文档中：

- `docs/SeatTrellis_v2.0.0_开发与发布总计划_修订版.md`（总计划）
- `docs/v2-parity-ledger.md`（parity 账本）
- `docs/product-decisions/`（产品决策记录）
- `docs/audit-*.md`（审计报告）
- `docs/adr/`（架构决策记录）
- `docs/migration-v1-to-v2.zh.md` 等迁移文档（对用户只给"可迁移"的
  事实陈述，不给实现细节）
- `scripts/` 下的 parity/oracle 工具链（不出现在发行包中）
- `fixtures/`（parity corpus 为内部验证证据）

**发行包清单**（sdist MANIFEST.in 强制）：包源码 + README(中/英) +
CHANGELOG + LICENSE/NOTICE + SECURITY/CONTRIBUTING + 示例数据。

## 2. 产品对外叙事（公开事实，不涉内部）

- SeatTrellis 是本地优先的课堂排座工具：导入名单 → 生成排座 → 手工
  调整 → 导出/打印。
- v1（Python）线已完结于 1.9.0 并进入维护；新功能在 v2（Rust-only）
  线开发。
- v2 安装包不含 Python 运行时，安装体积在 5–20MB 目标内。
- 迁移：v1 项目文件与工件可自动迁移（迁移前自动备份）；对用户只需
  说明"如何迁"，不引用内部迁移文档。
- 隐私：本地处理，无账号/云同步；公共导出自动匿名。

## 3. Release notes 模板（英文，段落式，无手动换行）

```markdown
# SeatTrellis <版本> — <一句话定位>

<本版变化，用户可见，英文>

## Install
<安装命令 + 校验和说明>

## Release assets
<资产清单 + SHA256SUMS 说明>

## Notes
<签名状态（未签名时用 PD-D19 措辞）、平台警告、已知问题>
```

规则：
- 每段一行（GitHub 自动换行），不用 80 列硬换行；
- 不出现"plan / ledger / decision / audit / migration guide"等内部
  文件引用；需要表达"可迁移"时写事实句；
- 未签名发布必须包含 PD-D19 的签名状态段落（英文措辞见
  `docs/product-decisions/2026-08-13-code-signing-rc1.md` 的
  "Release notes 措辞"节）。

## 4. 双语文档矩阵

| 文档 | 语言 |
|---|---|
| README / 用户指南 / 输入格式 / 快速开始 | 中英双语（README.md + README.en.md 等） |
| CHANGELOG / Release notes / issue / PR | 英文 |
| 内部 docs/（总计划、ledger、决策、审计） | 中文（不对外） |
