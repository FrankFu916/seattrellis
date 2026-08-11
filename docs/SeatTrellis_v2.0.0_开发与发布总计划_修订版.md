# SeatTrellis v2.0.0 开发与发布总计划（修订版）

> 基线日期：2026-08-07  
> 当前稳定线：v1.8.4  
> 分析基准源码：`282fd99a7e766aaedaea6c5bb4c61e3ef14d257c`  
> 目标：将 SeatTrellis 从 Python/Rust 双栈迁移为 **Rust 原生后端 + React/TypeScript 工作台**，并将 v2.0.0 作为经过多轮重构、差分验证、真实使用、压力测试、修复和候选版冻结后的长期稳定基线。**v2.0.0 是质量里程碑，不是时间里程碑：只要任一关键门槛没有满足，就继续发布 v1.9.x 或 v2.0.0 预发布版本，绝不为了版本号或日期强行发布 final。**

---

## 0. 先定义“v2.0.0 完成”到底意味着什么

### 0.1 “纯 Rust”的正式定义

v2.0.0 的生产架构为：

```text
React / TypeScript Workbench
          │
          ▼
Rust local API / Tauri desktop shell
          │
          ▼
Rust application layer
          │
          ├── domain / schema / validation
          ├── rule compiler / feasibility diagnostics
          ├── solver / scoring / candidate generation
          ├── editing / repair / history / rotation
          ├── project / migration / privacy / import
          └── export / rendering
```

正式版 **不得依赖**：

- Python 运行时；
- Pydantic；
- FastAPI / Starlette；
- Streamlit；
- OR-Tools Python；
- PyO3 / `seattrellis_native` Python 扩展；
- pywebview；
- Python CLI；
- Python 打包、wheel、PyPI 作为 v2 主分发方式。

Node.js 仅允许作为 React **构建时**工具，不进入正式安装包；正式桌面包运行时只需要 Rust/Tauri 产物及系统 WebView。

Python v1.x 在迁移阶段继续作为行为 oracle；进入 v2 RC 前从主线删除。旧 Python 版本通过 Git tag 和 `v1.x-maintenance` 分支保留，不在 v2 仓库中继续双轨维护。

### 0.2 “完整正式版”的定义

v2.0.0 不要求把所有能想到的功能全部加入，而要求以下产品主流程全部达到正式质量：

> 新建/打开班级 → 导入/维护学生 → 设置教室 → 设置规则与目标 → 生成并比较方案 → 手动调整 → 诊断 → 历史/轮换 → 保存/迁移/备份 → 导出/打印

以下能力不能因移除 Python 而倒退：

- v1.x 项目、RuleSet、snapshot、rotation 等长期文件可读取和安全迁移；
- 所有当前正式 hard rules 与 soft objectives；
- 历史公平、近期邻座、关系冷却、多期轮换；
- 编辑、锁定、撤销/重做、局部修复；
- 候选方案、评分与解释；
- 项目包、迁移、隐私扫描、恢复；
- React 工作台完整流程；
- CLI 的核心自动化能力；
- 当前正式支持的导出用途不得出现无说明的大幅退化。

### 0.3 “无 Bug”的工程定义

软件无法从数学上证明绝对不存在 Bug，因此正式发布门槛定义为：

- **0 个已知 P0 / P1 Bug**；
- 核心求解、数据安全、迁移、隐私、导出中 **0 个已知 P2 Bug**；
- 不存在已知 hard-rule 违规却被标记为合法结果的情况；
- 不存在启发式搜索失败却被错误宣称为“已证明无解”的情况；
- 不存在已知数据丢失、静默覆盖、迁移不可回滚或隐私泄漏问题；
- 所有正式支持平台的发布矩阵连续多轮全绿；
- RC 冻结后只允许修复，不允许继续塞功能。


### 0.4 v2.0.0 的发布哲学：允许无限延期，不允许降低门槛

本计划**不设强制发布日期**。版本进入下一阶段只依据 Exit Gate，而不是日历。

允许出现：

- `v1.9.7`、`v1.9.18` 甚至更多 v1.9.x；
- `v2.0.0-alpha.N` 多轮返工；
- `v2.0.0-beta.N` 长期真实使用；
- `v2.0.0-rc.N` 因 blocker 反复重发；
- 发现架构级问题后从 RC 退回 beta，甚至重新打开某个迁移里程碑。

不允许出现：

- “已经宣传 2.0，所以必须发”；
- “只剩少数已知严重问题，先发再修”；
- “为了赶版本，把未完成能力隐藏起来但仍称完整 parity”；
- “测试没覆盖到，所以假定没问题”；
- “RC 期间顺便塞一个新功能”。

Final 的含义是：**当前构建已经被证明达到既定质量基线，而不是开发周期结束。**

### 0.5 产品与 UX 不提前锁死：使用 Decision Gate

v2.0.0 必须解决核心用户问题，但当前还没有经过真实使用验证的交互细节，不在本计划中提前指定唯一实现。对于画布、规则编辑器、导航、候选比较、历史工作流、导入确认、导出面板、新手引导等产品问题，统一采用：

```text
问题定义
  ↓
约束与成功指标
  ↓
2–3 个可运行原型/候选方案
  ↓
真实任务测试 + dogfood + 可用性记录
  ↓
Decision Gate
  ↓
冻结最终交互契约
  ↓
实现 + E2E + 回归
```

每个 Product Decision 至少记录：

- `decision_id`；
- 要解决的真实任务；
- 已知用户痛点；
- 不可违反的领域/安全约束；
- 候选方案及 trade-off；
- 需要收集的证据；
- 最终决定与理由；
- 冻结阶段；
- 是否会影响 schema/API/editing protocol。

建议新增 `docs/product-decisions/` 或 `docs/adr/product/`。在 Decision Gate 通过之前，roadmap 只规定**必须达到的用户结果和验收标准**，不规定具体按钮位置、面板形态或交互手势。

---

# 1. v2.0 的架构目标

## 1.1 将当前多 workspace 收敛为单一 Rust workspace

当前 `native/`、`app/`、`app/src-tauri/` 之间存在多个独立 workspace，且 `app/src/server.rs` 已成为新的大型适配层。v2 前应完成一次结构性收敛。

建议最终布局：

```text
Cargo.toml                     # 单一 workspace
crates/
  seattrellis-domain/          # Student/Layout/Rule/History/Snapshot/Project
  seattrellis-schema/          # versioned DTO、schema、migration primitives
  seattrellis-rules/           # RuleSpec、编译、诊断、feasibility precheck
  seattrellis-solver/          # hard search、soft optimization、candidate engine
  seattrellis-application/     # use cases；唯一业务编排层
  seattrellis-io/              # roster/project/archive/import/privacy
  seattrellis-export/          # SVG/PNG/PDF/HTML/Office 输出
  seattrellis-server/          # loopback HTTP；只做 transport adapter
  seattrellis-cli/             # CLI adapter
  seattrellis-testkit/         # fixtures、generators、golden helpers，仅 dev
app/
  src-tauri/                   # 薄 Tauri shell
clients/web/                   # React/TS；保留
xtask/                         # 构建、生成 schema、发布检查
schemas/
fixtures/
```

### 强制依赖方向

```text
transport/UI
    ↓
application
    ↓
domain + rules + solver + io + export
```

禁止：

- solver 依赖 HTTP；
- domain 依赖 Tauri；
- React 自行实现 hard-rule 判定；
- `serde_json::Value` 在深层领域逻辑中长期传播；
- server handler 直接完成复杂业务编排；
- 为 CLI/Web/Desktop 分别实现三套业务逻辑。

## 1.2 拆掉新的单体热点

优先重构：

- `app/src/server.rs`；
- Python 阶段的 `src/seattrellis/api/handlers.py` 只作为迁移参照，不继续扩张；
- `crates/seattrellis-core/src/lib.rs` 中 solver/evaluator/validation 混合逻辑。

目标：

- route 只负责解析、认证、错误映射和序列化；
- use case 放入 application；
- DTO 与 domain model 明确转换；
- 每个模块保持可独立单测；
- 错误类型使用结构化 enum，不依赖字符串匹配。

## 1.3 建立稳定的 v2 错误与状态模型

求解状态必须从 `feasible: bool` 升级为明确状态，例如：

```text
Solved
ProvenInfeasible
Timeout
Unknown
InvalidInput
Cancelled
InternalError
```

尤其必须修复当前启发式路径潜在的语义问题：**“若若干次 greedy attempt 没找到解”只能得到 `Unknown/SearchExhausted`，不得等价于 `ProvenInfeasible`。**

所有 API、CLI、UI 使用同一错误码与状态码，错误信息另外本地化。

---

# 2. 发布列车与阶段划分

版本号与工程里程碑解耦。M0–M4 是**能力成熟度阶段**，不提前绑定未来具体 patch 号；只要 v2.0 尚未成熟，稳定/预览交付可以继续沿 `v1.9.x` 线发布。

| 阶段 | 对外版本策略 | 主题 | 新功能策略 |
|---|---|---|---|
| M0 | v1.8.x 最终基线修复或进入 v1.9.0 | 基线冻结、parity inventory、测量 | 仅修复/测量 |
| M1 | v1.9.x | Rust 架构收敛与契约整理 | 仅迁移/架构必需 |
| M2 | v1.9.x | Rust 应用层完整 parity | 仅为消除 Python 依赖所需 |
| M3 | v1.9.x | Solver、诊断、规则模型可靠化 | 严格受控；先解决正确性 |
| M4 | v1.9.x | 产品完整性、原型验证、UX 决策冻结 | 只做经 Decision Gate 证明必要的功能 |
| M5 | `v2.0.0-alpha.N` | Rust-only 全流程切换 | 禁止无关 scope expansion |
| M6 | `v2.0.0-beta.N` | Feature complete、真实使用、稳定化 | 默认冻结；必要设计修正需重新过 Gate |
| M7 | `v2.0.0-rc.N` | 发布候选、代码/schema/protocol 冻结 | 仅 blocker 修复 |
| M8 | `v2.0.0` | 正式长期稳定基线 | 仅在所有 Final Gates 通过后发布 |

### 2.1 v1.9.x 的角色

`v1.9.x` 不是“快到 2.0 的倒计时”，而是迁移期间可以长期存在的生产/预览线。它允许：

- Python/Rust parity 工作；
- Rust 架构重构；
- UI 原型和用户测试；
- 迁移工具验证；
- 关键 bug 修复；
- 为 v2 建立新契约，但仍保留必要兼容路径。

如果 v2 的任何关键问题没有解决，就继续 v1.9.x。**不需要为了“版本号难看”升级到 2.0。**

### 2.2 预发布版本语义

- **alpha**：Rust-only 架构已经可以端到端运行，但允许发现设计错误后重构；
- **beta**：功能范围原则上完成，重心从开发切换到真实使用、兼容性和缺陷发现；
- **RC**：若没有新 blocker，这个构建本身就应该能够成为 final。RC 不再承担产品探索。

如果 RC 暴露系统性 UX、schema、solver 或迁移设计问题，应退回 beta/M4 重新决策，而不是在 RC 中强行打补丁掩盖。

---

# 3. M0 — v1.8.5：建立不可争议的迁移基线

这一阶段不要做大功能。目的只有一个：在继续迁移之前，把“现有系统到底有什么行为”记录清楚。

## 3.1 建立 Parity Ledger

新增 `docs/v2-parity-ledger.md`，逐项记录：

- Python public CLI 命令；
- service/application 公开用例；
- React 当前调用的全部 `/api/v1/*`；
- Project、RuleSet、Snapshot、CandidateSet、Rotation、Editor protocol schema；
- roster import/mapping/update；
- layout editor；
- hard rules；
- soft objectives；
- history/pair history；
- candidate generation/comparison；
- repair/editing；
- export formats/options/privacy modes；
- migration、backup、restore；
- desktop native file workflows。

每项状态只允许：

```text
PYTHON_ONLY
RUST_PARTIAL
RUST_PARITY_PENDING
RUST_VERIFIED
INTENTIONALLY_REMOVED_V2
```

`INTENTIONALLY_REMOVED_V2` 必须有书面理由、迁移方案和用户影响说明。

## 3.2 固化差分 corpus

建立 `fixtures/parity/`：

- 20/40/50/60/80 人；
- 座位数 = 人数、略多于人数、明显多于人数；
- 标准矩形、过道、禁用座位、异形布局、自定义 adjacency；
- 无规则、稀疏规则、密集规则；
- 各类 hard rule 单独与组合；
- 全部 soft objectives；
- 无历史、短历史、长历史；
- 多期轮换；
- 缺失成绩/身高/视力/标签；
- Unicode/中文姓名/长字段；
- 边界与非法输入。

Python v1.x 在此阶段输出 golden：

- 编译后的 hard constraints；
- objective breakdown；
- history/pair history；
- schema migration 结果；
- export metadata；
- 可复现 seed 下的候选结果。

注意：对启发式解不要求 Rust 与 Python 得到完全相同座位表；要求的是**语义、合法性、评分定义和质量门槛**一致。

## 3.3 固化性能基准

保留已有 40/50/60 基准，再增加：

- 80 人压力场景；
- 5/20 candidates；
- 高密 hard rules；
- 10/30/100 期历史统计；
- 大项目包导入/恢复；
- Office 导出；
- 冷启动与内存。

结果必须存 JSON + Markdown，不凭主观感觉判断“更快”。

## 3.4 M0 退出条件

- parity ledger 覆盖全部公开主流程；
- golden fixtures 有版本号和来源 commit；
- 当前 Rust/Python gap 全部列明；
- 所有已知高风险迁移点建立 Issue；
- v1.8.5 只包含安全修复、测试基础和测量工具。

---

# 4. M1 — v1.9.x：Rust 架构收敛与契约冻结

## 4.1 重组 Rust workspace

完成第 1 节的 crate 分层。此时不要急着增加新规则。

要求：

- `#![forbid(unsafe_code)]`，除非某个极小模块有不可避免且经过审计的 unsafe；
- workspace 统一 rust-version/MSRV；
- `cargo fmt --check`；
- `cargo clippy --all-targets --all-features -- -D warnings`；
- 锁定依赖版本；
- 每个 crate 明确职责和 public surface；
- 尽量减少 feature coupling。

## 4.2 DTO 与领域模型分离

当前 JSON 契约可继续兼容，但内部建立 typed DTO → domain conversion。

要求：

- boundary 可以容忍旧字段；
- domain model 不保存 transport-only 字段；
- unknown field 策略明确；
- schema version 在一个中心 registry 管理；
- canonical serialization 明确，便于 hash、diff、fixture。

## 4.3 Capability API

落实原 `rust-migration.md` 中尚未完成的 capability response。

至少返回：

- supported schema versions；
- supported rules/objectives；
- supported imports/exports；
- supported editor commands；
- solver capabilities；
- build/version/platform；
- migration capabilities。

React 不应“试着调用然后失败”，而应据此隐藏或禁用未支持操作。

## 4.4 统一 error taxonomy

建立稳定错误码，例如：

```text
invalid_input.*
reference.*
constraint.*
solve.*
project.*
migration.*
import.*
export.*
security.*
internal.*
```

错误响应包含：

- stable code；
- human message key；
- relevant entity/rule；
- recoverable；
- suggested action；
- debug details（仅开发模式）。

## 4.5 M1 退出条件

- Rust workspace 收敛完成；
- `server.rs` 不再承担业务逻辑；
- React 正常跑现有主流程；
- v1 文件仍能读；
- capability/error contract 有 schema 和测试；
- 迁移重构没有用户可见功能回退。

---

# 5. M2 — v1.9.x：完成 Rust 应用层功能等价

这一阶段的原则是：**先完整迁移现有正式能力，再新增功能。**

## 5.1 Student / roster

Rust 原生完成：

- CSV、XLSX 导入；
- 自动姓名列识别；
- 字段映射；
- 差异预览；
- 全量覆盖与增量更新；
- 数据校验；
- Unicode/中文；
- 大名单和空行/脏数据处理。

吸收 Seatflow/open_fuckseats 优点：

- 模糊表头同义词；
- 行式/列式结构识别；
- 映射置信度；
- 拼音全拼/首字母；
- 自然数字排序；
- 多字段排序模板。

## 5.2 Layout

Rust 成为唯一 layout 编译权威：

- 标准教室生成；
- 行列增删；
- 过道；
- 讲台；
- 禁用/空位；
- 镜像/平移；
- 异形布局；
- 显式邻接图；
- zone/group/tags/attributes；
- layout schema migration。

React 只能提交 layout commands。

## 5.3 Project / migration / backup

Rust 完整接管：

- recent projects；
- project validate；
- project history；
- `.seattrellis.zip` bundle；
- privacy scan；
- migration preview/apply/batch；
- backup/restore；
- reference checks；
- rotation outputs；
- group registers。

新增可靠性：

- archive 内每个逻辑块 hash；
- 整体 manifest hash；
- zip bomb/路径穿越/symlink 防护；
- 原子写入：temp → fsync → rename；
- 原地迁移必须先备份；
- 批量迁移采用 all-or-nothing transaction；
- 导入失败不修改原文件。

## 5.4 Editing / repair

Rust 接管完整编辑协议：

- `draft_id`；
- revision；
- `command_id`；
- swap/move/unseat/seat/lock；
- batch；
- undo/redo；
- local repair；
- operation log；
- snapshot provenance。

增加 property/model-based tests：随机生成数百条命令序列，保证：

- revision 单调；
- undo/redo 可逆；
- 不重复占座；
- locked semantics 不漂移；
- failed command 原子回滚。

## 5.5 CLI 完整化

Rust CLI 不能只保留 `validate/solve/export`。

v2 CLI 至少覆盖：

- version/help/doctor；
- validate；
- solve/candidates；
- project info/validate/solve/edit/repair/export；
- history/pair reports；
- rotation；
- schema inspect/export/migrate；
- project bundle/privacy/migration；
- demo/sample；
- machine-readable JSON output；
- 稳定 exit code。

CLI 和 local API 调用同一个 application crate。

## 5.6 Export parity

Rust 需要统一 `ExportRequest` 和 render model。

必须支持当前正式用途：

- SVG；
- PNG；
- PDF；
- HTML/print HTML；
- Excel；
- DOCX；
- PPTX；
- 教师版/公开版隐私策略；
- 中英文；
- A4 横/竖；
- 单页 16:9 等既有模板。

实现策略：先建立与格式无关的 `PresentationModel`，各 exporter 只负责编码，不允许每个 exporter 自行重新解释座位规则。

Office 格式先进行 crate/OOXML 技术评估；若第三方 crate 成熟度不足，宁可实现受控的最小 OOXML writer，也不要为方便引入不稳定或体积巨大的依赖。

## 5.7 M2 退出条件

- parity ledger 中 application/IO/editing/project/export 主流程全部至少 `RUST_PARITY_PENDING`；
- React 普通流程可完全绕过 Python；
- CLI 能完成一整个项目生命周期；
- 所有项目写操作具备 rollback；
- Python 仍存在，但只作为 compatibility/oracle，不再承担 v2 新功能。

---

# 6. M3 — v1.9.x：Rust Solver v2、可行性预检与解释体系

这是 v2 成败最关键的一阶段。

## 6.1 Hard constraints：从“多次贪心”升级为可靠搜索

借鉴 SeatingChartEditor2 的预检思想，但在 Rust core 中实现。

### 第一层：静态冲突

生成前立即检测：

- 不存在的学生/座位/区域；
- 同一学生多个固定座位；
- 多人固定同一座位；
- 同一对同时 must/cannot adjacent；
- 距离规则与固定座位冲突；
- group expand 后产生矛盾；
- 区域/标签容量明显不足。

### 第二层：候选域

为每名学生建立当前 hard rules 下的 candidate seat domain。

输出：

- candidate count；
- 哪些规则排除了哪些座位；
- 最紧张学生；
- 空 domain 直接给出原因。

### 第三层：全局匹配预检

使用二分图最大匹配检测“每人都有候选，但整体不能一一落座”的情况。

对能形成 sound 证明的情形返回 `ProvenInfeasible`；无法证明的复杂关系进入搜索，不做错误结论。

### 第四层：完整 hard-search

建议采用：

- MRV（最少剩余候选学生优先）；
- degree/constraint impact tie-break；
- forward checking；
- adjacency/min-distance propagation；
- matching-aware pruning；
- 固定学生和高约束块优先；
- 有限 backtracking；
- 可取消和 time budget。

搜索耗尽且覆盖了完整状态空间时才允许 `ProvenInfeasible`；达到时间/节点预算但未找到解必须返回 `Unknown/Timeout`。

## 6.2 Soft optimization：从 greedy ranking 升级为稳定 local search

在已合法初始解上优化，不让 soft search 破坏 hard correctness。

借鉴 SeatingChartEditor2 的优点：

- 优先选择当前 objective/violation 贡献大的学生；
- swap；
- move-to-empty；
- desk-mate/block move；
- small-cycle move；
- 局部 repair；
- stagnation detection；
- controlled reheating；
- ruin-and-recreate/LNS 风格 neighborhood；
- multi-start。

所有随机均使用统一 deterministic RNG；相同版本、相同输入、相同 backend、相同 seed 应可复现。

## 6.3 Candidate engine

候选不应只是重复不同 seed。

加入：

- seed derivation 明确记录；
- diversity penalty；
- 与最佳方案的 assignment distance；
- hard validity 100% 后才进入 candidate set；
- 质量差距和主要 objective breakdown；
- 推荐理由；
- reproducibility metadata。

## 6.4 Rule metadata / Rule DSL

吸收 SeatingChartEditor2 与 open_fuckseats 的优点，建立唯一 `RuleSpecRegistry`：

每条 rule spec 描述：

- stable rule id；
- version；
- hard/soft；
- subjects；
- parameters schema；
- defaults/range/enums；
- required student/layout attributes；
- compiler；
- validator；
- evaluator；
- explanation template；
- capability metadata。

优先新增/正规化：

- tag → allowed/forbidden zone；
- same-tag separation；
- tag A/B relation；
- row/zone range；
- score gradient；
- grouped score balance；
- layered distribution。

React 的规则表单由 metadata 驱动，但业务判定仍在 Rust。

## 6.5 Rule audit / explanation

每个最终候选必须能回答：

- 哪些 hard rules 已检查；
- 每条 hard rule 是否满足；
- 每个 soft objective 的 raw loss、weight、weighted cost；
- 哪些学生/座位贡献最大；
- 哪些规则因为数据缺失未参与；
- 历史公平和近期邻座如何影响结果。

## 6.6 Solver 质量门槛

在 Python OR-Tools 尚存在时做最终大规模差分。

必须达到：

1. **合法性**：官方 corpus + 随机生成 corpus 中 Rust 返回 `Solved` 的结果 hard-rule violation = 0；
2. **假无解**：0 个启发式失败被标成 `ProvenInfeasible`；
3. **已知可行 corpus**：正式 corpus 100% 找到合法解；随机可行压力 corpus 目标 ≥99.5% 在标准时间预算内找到，其余返回 Unknown/Timeout；
4. **评分语义**：Rust/Python 对同一固定 assignment 的 objective breakdown 完全一致或在浮点容差内一致；
5. **质量**：相对 OR-Tools 基准的 normalized regret 中位数目标 ≤5%，P95 ≤15%；如果某 objective 不能用比例衡量，则建立对应绝对误差门槛；
6. **回归**：不得低于当前 Python fallback 的质量基线；
7. **性能**：40/50/60 人标准任务保持交互级响应，性能不得较当前 Rust 基线退化 >10% 而无充分理由。

这些数字是 release gate，不是宣传语；若实测表明不合理，应在 M3 前半段用基准数据修改一次，之后冻结。

## 6.7 M3 退出条件

- 求解状态语义正确；
- feasibility report 可被 UI 消费；
- hard-search 与 soft optimizer 分离；
- official parity corpus 全绿；
- rule registry 生效；
- Python OR-Tools 不再是正常用户流程必须依赖的 backend。

---

# 7. M4 — v1.9.x：产品完整性、原型验证与交互冻结

这是 v2 前最后一个允许**产品方案仍然开放**的阶段。目标不是一次性实现一张“理想界面清单”，而是把关键用户任务通过原型和真实使用验证后逐项冻结。M4 结束时必须得到稳定的产品契约；M5 alpha 以后不再把大规模 UX 探索当作常规开发。

## 7.1 建立 Product Decision Backlog

至少为以下问题建立 Decision 项，而不是预先指定最终方案：

1. **班级主导航**：单页工作台、步骤式流程、侧栏工作区如何组合；
2. **Seating canvas**：pan/zoom、选择、拖拽、批量操作、触控、键盘访问的最终交互；
3. **Rule builder**：句式构建器、表单、规则卡片、画布上下文操作如何组合；
4. **快速排座 vs 高级模式**：默认隐藏哪些技术参数，何时展开；
5. **候选方案比较**：如何在不增加认知负担的情况下解释质量差异；
6. **可行性诊断**：问题列表、学生高亮、座位高亮、修复建议如何串联；
7. **历史/轮换**：历史是“时间线”“版本”“周期计划”还是组合模型；
8. **导入确认**：映射、预览、冲突、rollback 的最自然流程；
9. **导出/打印**：快速导出和高级版式设置如何分层；
10. **新手引导**：是否采用临时示例班级、内嵌任务或首次运行 checklist。

每个 Decision 必须有真实任务场景，不能只讨论视觉偏好。

## 7.2 Seating canvas：规定能力，不规定最终手势

在 M4 结束前，座位编辑必须具备这些**结果能力**：

- 用户能在大教室中快速定位、缩放和移动视图；
- 能单选、多选和批量移动；
- 能明确看到锁定、违规、未入座和选中状态；
- 非法操作在提交前可被识别或在提交后得到明确诊断；
- 所有持久编辑都通过 Rust `editing_protocol` 命令；
- undo/redo 不因 UI 重构而破坏；
- 键盘可完成核心操作；
- 触控支持是否作为 v2 final 必须项，由原型测试 Decision Gate 决定。

候选方案可以参考 SeatingChartEditor2 的 pan/zoom、框选、多拖拽等机制，但是否原样采用要通过可用性验证。

### Canvas Decision Gate

至少使用：

- 30–60 人标准教室；
- 异形布局；
- 锁定学生；
- 多条 hard rule；
- 已生成方案后的局部调整；
- 连续撤销/重做；

完成 dogfood。最终方案必须在“完成任务步骤数、误操作率、规则冲突可见性、恢复能力”上达到既定指标。

## 7.3 Rule builder：规定可表达性，不规定最终 UI

普通用户在 v2 final 中不得被迫编辑 JSON。必须做到：

- 常用 hard/soft rules 可由 UI 创建、修改、删除；
- 标签、区域、关系、距离、成绩/属性、历史目标可以组合；
- 规则参数由 Rust RuleSpec/metadata 作为唯一 schema 来源；
- UI 能立即发现未知学生/座位、字段缺失和明显冲突；
- 生成前能查看 feasibility summary；
- 高级用户仍可检查底层规则表示，但原始 JSON 是否允许直接编辑由 Decision Gate 决定；
- UI 展示语义与 Rust compiler 语义必须一致。

候选 UI 可以包括规则句式构建器、规则卡片、分层表单、上下文菜单等。最终形态不在本计划预先指定。

## 7.4 Teacher workflow：保留原则，验证具体操作逻辑

必须支持的任务结果：

- 快速恢复最近使用的班级；
- 班级、名单、教室、规则和历史具有稳定上下文；
- 重复任务可以复用配置；
- 常用排序至少支持中国姓名/拼音/自然数字和多字段需求；
- 示例数据不会污染真实班级；
- 多期轮换结果可以批量保存/导出；
- 临时排一次与长期管理同一个班级不应形成两个互不兼容的产品概念。

是否采用“最近班级首页”“班级组”“共享模板”“临时示例班级”等具体形态，在 M4 内通过 Product Decision 逐项决定。

## 7.5 Import transaction：这是数据正确性要求，不是 UX 偏好

无论最终界面如何，导入必须具备：

- parse 与 apply 分离；
- 映射/迁移/验证在 apply 前可预览；
- apply 原子化；
- 任一步失败不得留下半应用状态；
- 可以安全回滚到导入前状态；
- 导入产生的 schema/data transformation 可审计。

确认按钮、预览页、向导还是 inline workflow 可以之后决定，但事务语义必须在 Rust application 层冻结。

## 7.6 Reproducibility UX：结果能力先冻结

用户必须能够：

- 查看本次结果的 seed、solver/build、规则版本、history range、candidate id；
- 复现同一输入/版本/seed 的结果；
- 方便地产生新的 seed；
- 复制或导出复现信息；
- 明确知道“相同 seed 但规则/历史/版本已变化”不构成同一实验。

这些信息展示在主结果页、详情页还是导出元数据中，由可用性测试决定。

## 7.7 候选比较与解释：建立证据驱动的 UX Decision

必须能回答：

- 为什么推荐方案 A；
- A 与 B 的主要 objective 差异；
- hard rule 是否全部满足；
- 哪些学生/关系/历史因素导致主要成本；
- 两个方案在座位分配上的差异有多大。

但“表格、雷达图、差异高亮、逐规则列表”不提前指定。先构建数据契约和最小原型，再依据理解速度与误读率冻结展示方式。

## 7.8 M4 明确不承诺的产品边界

以下能力**不是 v2.0.0 必需条件**，也不在本计划中承诺属于某个具体后续版本：

- 第三方任意动态代码插件；
- Python/JavaScript/Lua 脚本扩展；
- 云同步；
- 用户账号/身份系统；
- 多人实时协作；
- 远程 AI；
- 云端组织管理；
- 通用会场/极坐标布局；
- 为极少数边缘需求无限增加格式。

可以为未来保留干净的契约边界，但不能为了“也许以后要做”在 v2 引入运行时、权限系统或云架构复杂度。

## 7.9 M4 退出条件

- 所有 v2 必须解决的产品问题都有已记录的 Decision；
- 所有仍开放的 UX 问题要么完成 Decision Gate，要么明确从 v2 scope 移除；
- 普通教师无需 JSON/CLI 完成主流程；
- React 不含第二套业务规则；
- 所有已冻结交互有 Rust contract + React E2E；
- 数据正确性相关交互（导入、保存、迁移、编辑）已冻结事务语义；
- 从此进入 feature complete / feature freeze。

---

# 8. M5 — v2.0.0-alpha：Rust-only 全流程切换

## 8.1 alpha.1：切默认路径

- 默认开发、Web、Desktop、CLI 全部运行 Rust；
- Python backend 不再被 React 调用；
- CI 新增 `NO_PYTHON_RUNTIME=1` 完整主流程；
- 在没有 Python、pip、OR-Tools 的干净机器上完成 import → solve → adjust → save → export；
- v1 项目 migration 全走 Rust。

Python 代码暂时仍存在仓库中，仅用于差分测试和行为 oracle；不得再作为 v2 新功能的实现位置。

## 8.2 alpha.2：关闭 parity gap

alpha.1 收集到的问题只做修复：

- 行为差异；
- error contract；
- migration；
- export visual parity；
- solver quality；
- slow paths；
- accessibility；
- installer integration。

Parity ledger 中所有 v2 必须项必须进入 `RUST_VERIFIED`。

## 8.3 Alpha 退出条件

- 无 `PYTHON_ONLY` / `RUST_PARTIAL` 的 v2 必须项；
- Rust-only E2E 全绿；
- 所有正式 schema 有 Rust round-trip；
- Python 只剩 oracle/test reference 身份。

---

# 9. M6 — v2.0.0-beta：删除 Python，进入真正的稳定化

## 9.1 beta.1：Python retirement

先创建并保护：

- 最终 v1.x tag；
- `v1.x-maintenance` 分支；
- v1 → v2 migration documentation；
- golden fixture provenance。

然后从 v2 主线删除：

- `src/seattrellis/`；
- Python tests/e2e；
- `pyproject.toml`/Python packaging；
- Streamlit 配置；
- Python publish workflow；
- PyO3 `seattrellis_native`；
- OR-Tools/Pydantic/FastAPI/Streamlit references；
- Python desktop compatibility code。

允许文档中保留“v1 Python legacy”的历史说明，但不得存在生产依赖。

建立自动检查：最终 bundle/CLI/desktop 不得探测或加载 Python。

## 9.2 beta.2：稳定性修复

严格 feature freeze。

只允许：

- bug fix；
- performance regression fix；
- security fix；
- accessibility fix；
- migration/export correctness fix；
- documentation correction。

禁止：

- 新规则；
- 新插件；
- 新数据模型；
- 新大 UI 功能；
- 大规模重写。

---

# 10. M7 — RC：发布候选版

至少准备 `rc.1` 和 `rc.2`，除非 rc.1 在完整验证与 soak 期间完全没有需要代码修改的问题。

## 10.1 RC 代码冻结

RC 后：

- lockfile 冻结；
- schema 冻结；
- API protocol 冻结；
- installer metadata 冻结；
- dependencies 非安全原因不升级；
- 每个 fix 必须有 regression test。

## 10.2 RC soak

建议最后一个 RC 至少经历 **7 天实际使用观察**，并满足：

- 无新增 P0/P1；
- 无数据损坏；
- 无 hard-rule correctness 问题；
- 无跨平台安装 blocker；
- 无迁移 blocker；
- 无 privacy blocker。

如果期间修复核心逻辑，重新发 RC 并重新开始关键验证，而不是直接把修改后的 commit 标成 final。

---

# 11. v2 测试体系：不是“一次完整测试”，而是多层持续防线

## 11.1 每个 PR 必跑

Rust：

- `cargo fmt --check`；
- clippy `-D warnings`；
- unit tests；
- domain/schema contract tests；
- changed-module property tests；
- dependency/security quick check。

React：

- typecheck；
- ESLint；
- Vitest；
- component tests。

## 11.2 每日/主分支重测试

- 全 Rust workspace；
- React 全测试；
- browser E2E；
- parity/golden corpus；
- solver deterministic corpus；
- migration corpus；
- export validation；
- Linux/Windows/macOS。

## 11.3 Property-based tests

重点覆盖：

### Solver

- 返回 assignment 永远唯一；
- 所有学生最多一个座位；
- 所有座位最多一个学生；
- `Solved` ⇒ validator 100% 通过；
- seed determinism；
- 增加无关空座不得让 fixed rule 失效；
- 重排输入顺序但 stable key 不变时语义不变。

### Editing

- undo(apply(x)) 恢复原状态；
- redo(undo(x)) 恢复 apply 状态；
- stale revision 永远拒绝；
- failed batch 不产生 partial state。

### Migration

- migrate → validate；
- migrate → serialize → read；
- backup 永远可恢复；
- current schema normalization 幂等。

## 11.4 Fuzzing

至少建立 fuzz targets：

- JSON/DTO parser；
- RuleSet parser/compiler；
- project archive/zip；
- migration；
- CSV importer；
- XLSX importer；
- editor command sequence；
- export option parser；
- local HTTP request parser。

要求：不 panic、不 OOM、不目录穿越、不无限循环。

## 11.5 Solver 随机压力

自动生成数千组：

- known-feasible；
- known-infeasible；
- unknown/hard；
- random topology；
- random constraints；
- random histories。

对 known-feasible：Rust 必须不能返回 `ProvenInfeasible`。  
对 known-infeasible：只有经过 sound proof 才可返回 `ProvenInfeasible`。  
所有 `Solved` 必须重新走独立 validator。

## 11.6 Export tests

不能只比较“文件生成成功”。

- PNG/PDF：尺寸、分页、文字存在性、隐私字段；
- SVG/HTML：DOM/元素语义；
- XLSX：重新用独立 reader 打开并检查 sheet/cell；
- DOCX/PPTX：解包 OOXML，校验 XML/relationship/manifest；
- public export：敏感字段扫描必须为 0；
- 中文字体 fallback；
- 长姓名/空组/未入座/异形布局。

## 11.7 Migration matrix

维护真实历史 fixture：

```text
v0.x → v2
v1.0 → v2
v1.4 → v2
v1.7 → v2
v1.8.4 → v2
current-v2 → current-v2
```

每个 fixture 检查：

- 名单；
- layout；
- rules；
- history；
- outputs refs；
- rotation；
- editor provenance；
- privacy metadata。

## 11.8 End-to-end

Browser/Tauri 都覆盖：

1. 新建班级；
2. 导入 Excel；
3. 自动映射/手动映射；
4. 创建教室；
5. 添加 hard/soft rules；
6. feasibility precheck；
7. 生成多个 candidates；
8. 交换/拖动/锁定/undo；
9. 保存；
10. 重开；
11. 历史/轮换；
12. 导出 public/teacher；
13. 退出。

## 11.9 长时间/重复运行

至少做：

- 连续 500 次 solve；
- 连续 100 次项目打开/保存；
- 连续 1000 次编辑命令；
- 连续导出所有格式；
- 长时间 Tauri 开关班级；
- 监测内存是否单调泄漏；
- 取消正在运行的 solve 后再次 solve。

---

# 12. 安全与数据可靠性

## 12.1 Loopback API

完成原 roadmap 未完成项：

- 短生命周期 session token；
- Origin/Host 检查；
- 只绑定 loopback；
- CSP；
- 请求 body 大小限制；
- endpoint rate/parallelism 保护；
- 不允许任意文件路径从网页直接传入并读取。

## 12.2 文件安全

- canonical path；
- zip traversal；
- symlink；
- archive expansion ratio；
- 文件数上限；
- 单文件上限；
- 临时目录权限；
- cleanup；
- save-as extension normalization。

## 12.3 Privacy

public export 的敏感信息名单作为中心 policy，而不是各 exporter 自己判断。

每次 release 自动扫描：

- 分数；
- 备注；
- special needs；
- 身高；
- 视力；
- 未匿名 ID；
- 本机路径；
- 调试日志；
- 环境变量。

## 12.4 Supply chain

- Cargo.lock 提交并锁定；
- `cargo audit` / `cargo deny`；
- npm audit 用于 build dependencies；
- GitHub Actions 固定 major 或 commit；
- 发布 artifacts 生成 SHA-256；
- SBOM；
- provenance/attestation；
- 禁止 release 构建读取开发机未跟踪文件。

---

# 13. 桌面与发布工程

## 13.1 三平台

正式支持：

- macOS ≥ 13；
- Windows ≥ 10；
- Ubuntu ≥ 22.04（或明确支持的主流发行形式）。

## 13.2 Clean-machine acceptance

必须在**没有 Python、Node、源码 checkout** 的干净环境验证：

- 安装；
- 首次启动；
- 离线启动；
- 打开 v1 项目；
- solve；
- export；
- 退出；
- 卸载；
- 无残留后台进程。

## 13.3 签名

v2.0 final 不发布“正式但未签名”的桌面包：

- macOS Developer ID + notarization；
- Windows code signing；
- Linux checksums/package metadata；
- GitHub Release 附所有 SHA256SUMS。

## 13.4 体积/启动性能目标

沿用 `rust-migration.md` 的 5–20 MB 原生目标作为工程指导，但以最终签名 bundle 实测为准。

记录：

- CLI 大小；
- backend binary；
- Tauri bundle；
- cold start；
- first window；
- first solve；
- idle memory；
- solve peak memory。

任何明显回归都必须在 release notes 前解释，而不是静默接受。

---

# 14. CI/CD 重构

最终 CI 建议分为：

### PR-fast

- fmt/clippy/unit/typecheck/Vitest；
- <10 分钟为目标。

### Core-full

- property；
- parity/golden；
- solver corpus；
- migrations；
- exports。

### Cross-platform

- Rust tests Windows/macOS/Linux；
- Tauri build smoke；
- frontend E2E。

### Security

- audit/deny；
- secret scan；
- SBOM；
- archive hygiene。

### Release-candidate

- signed bundles；
- clean-machine automation；
- checksum；
- E2E；
- benchmark；
- provenance。

v2 删除 PyPI workflow；v1 maintenance 分支保留旧发布流程。

---

# 15. 文档迁移

v2 前必须重写而不是只修改版本号：

- README / README.en；
- Quickstart；
- Architecture；
- Rules；
- Projects；
- Migration from v1；
- CLI；
- Desktop；
- Privacy；
- Troubleshooting；
- Security；
- Release checklist；
- Versioning；
- Rust solver methodology；
- Reproducibility；
- Known limitations。

所有 Python/Streamlit/OR-Tools 文档从主流程删除，仅在 `v1 legacy` 页面保留历史说明。

---

# 16. Bug 分级与发布阻断规则

## P0 — 绝对阻断

- 数据丢失/损坏；
- 隐私泄漏；
- hard constraints 被违反但显示成功；
- 错误的 `ProvenInfeasible`；
- 远程代码执行/任意文件读取等严重安全问题；
- 安装后无法启动。

## P1 — 阻断

- 主流程无法完成；
- migration 大范围失败；
- 常见班级无法 solve；
- export 主格式不可用；
- undo/redo 损坏状态；
- 跨平台重大功能缺失。

## P2 — RC 前必须清零的核心问题

- 明显错误的诊断；
- 可稳定复现的严重 UI 状态错误；
- 较大性能回归；
- 某正式规则/目标语义错误；
- 非核心但常用导出错误。

## P3

- 轻微视觉/文案问题；
- 低概率、不影响数据正确性的边缘体验。

Final 允许存在少量已知并明确记录的 P3，但不允许用“known issue”包装核心缺陷。

---

# 17. v2.0.0 Final Gate

只有以下**全部满足**才能打 `v2.0.0`：

## 17.1 代码

- [ ] Python 生产代码和依赖已从 v2 主线移除；
- [ ] 单一 Rust workspace；
- [ ] release checkout 干净；
- [ ] fmt/clippy 全绿；
- [ ] 无未经解释的 unsafe；
- [ ] capability/error/schema 冻结。

## 17.2 行为与数据

- [ ] parity ledger 全部 v2 必须项 `RUST_VERIFIED`；
- [ ] v1 历史 fixture 全部迁移通过；
- [ ] migration 均可备份/回滚；
- [ ] public export privacy scan = 0 泄漏；
- [ ] hard validator corpus 100% 通过；
- [ ] 0 false-proven-infeasible。

## 17.3 Solver

- [ ] official 20/40/50/60/80 corpus 全部合法；
- [ ] fixed-assignment scoring 与冻结 oracle 一致；
- [ ] quality gates 达标；
- [ ] deterministic seed tests 达标；
- [ ] timeout/cancel/unknown semantics 正确；
- [ ] candidate diversity 达标。

## 17.4 UI/E2E

- [ ] 浏览器完整主流程；
- [ ] Tauri 完整主流程；
- [ ] drag/multi-select/undo/redo；
- [ ] rule builder；
- [ ] feasibility diagnostics；
- [ ] import rollback；
- [ ] keyboard accessibility；
- [ ] dark/light/theme。

## 17.5 平台

- [ ] macOS clean install；
- [ ] Windows clean install；
- [ ] Linux clean install；
- [ ] offline；
- [ ] upgrade/open v1 project；
- [ ] uninstall；
- [ ] no residual process。

## 17.6 Security/release

- [ ] dependency audit；
- [ ] fuzz 无 release blocker；
- [ ] SBOM；
- [ ] checksums；
- [ ] signed/notarized；
- [ ] release artifacts 从 clean checkout 构建；
- [ ] release notes/upgrade guide 完成。

## 17.7 Defect gate

- [ ] 0 P0；
- [ ] 0 P1；
- [ ] core/data/security 0 P2；
- [ ] 最后 RC soak 期间无新增 blocker；
- [ ] 所有 RC 后修复均有 regression test。

---

# 18. 推荐的实际开发顺序

不要按“哪个功能看起来有趣”推进，按下面的依赖链：

```text
1. Freeze contracts + parity ledger
        ↓
2. Rust workspace / typed domain / errors
        ↓
3. Project + import + editing + export parity
        ↓
4. Hard feasibility engine
        ↓
5. Solver v2 soft optimization
        ↓
6. Rule metadata + explanation
        ↓
7. Canvas / rule builder / teacher UX
        ↓
8. Rust-only alpha
        ↓
9. Python removal
        ↓
10. Beta stabilization
        ↓
11. Signed RC
        ↓
12. v2.0.0
```

原因：如果过早把精力投入新画布、插件或更多规则，而底层契约、数据迁移和 solver 状态仍未冻结，后续会反复返工。

---

# 19. 从四个对比项目中明确吸收什么

## open_fuckseats

吸收：

- 教师长期班级工作流；
- 标签成为一等规则对象；
- 拼音/自然排序；
- 可读冲突提示；
- 新手示例班级；
- 桌面文件/升级细节。

不吸收：

- 巨型单体 view/JS；
- 贪心排座替代 solver；
- 任意 Python 插件执行。

## SeatingChartEditor2

吸收：

- metadata-driven rule DSL；
- bipartite matching feasibility precheck；
- violator-driven local search 邻域；
- pan/zoom/box-select/multi-drag canvas；
- transaction-style import rollback。

不吸收：

- 在 React 中维护求解真相；
- `Math.random()` 不可复现 solver；
- 前端巨型 composable。

## Seatflow

吸收：

- 清楚的领域/application/infrastructure 边界；
- manifest/config schema 思维；
- archive 分块 hash；
- 模糊导入；
- 未来 capability-based extension 设计。

v2.0 暂不开放动态插件执行。

## RandomSeatGenerator-JE

吸收：

- seed 作为显式产品能力；
- GUI/CLI 共用一个 core；
- 简洁可复现工作流；
- 配置 alias/兼容读入；
- “先排序/分带，再随机或优化”的预设思路。

---

# 20. v2.0 之后的版本政策：先维护“SeatTrellis 2”，不要现在规划未来产品

当前只冻结**版本原则**，不冻结 v2.1、v2.2 或 v3 的具体功能表。v2.0 发布之后先观察真实用户、bug、性能数据和长期维护成本，再决定后续方向。

## 20.1 `2.0.x`：极度保守的稳定维护

`2.0.1 / 2.0.2 / ...` 原则上只接受：

- bug fix；
- security fix；
- 数据/迁移兼容修复；
- 性能回归修复；
- 极低风险 UX 修复；
- 文档、翻译和发布工程修复。

不在 `2.0.x` 中扩大产品边界，不引入会改变 threat model、数据模型或部署模型的新系统。

## 20.2 后续 `2.x` minor：在 v2 架构契约内渐进完善

未来 `2.1.0 / 2.2.0 / ...` **可以有新功能**，但是否做、做什么现在不承诺。合理的 v2 minor 应满足：

- Rust-only、本地优先的基本产品模型不变；
- 不要求用户迁移到账号/云端才能使用；
- 不破坏 v2 schema/API/editing contract 的兼容承诺；
- 可以增加规则、导入器、导出器、布局模板、诊断、性能优化、可访问性和教师工作流；
- 每项功能仍经过 Product Decision / ADR，而不是因为“roadmap 写过”就必须做。

因此，v2.x 的定位是：**持续打磨 SeatTrellis 2，而不是不断重定义 SeatTrellis。**

## 20.3 何时才应考虑 v3

只有当未来计划改变产品的基本边界时，才有理由进入新的 major。例如若未来真的决定组合引入：

- 第三方插件平台；
- 云同步与多设备；
- 账户/身份/权限；
- 多人协作；
- 远程 AI 或云端求解；
- 组织级管理后台；

这会同时改变扩展模型、数据一致性、隐私、认证、安全、服务器运营和兼容责任，适合作为**未来 major 级重新设计的候选方向**。

但现在不承诺这些功能一定属于 `v3.0.0`，甚至不承诺一定开发。只在 `Post-v2 Exploratory` 文档中记录为研究方向，等 v2 有足够真实使用证据后再做产品研究和 architecture review。

## 20.4 Major 版本判定原则

不要因为“功能多”就升 major。只有出现以下情况之一才考虑新的 major：

- 核心领域/项目 schema 需要不可兼容重构；
- 本地优先/隐私边界发生根本变化；
- 扩展或云平台成为产品的一等组成部分；
- 用户心智模型和主工作流需要整体重构；
- 为了继续演进必须打破 v2 已冻结的重要兼容契约。

否则优先在 2.x 内渐进完善。

---

# 21. 最终原则

SeatTrellis v2.0.0 的目标不是“把 Python 删除掉”这么简单，而是完成一次**责任边界重建**：

1. Rust 成为唯一业务真相；
2. solver 不再混淆“没找到”和“证明无解”；
3. 数据写入具备事务与恢复能力；
4. 规则既能表达复杂需求，又能被教师理解；
5. UI 是 Rust domain 的视图，而不是第二套实现；
6. v1 历史数据可安全进入 v2；
7. 发布物不需要 Python/Node/源码环境；
8. final 之前经历 alpha → beta → RC 的真实冻结与修复周期。

如果上述门槛没有全部达到，应继续发布 `v1.9.x`、`2.0.0-beta.N` 或 `2.0.0-rc.N`，甚至退回前一阶段重新设计，而不是为了版本号强行发布 final。v2.0.0 一旦发布，就应代表一个可以放心长期使用、长期维护、长期兼容的 SeatTrellis 2 基线。
