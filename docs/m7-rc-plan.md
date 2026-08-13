# SeatTrellis v2 — M7 RC 发布候选计划（2026-08-13 起草）

> 依据：`SeatTrellis_v2.0.0_开发与发布总计划_修订版.md` §10（M7 RC）。
> 状态：**计划**——M6 beta.1 retirement 完成且 CI 复验通过后进入。
> 诚实记录原则：7 天 soak 与三平台真实硬件验收未完成前，不得宣称 RC 通过。

## 1. rc.1 候选定义

- **候选 commit**：M6 retirement 链收尾后的 main 头（当前 e594cc5 之后的
  最终验收 commit）。
- **版本号**：v2.0.0-rc.1（Tauri 壳版本、Cargo workspace 版本同步）。
- **RC 冻结范围（§10.1）**：
  - lockfile 冻结（`cargo test --locked` 全链）；
  - schema 冻结（12 个 `.v2.` schema + contract check）；
  - API protocol 冻结（OpenAPI + generated.ts + ErrorEnvelope）；
  - installer metadata 冻结（tauri.conf.json 版本/图标/标识）；
  - dependencies 非安全原因不升级（dependabot 冻结窗口）；
  - 每个 fix 必须带 regression test。

## 2. RC 门禁清单（rc.1 发布前必须全绿）

| 门禁 | 证据 |
|---|---|
| Rust 三平台 test/clippy/fmt/contract/msrv/no-python | CI Rust workflow |
| Tests：parity-oracle（1.9.0 冻结 oracle 重放）+ differential（41/34/374/38）+ E2E 4/4 | CI Tests workflow |
| 打包：`cargo tauri build` 三平台 + 安装包体积 5–20MB 红线 | tauri.yml 手工触发 + 本地验证 |
| 安装：干净机器（无 Python/无 Node）安装 + 启动 + 全流程 | 发布红线验收 |
| 迁移：v1.9.0 项目 → v2 工作台全走 Rust（migration 单/批/bundle） | E2E + CLI 生命周期测试 |
| 隐私：privacy 扫描 + 无真实数据泄漏（hygiene 门禁） | CI + 手动抽查 |

## 3. RC soak（§10.2，7 天实际使用观察）

- **方式**：rc.1 构建物交给真实教师/团队日常使用 7 天；记录问题到
  GitHub issues（P0/P1 分级，§16 Bug 分级）。
- **观察项**：无新增 P0/P1；无数据损坏；无 hard-rule correctness 问题；
  无跨平台安装 blocker；无迁移 blocker；无 privacy blocker。
- **soak 期间允许**：bug fix / performance / security / accessibility /
  migration-export 正确性 / 文档修正（§9.2 beta.2 规则）。
- **soak 期间禁止**：新规则/新插件/新数据模型/新大 UI/大规模重写。
- **结论规则（§10.2）**：若 soak 期间修复核心逻辑 → 重新发 RC 并重新
  开始关键验证；不把修改后的 commit 直接标 final。

## 4. 平台验收（真实硬件，禁止伪造）

- **Windows**：Word/Excel 中文渲染（DOCX/XLSX 重开）、PDF 查看器、
  打印机输出、Tauri 安装包（NSIS）安装/卸载。
- **macOS**：dmg 安装、系统字体（PingFang）回退、Gatekeeper 公证
  （如适用）、打印机。
- **Linux**：.deb 安装、CJK 字体（Noto）、打印机。
- 对照清单：`docs/product-decisions/2026-08-12-platform-acceptance-checklist.md`。
- **签名**：三平台代码签名/公证为发布红线项；无签名环境时如实记录为
  RC 阻断项而非假装完成。

## 5. rc.2 判定

- 仅当 rc.1 在完整验证与 soak 期间**零代码修改需求**时，可跳过 rc.2
  （§10）；否则按 §10.2 规则发 rc.2 并重跑关键验证。

## 6. Final Gate（§17）

- 全部 RC 门禁绿 + soak 7 天无 P0/P1 + 平台验收记录完整 + 签名完成 →
  v2.0.0 final 候选；发布决定由产品负责人确认（本计划不自行发布）。
