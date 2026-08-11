# SeatTrellis 与四个同类开源项目的代码级对比分析

> 分析对象：SeatTrellis、open_fuckseats、SeatingChartEditor2、Seatflow、RandomSeatGenerator-JE  
> 分析方式：基于用户上传的完整源码压缩包，解压后按固定源码快照进行全仓库静态清点，并沿核心业务路径逐层追踪数据模型、规则系统、算法、编辑器、导入导出、扩展机制、桌面封装、安全与工程组织。本文不以 README 功能表或少量代码片段代替源码分析。

---

## 一、源码范围与版本基准

本次分析使用的代码快照如下：

| 项目 | 代码快照 |
|---|---|
| SeatTrellis | `282fd99a7e766aaedaea6c5bb4c61e3ef14d257c` 对应的 `seattrellis-main` 源码快照 |
| open_fuckseats | `625fc9a074545977516e96c1898bcb7c041c9bd3` |
| SeatingChartEditor2 | `a63b80e8caad716990a67b7e7ab525a4591d3e1e` |
| Seatflow | `5ad6366c53e000028472f5a3701d3f18d04823b1` |
| RandomSeatGenerator-JE | `0e572cebdd40561439b4b58695f68150996a3b12` |

完整文件路径和 SHA-256 已另行生成清单。代码统计包含源码、配置与文档；二进制字体、图片等资产被纳入文件清单，但不作为业务逻辑分析对象。

### 1.1 规模与结构指标

| 项目 | 主要语言 | 主要源码规模（非空代码行） | 结构特征 |
|---|---:|---:|---|
| SeatTrellis | Python、Rust、React/TypeScript | Python 45,356；Rust 22,802；TS/TSX 12,875；CSS 4,475 | 多运行时、强契约、求解器中心 |
| open_fuckseats | Python/Django、JavaScript、HTML/CSS | 约 56,998 | 功能面极广，但核心逻辑高度集中 |
| SeatingChartEditor2 | Vue、JavaScript/TypeScript、PHP | 约 60,607（不计文档/锁文件） | 前端工作台强，交互与规则表达丰富 |
| Seatflow | C#、Avalonia AXAML | 约 42,776（不计文档） | 分层架构、策略与插件体系最完整 |
| RandomSeatGenerator-JE | Java/JavaFX | 约 3,246 | 极简专用核心、可复现、易发布 |

一个很有说明力的指标是“最大五个源码文件占比”：open_fuckseats 约为 54%，说明大量业务集中在 `seats/views.py`、`classroom.js` 等巨型文件；SeatTrellis、Seatflow、SeatingChartEditor2 均约为 12%—16%，模块化程度明显更高。

---

## 二、总判断：五个项目并不是同一种产品

它们都处理“人—座位—规则”的关系，但核心定位不同：

- **SeatTrellis** 是“约束求解与长期公平性平台”。真正的壁垒在可复现求解、硬约束正确性、历史公平、候选解释、版本化数据与隐私边界。
- **open_fuckseats** 是“中国教师日常座位管理系统”。真正的优势不是算法，而是班级、分组、标签、排序、历史、批量操作、导出、桌面与云同步组成的完整工作流。
- **SeatingChartEditor2** 是“高交互座位工作台”。真正的优势是可视化编辑、规则 DSL、规则预检、触控与区域操作，以及将复杂规则变成教师能理解的界面。
- **Seatflow** 是“可扩展的通用排座框架”。真正的优势是领域分层、策略 SPI、插件包、能力权限、可选择导入导出和完整归档协议。
- **RandomSeatGenerator-JE** 是“把一个窄问题做到简单可靠的工具”。真正的优势是纯净核心、种子复现、GUI/CLI 共用、向后兼容配置和单文件发布。

因此，SeatTrellis 不应把目标设为“把其他四个项目的功能全部搬进来”。最合理的方向是：

> **保留 SeatTrellis 的正式求解内核与契约优势，吸收 SeatingChartEditor2 的交互和规则表达、open_fuckseats 的教师工作流、Seatflow 的受控扩展机制、RandomSeatGenerator-JE 的极简复现体验。**

---

# 三、SeatTrellis 当前代码的核心优势与结构性问题

## 3.1 SeatTrellis 已经领先的部分

### 3.1.1 数据模型不是“二维名单”，而是可扩展的领域模型

`src/seattrellis/models/student.py`、`layout.py`、`rules.py`、`history.py`、`snapshot.py`、`rotation.py` 形成了相对完整的领域层。

学生模型除姓名、编号外，还可表达：

- 性别、身高、成绩、视力；
- 标签和特殊需求；
- 任意属性字段；
- 用于排序、分布和平衡的扩展数据。

座位模型不是简单的 `(row, column)`，而是带稳定 `seat_id` 的节点，可包含：

- 行列与自由坐标；
- 是否启用；
- 区域、组别；
- 靠窗、靠门、靠讲台、靠空调等环境属性；
- 自定义标签和属性；
- 显式自定义邻接边。

这使 SeatTrellis 能将教室理解为“带属性的座位图”，而不只是矩形数组。相比之下，RandomSeatGenerator-JE 仍是固定矩形；open_fuckseats 虽有过道、讲台和空位，但求解逻辑主要仍依赖行列；SeatingChartEditor2 的区域与座位表达很强，但核心计算长期停留在前端状态；Seatflow 支持多种布局构建器，但教育场景规则深度不如 SeatTrellis。

### 3.1.2 硬约束与软目标真正分离

SeatTrellis 的规则模型明确区分：

- 固定座位；
- 必须相邻、不得相邻；
- 最小距离；
- 小组聚合或分离；
- 公平轮换、最近同桌、前后排公平；
- 成绩位置、成绩分布、导师配对；
- 身高、视力、多样性与稳定性。

其价值在于：硬约束决定可行性，软规则进入代价函数，二者不会混为一谈。这比 open_fuckseats 的“排序后贪心填座”更严格，也比 Seatflow 的“按策略优先级逐步填入”更适合处理相互耦合的规则。

### 3.1.3 求解后仍进行独立校验

SeatTrellis 不是把求解器返回的结果直接视为正确。`solver/assignment_validator.py`、候选生成和服务层会再次核验硬约束。这是非常重要的工程防线：

- 防止不同后端语义漂移；
- 防止本地搜索或回退算法在边界条件下产生非法解；
- 防止数据转换层把学生、座位索引映射错误；
- 允许 Rust、Python、OR-Tools 多后端在统一契约下被验证。

这一点是 SeatTrellis 相比其他项目最重要的“可信性优势”之一。

### 3.1.4 可复现性已经嵌入算法而非停留在宣传

`fallback_backend.py`、Rust core 和候选生成逻辑都使用明确 seed。候选的元数据会保留求解后端、种子、历史范围等信息。RandomSeatGenerator-JE 也做到了 seed 复现，但规则复杂度远低于 SeatTrellis；SeatingChartEditor2 的模拟退火主要使用 `Math.random()`，open_fuckseats 的随机策略也没有形成完整的结果溯源协议。

### 3.1.5 历史不是附件，而是目标函数的一部分

`history.py` 不只保存过去的座位表，还把历史转化为：

- 每个学生坐过的座位类别计数；
- 每对学生过去的邻接关系记录；
- 前排、后排、边角、同桌、近邻等可计算统计；
- 轮换与近期重复惩罚的输入。

这使“公平”从一句产品文案变成可定义、可计算、可解释的成本。其他项目中，Seatflow 有同桌历史策略，open_fuckseats 有历史记录与快照，但 SeatTrellis 的历史建模最系统。

### 3.1.6 编辑协议具有真正的事务语义

`editing_protocol.py` 与 `editing.py` 使用：

- `command_id`；
- `draft_id`；
- `base_revision`；
- apply / undo / redo；
- 批量操作上限；
- 学生和座位唯一性校验；
- 原子快照与冲突检测。

这比单纯在浏览器里保存一个 undo 栈更稳健。后续增加拖拽、多选、区域移动时，应继续让所有交互转换为该协议的命令，而不是让 React 直接改最终状态。

### 3.1.7 项目包、迁移、隐私和导出体系已形成平台能力

SeatTrellis 已具备：

- 多种导入映射；
- JSON schema 与迁移；
- 项目打包/恢复；
- 路径穿越防护；
- 敏感字段检查；
- SVG、PNG、PDF、Excel、Word、PPTX、HTML 等多格式导出；
- Python 与 Rust/React 运行时共用核心契约。

因此，SeatTrellis 的下一阶段不应继续无边界增加导出格式，而应重点提升“教师能否轻松完成整个工作流”。

## 3.2 SeatTrellis 当前最明显的短板

### 3.2.1 规则内核强，但规则表达界面仍偏工程化

SeatTrellis 已有 `RuleSetEditorPanel.tsx`，但当前规则仍主要围绕固定座位、学生对约束、距离、组规则和若干预设软目标。它缺少 SeatingChartEditor2 那种统一、元数据驱动的表达：

- “某标签学生只能在某区域”；
- “某标签不得进入某区域”；
- “某数值属性按前后形成梯度”；
- “按数值分层后分散到各大组”；
- “相同标签尽量分散”；
- “同组/异组、相邻排、遮挡关系”等通用谓词。

SeatTrellis 的学生和座位模型其实已经有 `tags`、`attributes`、`zone` 等字段，问题不在数据承载，而在缺少统一规则编译层。

### 3.2.2 可视化编辑器明显落后于 SeatingChartEditor2

当前 `SeatingCanvas.tsx` 主要是 SVG 点击和键盘激活；`LayoutEditorPanel.tsx` 也偏单元格编辑。缺少：

- 平移、连续缩放、双指缩放；
- 框选和套索多选；
- 拖动单人、成对学生或整块座位；
- 区域刷涂；
- 规则影响范围与违规点叠加；
- 移动前预览与非法落点反馈；
- 移动端抽屉和触控手势。

这不是装饰性差距，而是会直接影响教师是否愿意采用该工具。

### 3.2.3 缺少求解前的“可解释可行性预检”

SeatTrellis 有严格校验和求解失败返回，但在用户点击生成前，还缺少一份教师易读的预检报告。SeatingChartEditor2 的二分图匹配预检能发现：每个学生单独都有候选座位，但整体无法一一匹配的全局冲突。这类问题若只交给 CP-SAT 返回 infeasible，用户仍不知道应该改哪条规则。

### 3.2.4 多运行时带来语义漂移成本

当前同时存在：

- Python 模型、服务与求解后端；
- Rust core；
- Rust desktop/app server；
- React 前端；
- Python Web/桌面兼容路径。

项目通过差分脚本、后端校验和版本化 DTO 缓解了风险，但 `app/src/server.rs` 已达到约 4,839 行，`src/seattrellis/api/handlers.py` 也超过 2,200 行。随着功能增加，这两个适配层会成为新的单体热点。

### 3.2.5 对外扩展仍停留在“内部后端注册”

SeatTrellis 有 solver backend registry、导出器和可选依赖，但还没有面向第三方的：

- 插件包格式；
- 清单和版本兼容规则；
- 配置 schema；
- 能力权限；
- 生命周期；
- 安全隔离边界。

这一点可以借鉴 Seatflow，但必须避免直接开放任意 Python/JavaScript 执行。

---

# 四、open_fuckseats：最值得学习的是教师工作流，而不是求解算法

## 4.1 核心优势一：它真正覆盖了教师从建班到导出的全过程

open_fuckseats 的数据模型和页面不是围绕“一次随机排座”设计，而是围绕长期班级管理设计。`seats/models.py` 中可以看到：

- ClassroomGroup、Classroom；
- Student、Seat、SeatGroup；
- StudentTag、TagMembership、TagRule；
- SortStrategy；
- SeatConstraint；
- LayoutSnapshot；
- ClassroomHistoryEntry；
- OnboardingState；
- 云同步与会话元数据。

这意味着它将“排座”放在班级、分组、标签、历史和配置的上下文里。SeatTrellis 虽已有 Project/Workspace/History，但产品界面仍更像“先进排座工具”，而不是“教师的班级座位工作台”。

### SeatTrellis 可借鉴

1. **班级组与共享资源**：多个班级共享座位模板、导出模板和部分规则预设。
2. **批量操作**：批量建班、批量导入、批量生成、批量导出。
3. **班级长期视图**：进入班级后直接看到上次座位、历史版本、当前违规、待处理学生和常用操作。
4. **命名快照**：不仅保存“历史结果”，还允许用户给某次布局命名、备注和恢复。

## 4.2 核心优势二：标签成为一等公民

`StudentTag`、`TagMembership` 与 `TagRule` 不是展示性分类。源码中定义了：

- `must_area`：该标签学生只能坐指定区域；
- `forbid_area`：该标签学生不得坐指定区域；
- `separate_same_tag`：同标签学生需要保持距离。

标签还能用于学生搜索、批量筛选、排序和约束。这种设计非常符合真实学校场景：视力问题、纪律关注、班干部、住宿生、学习互助、需要靠门、需要远离空调等都不应硬编码为独立字段。

### SeatTrellis 可借鉴

SeatTrellis 已有学生 `tags`、座位 `zone/tags/attributes`，应新增“标签规则编译器”，使教师可以表达：

- 标签 → 允许/禁止座位集合；
- 标签 → 最小距离或分散；
- 标签 A 与标签 B → 鼓励/禁止相邻；
- 标签 → 某数值目标或区域配额。

实现上不应照搬 Django 模型，而应把这类用户规则编译为现有的 `CompiledProblem` 和软目标。

## 4.3 核心优势三：中国名单排序细节非常成熟

`seats/sorting.py` 支持：

- 中文拼音；
- 拼音首字母；
- 自然数排序；
- 数值、文本、空值处理；
- 多字段、多级升降序；
- 最多八层规则组合；
- 自定义字段和排序定义。

这类细节看似与“智能排座”无关，但教师导入名单后最常做的第一件事就是按姓名、学号、成绩或自定义字段整理。SeatTrellis 当前导入能力较强，但中国学校名单的“拼音＋数字混排＋空值＋多关键字”体验仍可明显加强。

### SeatTrellis 可借鉴

- 在名单表格中增加拼音全拼、首字母和自然排序；
- 保存用户常用多字段排序方案；
- 允许排序方案作为初始解或分组前处理，但不能替代正式求解器；
- 将中文姓名搜索同时匹配汉字、全拼和首字母。

## 4.4 核心优势四：约束诊断直接面向人

`seats/constraints.py` 不只把约束存入数据库，还负责：

- 规范化 payload；
- 检查重复和互斥；
- 编译固定座位、行列、成对关系和标签规则映射；
- 计算当前座位表中的违规；
- 生成可直接显示给教师的错误说明。

open_fuckseats 的求解并不先进，但其“规则录入时就告诉用户哪里冲突”的产品思路很值得学习。

### SeatTrellis 可借鉴

将现有严格模型校验扩展为三层诊断：

1. **语法/引用错误**：不存在的学生、座位、区域；
2. **显式逻辑冲突**：同一人既固定 A 又固定 B、同一对既必须相邻又不得相邻；
3. **全局可行性风险**：受限学生候选座位不足、匹配失败、区域容量不足。

## 4.5 核心优势五：插件不仅能运行代码，还能声明 UI

`plugin_system.py` 和 `plugin_components.py` 提供 Hook、Action、UI Script、Workspace Script，并定义 metric、text、list、table、progress、badge、section 等组件描述。这使插件可以向界面贡献受控的展示，而不是直接修改主界面的 HTML。

这比“插件返回任意模板”更可维护，也比只有 Python hook 更接近产品级扩展。

### SeatTrellis 可借鉴

先做**声明式扩展**，例如：

- 新导入器；
- 新导出器；
- 新目标预设；
- 新报告面板；
- 新数据清洗器。

插件只能返回 SeatTrellis 规定的 UI schema 和数据，不应直接注入 DOM 或任意脚本。

## 4.6 核心优势六：桌面端细节做得务实

open_fuckseats 在桌面封装上实现了：

- 本地数据库加密与密钥环；
- 明文数据库迁移、备份和中断恢复；
- 本地文件选择/保存桥；
- 扩展名和路径校验；
- 下载哈希验证；
- macOS 签名/Team ID 检查；
- 本地 URL 限制；
- 云端混合加密。

SeatTrellis 的项目包和隐私边界更系统，但 open_fuckseats 对“普通用户安装、升级、保存文件时会遇到什么”的处理更贴近桌面软件。

## 4.7 最不应照搬的部分

- `seats/views.py` 约 15,000 行，模型、业务、HTTP、模板上下文和算法混在一起；
- `classroom.js` 约 4,800 行，交互状态高度耦合；
- 排座主逻辑本质上多为“排序＋固定规则＋贪心填座”，不能替代 SeatTrellis 的正式求解；
- 自定义排序和插件允许受限 Python 执行，仍然属于可信本地代码而非真正安全沙箱；
- 依赖约束与架构边界不如 SeatTrellis 严格。

**结论：学习 open_fuckseats 的产品流程和中国学校细节，不学习它的单体结构与贪心求解。**

---

# 五、SeatingChartEditor2：最值得学习的是规则 DSL、求解预检和可视化工作台

## 5.1 核心优势一：规则是一套元数据驱动的语言

`src/constants/ruleTypes.js` 中的 `PREDICATE_META` 不只是枚举，而是为每个谓词描述：

- 关系类别；
- 最少主体数；
- 参数类型；
- 默认值；
- 可选项；
- 文本说明。

谓词覆盖：

### 位置规则

- 行范围；
- 大组范围；
- 在区域内/不在区域内；
- 特定列类型。

### 两人关系

- 必须/不得同桌；
- 最大/最小距离；
- 不遮挡；
- 同组/异组；
- 相邻排。

### 分布和数值规则

- 均匀分散；
- 聚集；
- 数值前后梯度；
- 各组数值平衡；
- 两人属性差；
- 数值分层分散。

优先级又分为 required、prefer、optional，并映射到不同惩罚量级。

### 为什么比 SeatTrellis 当前 UI 更先进

SeatTrellis 的底层模型严谨，但规则界面仍按具体规则类型逐项编写。SCE 的元数据允许一个 RuleBuilder 自动生成大量规则表单，同时统一验证、渲染、导入导出和描述。

### SeatTrellis 的正确吸收方式

不要把 SCE 的 JavaScript DSL直接移入前端求值；应建立：

```text
RuleAuthoringDocument（用户表达）
        ↓ 解析、展开标签/组/全部学生
RuleCompiler（唯一语义入口）
        ↓
现有 RuleSet / CompiledProblem
        ↓
Rust / Python / OR-Tools 后端
```

UI 元数据只负责生成表单和说明，规则真值必须由服务端/核心编译器决定。

## 5.2 核心优势二：求解前的二分图匹配预检非常有价值

`src/utils/assignmentPrecheck.ts` 会先为每个受限学生计算可用座位集合，然后：

1. 按候选座位数从少到多排序；
2. 使用增广路径算法尝试构造学生—座位一一匹配；
3. 统计最多能匹配的人数；
4. 输出候选最少、最“紧”的学生；
5. 报告整体不可行而不是只报告单个学生无座位。

它能发现以下典型问题：

- 甲可坐 A/B，乙也可坐 A/B，丙只能坐 A；每个人单独看都有座，但三个人整体只有两个座位；
- 某区域有 8 个座位，却有 10 名学生被 required 规则限制进去；
- 两套看似独立的位置规则在学生交集上造成容量冲突。

这是 SeatTrellis 当前最值得优先吸收的机制之一。

### SeatTrellis 可进一步做得更强

- 在 Rust/Python core 中生成 `FeasibilityReport`；
- 对固定座位和位置域运行匹配预检；
- 对 must-adjacent 组件计算可用座位块；
- 对 group-together 计算连通区域容量；
- 返回最紧学生、最紧区域、冲突规则 ID 和可操作修改建议；
- 把预检标为“可证明不可行”“高风险但未证明”“通过基础检查”三种状态。

## 5.3 核心优势三：模拟退火实现有大量实用工程技巧

`src/composables/useAssignment.js` 的主求解路径是浏览器端模拟退火，包含：

- 预计算规则上下文和数值代价；
- 初始解生成；
- 增量评分跟踪；
- 双人交换；
- 空座移动；
- 四人/同桌块移动；
- 优先选择违规学生或被规则影响的学生；
- 停滞检测；
- 抖动和全局 shake-up；
- 重新升温；
- 取消与进度回调；
- 定期让出事件循环，避免页面冻结。

SeatTrellis 不应把主求解器换成该算法，但其“邻域设计”和“交互式求解”值得用于 fallback/Rust heuristic 的增强：当硬约束已满足后，可用局部交换、同桌块移动和违规驱动选择进一步降低软成本。

### 必须注意的代码问题

`runSmartAssignment` 暴露 `algorithm = 'SA' // SA, LEGACY_GREEDY, EXHAUSTIVE`，但当前实现实际继续走模拟退火；“穷举＋剪枝回溯”章节只有占位。该参数并未形成真正的算法分派。随机过程也主要依赖 `Math.random()`，无法保证复现。

因此 SeatTrellis 只能借鉴邻域和进度机制，不能照搬它的算法选择与随机性设计。

## 5.4 核心优势四：可视化编辑器是真正的工作台

`SeatChart.vue` 和相关 composables 支持：

- 鼠标平移、滚轮缩放；
- 触摸拖动和双指缩放；
- 矩形/套索选择；
- 多选学生；
- 拖动预览；
- 整块移动；
- 放置区域和非法落点反馈；
- 多大组布局；
- guard seat；
- 移动端抽屉与手势。

这使“算法生成后人工微调”变成自然流程，而不是若干按钮。

### SeatTrellis 可借鉴，但必须保留现有协议

React 前端可以实现同等级交互，但最终动作应转换为：

- move；
- swap；
- batch move；
- lock/unlock；
- zone edit；
- layout cell edit；
- undo/redo。

由 `editing_protocol` 校验 `base_revision` 并原子应用。这样可同时获得 SCE 的交互体验和 SeatTrellis 的一致性保证。

## 5.5 核心优势五：Workspace 导入具有事务回滚

`useWorkspace.js`、`workspaceValidation.ts` 的做法值得学习：

- 文件先迁移，再验证；
- 限制学生、标签、组、座位、区域和规则数量；
- 校验唯一 ID 和引用完整性；
- 导入前保存完整运行态；
- 若应用任一步失败，恢复工作区、选择状态、拖拽状态、撤销栈和轮换状态。

SeatTrellis 的项目包和 schema migration 更正式，但“导入失败后整个 UI 状态无损回滚”可以进一步强化到 React workbench。

## 5.6 核心优势六：平台适配边界清楚

`src/platform/files.ts`、storage、runtime、API client、WebDAV transport 将浏览器与 Tauri 差异放在单独层中。业务组件只调用统一文件接口，而不直接判断环境。

SeatTrellis 当前 Python/Rust/React 多路径较复杂，更需要这一原则：所有前端平台差异应收敛在少量 adapter 中，不能扩散进 roster、layout、rules 和 editor 组件。

## 5.7 最不应照搬的部分

- 求解和大量业务逻辑集中在前端大型 composable；
- `Math.random()` 破坏可复现性；
- 算法下拉项与实际实现不一致；
- 若继续增长，`useAssignment.js`、`SeatChart.vue` 等会成为前端单体；
- PHP/WebDAV/多云路径扩大了安全与维护面。

**结论：学习 SCE 的规则表达、预检、画布和事务式工作区；不要把求解语义迁到前端。**

---

# 六、Seatflow：最值得学习的是扩展架构、策略清单和可验证归档

## 6.1 核心优势一：分层和依赖方向最清楚

Seatflow 将代码拆分为：

- `SeatFlow.Core`：领域实体与策略接口；
- `SeatFlow.Application`：应用服务、命令、管线、插件协调；
- `SeatFlow.Infrastructure`：文件、布局、迁移和归档；
- `SeatFlow.Contracts` / `SeatFlow.Plugins.Sdk`：外部契约；
- `SeatFlow.Presentation.Avalonia`：MVVM 界面。

这是五个项目中“插件和业务边界”最清楚的架构。SeatTrellis 的 Python/Rust 层也较清晰，但对外扩展契约还没有形成独立 SDK。

## 6.2 核心优势二：策略 SPI 简洁且可组合

`ISeatingStrategy` 公开：

- ID、名称、优先级、是否启用；
- 配置验证；
- 异步执行。

`IDependentSeatingStrategy` 可以拦截 RandomFill 的候选提案，返回：

- Approve；
- Reject；
- Handled。

主排座流程按优先级逐步执行：固定座位优先，前排轮换、同桌等中间策略随后，随机填充在后，最后整理碎片。这种模式非常适合“规则是局部、可按阶段填入”的场景。

### 对 SeatTrellis 的启示

SeatTrellis 不应让该策略管线取代全局求解，因为它不能天然处理复杂回溯。但它可以作为：

- 初始解构造器；
- 求解前预处理；
- 求解后局部修复器；
- 外部可插拔启发式；
- 只读分析/报告插件。

可以设计三个受控扩展点：

```text
ProblemTransformer  → 改写/补充问题，但不可直接落座
InitialSolutionProvider → 产生候选初始解
ResultAnalyzer → 只读生成报告与指标
```

所有结果仍必须经过 SeatTrellis 的硬约束验证和评分。

## 6.3 核心优势三：策略清单与代码分离

`Strategies/Manifests/*.json` 为策略定义展示名称、描述和配置 schema。UI 不需要硬编码每个策略的表单。

这与 SCE 的 `PREDICATE_META` 是相同方向：**让规则/策略具备机器可读元数据**。SeatTrellis 可以统一设计：

- 稳定规则 ID；
- 参数 schema；
- 默认值和范围；
- 用户说明；
- 是否属于硬约束；
- 所需学生/座位属性；
- 后端支持矩阵；
- 迁移版本。

## 6.4 核心优势四：插件包和能力权限比普通 hook 更成熟

Seatflow 插件体系包含：

- 插件包 manifest；
- 策略入口；
- DLL 的独立 `AssemblyLoadContext`；
- 初始化、停用、卸载；
- Lua/C# 脚本 adapter；
- 声明式配置 schema；
- capability 权限，例如固定座位能力；
- ZIP 安全验证和安装预览。

最值得学习的不是运行 DLL 或 Lua，而是“插件必须声明自己要做什么”。SeatTrellis 可建立能力列表：

- read_roster；
- read_layout；
- read_sensitive_attributes；
- propose_rules；
- provide_importer；
- provide_exporter；
- analyze_result；
- write_project_artifact。

默认插件不应获得敏感字段和文件系统权限。

### 不可照搬的部分

任意脚本或原生库插件难以真正隔离；源码中也存在脚本资源限制尚未完全落实的痕迹。SeatTrellis 第一阶段应优先支持声明式、进程外或 WASM 化扩展，而不是直接开放 Python `exec`。

## 6.5 核心优势五：布局多态性强

Seatflow 不是只支持 Grid，还提供 Polar、Freeform 等布局构建器，并把讲台、门、柱子等作为障碍物或场地对象。Polar builder 甚至会围绕径向过道分配弧段，并使用最大余数法分配座位数量。

### SeatTrellis 应谨慎学习

对普通中学教室，极坐标会场不是优先需求。真正值得吸收的是：

- 座位布局与场地对象分离；
- 障碍物有类型和几何；
- 邻接关系由布局构建器产生；
- 自由布局仍转化为统一图模型。

SeatTrellis 可以先增加门、窗、讲台、空调、柱子等一等场地对象及其几何影响，不必立刻扩展到剧院式布局。

## 6.6 核心优势六：SeatSets 归档具有完整性验证

Seatflow 的归档不是简单 ZIP：

- 按类别拆分 chunk；
- 支持选择性导入/导出；
- canonical JSON；
- 每个 chunk 有 SHA-256；
- 整体 archive 有 SHA-256；
- 导入前验证；
- 支持格式迁移。

SeatTrellis 现有 `project_bundle.py` 在路径安全、隐私扫描和恢复方面很强，可以进一步加入：

- 分块哈希；
- 总体哈希；
- 类别清单；
- 只恢复名单/布局/规则/历史/导出配置中的某几类；
- 差异预览。

## 6.7 核心优势七：导入器和命令模式成熟

`FuzzyColumnMatcher` 能从行式或列式 Excel 中寻找字段命中、容忍空行、合并列并构造学生，适合真实世界的脏表格。

`IUndoableCommand`、`AssignSeatCommand`、`RemoveStudentCommand`、`CommandHistory` 则将撤销定义为领域操作，而不是 UI 快照。

SeatTrellis 已有更强的版本化编辑命令；可借鉴 Seatflow 的命令类型分层和插件可发现性，但不必重写现有协议。

## 6.8 最不应照搬的部分

- 按优先级 Fill-in-Order 的策略体系不能替代全局约束求解；
- 通用会场、极坐标等能力可能偏离中学教室主场景；
- 原生插件和脚本插件扩大攻击面；
- Avalonia/.NET 10 带来额外打包和开发成本，不构成技术迁移理由。

**结论：学习 Seatflow 的接口、manifest、权限、归档和导入架构，不学习其算法作为唯一核心。**

---

# 七、RandomSeatGenerator-JE：最值得学习的是小而完整、种子可见和 GUI/CLI 同源

## 7.1 核心优势一：核心链路极其清楚

核心几乎可以概括为：

```text
SeatConfig → SeatGenerator → SeatTable
```

界面、命令行和 Excel 输出均围绕这一核心，而没有各自重写业务规则。对一个功能窄的工具，这是非常健康的结构。

SeatTrellis 功能更复杂，不可能同样小，但仍可借鉴“每条入口最终调用同一个 application service”的原则，减少 Python CLI、React、桌面服务的路径分叉。

## 7.2 核心优势二：种子是用户可见的产品功能

`SeatGenerator` 使用：

```java
new Random(seed == null ? 0L : seed.hashCode())
```

CLI 可传 `--seed`，结果和 Excel 中也会保存 seed。用户不仅能复现，还能把同一结果转交给他人。

SeatTrellis 底层已经更严谨地保存 seed，但界面应更直接：

- 结果页始终显示 seed；
- 一键复制“复现信息”；
- 导出页脚可选择写入 seed、规则版本、后端和历史范围；
- “使用相同 seed 重新生成”与“换一个 seed”并列。

## 7.3 核心优势三：保留粗略身高顺序的分块随机

名单被认为已按身高排序，然后按 `shuffledRowCount × columns` 划分块，每块内部随机。这样既保留大体的前后顺序，又避免完全固定。

这是一个非常符合直觉、易解释的预设。SeatTrellis 可以加入“分层随机”目标预设：

- 按某属性排序；
- 分成若干带；
- 带内随机；
- 再由正式求解器满足硬约束。

不应直接把分块洗牌作为最终算法，但可作为初始解或目标模板。

## 7.4 核心优势四：窄规则也做到了完整验证

生成器会检查：

- 行列和随机行数为正；
- 名单不超过容量；
- 最后一排禁用位置；
- 分离名单；
- 每列班干部数量是否足够；
- 最多 65,536 次尝试。

“每列必须有一名 leader”虽然专用，却体现了一个好原则：规则不一定要通用到抽象晦涩，只要用户场景明确，专用预设可以非常好用。

SeatTrellis 可把常见场景包装为高层 goal，而不要求普通教师从底层规则组合开始。

## 7.5 核心优势五：配置向后兼容成本很低

`SeatConfig.java` 使用 Gson `@SerializedName(alternate=...)` 同时接受新旧字段名。这是一种非常轻量的 schema 迁移方式。

SeatTrellis 已有正式 schema migration，更适合复杂项目；但对简单别名和字段改名，可以提供 alias 兼容层，避免每次都要求完整迁移。

## 7.6 核心优势六：发布与异常体验完整

项目包含：

- JavaFX GUI；
- `--nogui` 命令行；
- Excel 输出；
- ResourceBundle 国际化；
- 可翻译异常；
- 全局未捕获异常处理；
- Shadow fat JAR；
- Maven 发布；
- 依赖固定和许可证头检查。

它展示了“小工具也应有完整发布链路”。SeatTrellis 的发布体系更复杂，但可以进一步强化“下载即用”和无安装/便携包体验。

## 7.7 最不应照搬的部分

- 使用空格切分姓名和名单，无法稳健支持含空格姓名；
- `String.hashCode()` 可能碰撞，不适合作为严格跨实现随机协议；
- 依靠拒绝采样解决分离规则，复杂时会变慢，也不能证明不可行；
- 仅支持矩形网格，没有历史、公平、撤销和复杂规则。

**结论：学习它的产品简洁性、复现信息、GUI/CLI 共用和发布完整度，不学习其算法上限。**

---

# 八、跨项目能力矩阵

| 维度 | SeatTrellis | open_fuckseats | SeatingChartEditor2 | Seatflow | RandomSeatGenerator-JE |
|---|---|---|---|---|---|
| 全局约束求解严谨度 | **最强**：CP-SAT＋回退＋后验校验 | 较弱：排序/贪心为主 | 中强：模拟退火，但无严格可行性证明 | 中等：优先级策略管线 | 较弱：洗牌＋拒绝采样 |
| 可复现性 | **很强**：seed、后端、元数据 | 一般 | 较弱：`Math.random()` | 中等，取决于策略和 Random 注入 | **很强且用户可见** |
| 历史公平 | **最强** | 有历史/快照，算法结合较浅 | 有轮换/区域机制 | 有近期同桌策略 | 无 |
| 可视化编辑 | 中等，协议强但交互尚弱 | 较强 | **最强** | 较强桌面 MVVM | 基础 |
| 规则表达 | 强但偏具体/工程化 | 实用标签和约束 | **最灵活的用户 DSL** | 策略配置清楚 | 极窄 |
| 求解前诊断 | 基础严格校验 | 人类可读冲突较好 | **二分图预检最强** | 策略配置验证 | 基础参数校验 |
| 教师工作流 | 较完整但仍技术型 | **最贴近中国教师日常** | 较强工作台 | 偏通用框架 | 极简 |
| 中国名单适配 | 中等 | **拼音/自然排序最强** | 较好 | 通用表格适配 | 基础 |
| 导入容错 | 强映射与预览 | 实用 | 强 Excel 工作流 | **模糊列匹配突出** | 配置文件为主 |
| 导出 | **格式最广** | 配置/预览和桌面保存体验强 | 视觉预览强 | 接口清楚 | XLSX |
| 数据版本化 | **强 schema/migration** | Django migration/快照 | 强 workspace migration | **强归档迁移与哈希** | 简单别名 |
| 隐私与安全 | **最系统、最小化传输** | 本地加密和更新安全强，但攻击面大 | 文件/平台策略较好 | 插件包安全意识强 | 本地、结构简单 |
| 插件扩展 | 内部扩展强，对外 SPI 弱 | 实用 hook＋UI DSL | 无正式外部 SPI | **正式 SDK/manifest/capability 最强** | 无 |
| 架构模块化 | 强，但多运行时复杂 | 较弱，单体热点明显 | 中强，前端热点增长 | **最清晰** | 简单且清楚 |
| 产品简洁度 | 功能丰富、学习成本较高 | 功能密集 | 工作台密集 | 框架型 | **最简洁** |

---

# 九、SeatTrellis 应优先吸收的设计：按价值排序

## P0：直接影响正确性和核心体验

### 9.1 新增正式的可行性预检报告

来源：SeatingChartEditor2 的增广路径匹配＋open_fuckseats 的人类可读诊断。

建议新增领域对象：

```text
FeasibilityReport
- status: proven_infeasible / risky / passed_basic_checks
- capacity_issues
- invalid_references
- direct_rule_conflicts
- per_student_domains
- matching_summary
- tight_students
- tight_regions
- adjacency_component_issues
- suggested_actions
- involved_rule_ids
```

实现位置应在 solver/application 层，而不是 React。前端只显示报告并把学生、座位、区域和规则高亮。

### 9.2 建立元数据驱动的规则创作层

来源：SCE 的 `PREDICATE_META`＋Seatflow 的 strategy manifests＋open_fuckseats 的标签规则。

优先支持：

- subject：student / tag / group / all；
- position：row range / zone in-out / side-middle / environment attributes；
- relation：adjacent / desk-mate / distance / same-different group；
- distribution：spread / cluster / quota；
- numeric：gradient / balance / bands / pair difference。

必须保证所有规则都编译到统一 `CompiledProblem`，不能在不同 UI 或后端重复实现。

### 9.3 将画布升级为真正的直接操作编辑器

来源：SeatingChartEditor2。

优先顺序：

1. pan/zoom/pinch；
2. 框选、多选；
3. 拖动与交换；
4. 批量块移动；
5. 区域刷涂；
6. 锁定、规则和违规 overlay；
7. 移动前合法性预览。

所有动作转换为现有 versioned command，不直接改状态。

### 9.4 拆分两处新的单体热点

- `app/src/server.rs`：按 route/DTO/adapter/use-case 拆分；
- `src/seattrellis/api/handlers.py`：按 roster/layout/rules/generation/editor/project/export 拆分。

目标不是机械追求小文件，而是确保规则编译、历史转换、HTTP 解析和应用用例不混在一个模块。

### 9.5 建立逐规则解释和审计报告

来源：SCE 的规则文本与满意度报告、open_fuckseats 的违规说明。

每条规则应有：

- 自然语言句子；
- 约束类型和优先级；
- 是否满足；
- 相关学生/座位；
- 代价贡献；
- 不可评估原因；
- 修复建议。

这会把 SeatTrellis 的“可解释”从总分扩展到具体决策。

## P1：提升教师日常可用性

### 9.6 班级组、批量生成与批量导出

来源：open_fuckseats。

在现有 Workspace/Project 之上增加：

- 班级集合；
- 共享布局模板；
- 共享规则模板；
- 多班批量任务；
- 批量导出包；
- 结果状态和失败原因总览。

### 9.7 中国名单的人性化排序和搜索

来源：open_fuckseats。

- 拼音全拼；
- 拼音首字母；
- 自然数字；
- 多字段排序；
- 保存排序方案；
- 姓名搜索同时匹配汉字/拼音/首字母。

### 9.8 增加模糊表头与行列方向识别

来源：Seatflow 的 `FuzzyColumnMatcher`。

在 SeatTrellis 已有 mapping preview 之前，增加：

- 行式/列式自动识别；
- 常见中文表头同义词；
- 多候选映射置信度；
- 空白区段停止；
- 姓名列和学号列冲突提示。

### 9.9 使用真实临时项目完成新手引导

来源：open_fuckseats。

引导不应只是遮罩文字，而应创建可自动删除的示例班级，让用户亲手完成：

1. 导入名单；
2. 修改教室；
3. 添加一条规则；
4. 生成候选；
5. 交换座位；
6. 导出。

### 9.10 把复现信息变成显眼的 UI

来源：RandomSeatGenerator-JE。

SeatTrellis 已保存 seed，但应提供：

- 一键复制复现信息；
- 相同 seed 重跑；
- 更换 seed；
- 导出元数据页；
- 候选之间的 seed/backend/history 比较。

### 9.11 扩展项目包为可选择、可校验的归档

来源：Seatflow。

在现有安全项目包上增加：

- canonical chunk；
- 分块 SHA-256；
- archive SHA-256；
- 选择性恢复；
- 导入前差异预览；
- 恢复后完整性报告。

### 9.12 先建立声明式外部扩展，不开放任意代码

来源：Seatflow＋open_fuckseats。

第一阶段只允许：

- importer；
- exporter；
- goal preset；
- result analyzer；
- report panel。

每个扩展必须有 manifest、版本范围、配置 schema、能力声明和确定性说明。

## P2：在数据证明有价值后再做

### 9.13 增强本地搜索邻域

来源：SCE 的模拟退火。

可增加：

- 违规学生优先；
- 同桌块交换；
- 空位移动；
- 四人块移动；
- 局部 repair；
- 停滞检测和有限重启。

但必须通过 benchmark 证明收益，并保持 deterministic seed。

### 9.14 首先完善教室障碍物，不急于支持通用会场

来源：Seatflow。

优先把门、窗、讲台、空调、柱子建成一等对象，并将其影响映射为座位属性、距离或视线规则。极坐标和剧院布局只有在实际用户需求出现后再做。

### 9.15 独立的导出设置与实时预览

来源：open_fuckseats＋SCE。

SeatTrellis 已支持很多格式，下一步重点应是：

- 统一导出配置模型；
- 实时预览；
- 字体/方向/纸张/座位尺寸；
- 是否显示属性、组别、讲台、规则摘要和复现元数据；
- 保存导出预设。

---

# 十、哪些功能看起来先进，但不应照搬

## 10.1 不要复制 open_fuckseats 的单体业务文件

功能再多，也不能让 HTTP、数据库查询、排序、排座、插件和模板上下文继续堆进一个数万行文件。SeatTrellis 应保持领域层和应用层独立。

## 10.2 不要把 SCE 的求解器搬到浏览器作为权威实现

浏览器端 SA 可以提供快速预览，但不能成为 SeatTrellis 正式结果的唯一来源：不可复现、难以审计、无法严格证明硬约束、容易与后端规则语义漂移。

## 10.3 不要让 Seatflow 的优先级填座取代全局求解

策略管线适合初始解和插件扩展，但复杂规则需要全局协调。否则前面策略做出的局部决定可能让后面无解。

## 10.4 不要过早开放任意 Python/JavaScript/Lua 插件

本地软件中的“沙箱”常常只是受限内置函数，无法真正防止资源消耗、反射、文件系统或供应链风险。应先做声明式和能力最小化扩展。

## 10.5 不要把 RandomSeatGenerator-JE 的拒绝采样扩展为通用求解

它对少量分离规则足够简单，但规则密集后性能和可行性判断都会迅速恶化。

## 10.6 不要为了“布局通用”而失去学校场景焦点

Seatflow 的 Polar/Freeform 很漂亮，但 SeatTrellis 的核心用户是学校。每增加一种布局，都要承担编辑、规则、导出、测试和迁移的长期成本。

---

# 十一、建议形成的最终产品结构

综合五个项目，SeatTrellis 最合理的长期结构是：

```text
┌──────────────────────────────────────────────┐
│ 教师工作区                                    │
│ 班级组 / 名单 / 教室 / 规则 / 历史 / 批量任务 │
└──────────────────────────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│ 元数据驱动的规则创作层                        │
│ 学生/标签/组/全部 + 位置/关系/分布/数值谓词   │
└──────────────────────────────────────────────┘
                    ↓ 唯一规则编译
┌──────────────────────────────────────────────┐
│ Feasibility + CompiledProblem                 │
│ 预检、冲突证明、候选域、硬约束、软目标         │
└──────────────────────────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│ 多后端求解层                                  │
│ CP-SAT / Rust heuristic / Python fallback     │
│ 统一 seed、deadline、后验校验、评分和审计      │
└──────────────────────────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│ 候选比较与直接操作编辑器                      │
│ 画布、拖拽、多选、锁定、修复、undo/redo       │
└──────────────────────────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│ 项目包、历史、公平、导出与受控插件             │
└──────────────────────────────────────────────┘
```

这一结构的关键不是功能数量，而是保持三条原则：

1. **规则语义只有一个权威实现**；
2. **任何后端结果都必须经过统一验证和评分**；
3. **界面可以非常灵活，但状态修改必须经过版本化命令。**

---

# 十二、最终评价

## SeatTrellis 相比其他项目已经建立的真正壁垒

1. 最正式的硬约束/软目标模型；
2. 最强的可复现性与多后端校验；
3. 最系统的历史公平建模；
4. 最完整的候选比较与结果审计基础；
5. 最严谨的数据版本、隐私和项目包边界；
6. Python 与 Rust 迁移过程中的差分和契约意识。

## SeatTrellis 当前最影响产品成熟度的差距

1. 教师可理解的规则创作不足；
2. 缺少求解前全局可行性解释；
3. 可视化画布和人工微调体验不足；
4. 班级组、批量任务和中国名单细节不够完整；
5. 多运行时适配层正在变成新的复杂度中心；
6. 尚无安全、稳定、可声明能力的第三方扩展协议。

## 四个项目对 SeatTrellis 的一句话价值

- **open_fuckseats**：教你怎样让教师天天使用，而不只是生成一次座位表。
- **SeatingChartEditor2**：教你怎样把复杂规则和座位操作变成直觉化工作台。
- **Seatflow**：教你怎样开放扩展而不把核心代码与插件混在一起。
- **RandomSeatGenerator-JE**：教你怎样让“随机、复现、导出”简单到用户无需学习。

最终不应把 SeatTrellis 做成这四个项目的功能并集，而应把它打磨成：

> **一个具有正式求解可信度、又拥有教师级工作流和可视化编辑体验的本地优先排座平台。**

---

## 附录：重点源码路径

### SeatTrellis

- `src/seattrellis/models/`
- `src/seattrellis/solver/`
- `src/seattrellis/history.py`
- `src/seattrellis/candidates.py`
- `src/seattrellis/scoring.py`
- `src/seattrellis/editing.py`
- `src/seattrellis/editing_protocol.py`
- `src/seattrellis/project_bundle.py`
- `crates/seattrellis-core/`
- `clients/web/src/components/`
- `app/src/server.rs`

### open_fuckseats

- `seats/models.py`
- `seats/constraints.py`
- `seats/sorting.py`
- `seats/views.py`
- `seats/plugin_system.py`
- `seats/plugin_components.py`
- `static/js/classroom.js`
- `desktop_shell.py`
- `desktop_runtime.py`
- `database_security.py`

### SeatingChartEditor2

- `src/constants/ruleTypes.js`
- `src/composables/useSeatRules.js`
- `src/utils/assignmentPrecheck.ts`
- `src/composables/useAssignment.js`
- `src/components/seat/SeatChart.vue`
- `src/composables/useWorkspace.js`
- `src/utils/workspaceValidation.ts`
- `src/platform/`

### Seatflow

- `SeatFlow.Core/Strategies/`
- `SeatFlow.Application/`
- `SeatFlow.Plugins.Sdk/`
- `SeatFlow.Infrastructure/`
- `SeatFlow.Contracts/`
- `SeatFlow.Presentation.Avalonia/`
- `Strategies/Manifests/`

### RandomSeatGenerator-JE

- `src/main/java/.../core/SeatConfig.java`
- `src/main/java/.../core/SeatGenerator.java`
- `src/main/java/.../core/SeatTable.java`
- `src/main/java/.../AppLaunch.java`
- `src/main/java/.../util/SeatUtils.java`
- `build.gradle.kts`
