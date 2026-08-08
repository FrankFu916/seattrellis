# 竞品深度对标分析（2026-08-03）

对 4 个同类开源项目的穷尽式代码级分析，作为 SeatTrellis 打磨的参考资料。
源码克隆在 `/tmp/competitor-analysis/`（open_fuckseats / SeatingChartEditor2 / Seatflow / RandomSeatGenerator-JE）。

> 结论：SeatTrellis 在**规则系统、公平轮换、多格式导出、本地优先**上全面领先；缺口集中在
> **可视化交互编辑、规则可读性、数据安全落地、导入智能化、工程化发布链路**。

---

## 0. 项目画像

| 项目 | 技术栈 | 规模 | 一句话价值 |
|---|---|---|---|
| open_fuckseats | Python/Django + pywebview | ~6.4 万行 | 领域功能最全：8 算法/8 约束/标签规则/班级组/云 E2EE 同步 |
| SeatingChartEditor2 | Vue3 + Vite + Tauri | ~6.5 万行 | 可视化交互 + 工程化最强：拖拽/选区/规则中文渲染/预检/审计 |
| Seatflow | .NET 10 + Avalonia | ~4.8 万行 | 架构最干净：插件 SDK/Capability/依赖策略三态/自包含快照 |
| RandomSeatGenerator-JE | JavaFX | ~3.7 万行 | 极简定位：分块洗牌/seed 闭环/配置双命名兼容 |

---

## 1. 求解器 / 算法域

### 1.1 两段式贪心范式（open_fuckseats）
- 策略只决定两个键（学生排序序、座位填充序），全部收敛到 `_arrange_standard`（views.py:10553）。**先排学生顺序 + 先排座位顺序，再按顺序贪心填空** —— 与 SeatTrellis 的成本贪心不同：这里是「无评分」的硬约束贪心 + 重试兜底。
- 8 种 UI 白名单算法（views.py:10708-10748）：random/score_desc/score_asc/good_front/good_back/score_spread/group_balanced/group_mentor；另 4 个隐藏 legacy（standard/student_id/snake/field_sort）。
- `score_spread` 高低穿插（views.py:10719）：升序后交替 pop(max)/pop(0) 成帮扶式均衡。
- `snake` 蛇形填充（views.py:10731）：奇行正、偶行反，避免学生转头看前面。
- `good_back`：分数降序 + 座位序反转（views.py:10716）。
- `group_mentor` 导师制（views.py:10610-10635）：先 (最好,最差) 结对，再贪心分给累计分最小组合，真正均分均衡。
- `_arrange_standard` 三阶段（views.py:10553-10589）：固定座优先 → `_assign_pairs` 双人配对 → 剩余首可用合法位贪心。
- `_assign_pairs` 一次性锁双座（views.py:10500-10550）：MUST_TOGETHER 在 `dist×dist` 邻域同时为两人找座。
- `_attempt_auto_constraint_fix` 事务回滚重试（views.py:10752-10789）：方法序 `[preferred]+兜底8策略去重`，random/score_spread 各 16 次、其余各 5 次；事务内排座→stabilize→全过才算成功。
- `_enforce_constraints_by_moves` 布局后微调（views.py:7640-7762）：max_rounds=6，MUST/FORBID_TOGETHER **双向互补移动**（先移 A 再反向移 B）。
- `_simulate_move_valid` 双端仿真校验（views.py:7597-7621）：模拟交换后同时验两方，杜绝「修 A 破 B」。
- `_pick_best_target`（views.py:7624-7637）：曼哈顿距离 + (3 if 有占座) 打分，最小扰动 + 占用惩罚。
- `_stabilize_layout_with_rules`（views.py:7765-7772）：移动/交换后稳定流水线。
- 违反即回滚（views.py:11726/15280）：move/swap 后仍有 violations 整个事务回滚，布局永远合法。
- `_evaluate_layout` 质量评估（views.py:7783-7865）：未入座/违规/导出建议/小组均分均衡建议（max_avg−min_avg>5 时暴力搜索最优交换对，可一键 apply）。

### 1.2 模拟退火 + 可解释评分（SeatingChartEditor2）
- 权重分级：`required:100000 / prefer:1000 / optional:10`（ruleTypes.js:14-18）——硬约束靠 10 万倍权重「推」而非排除式限制（与 SeatTrellis 硬约束排除式互补）。
- 增量评分 `ScoreTracker`（useAssignment.js:1415-1480）：规则拆成 per-subject 评分单元 + `unitIndexesByStudentId` 反查，一次 move 只重算受牵连单元。
- 双重加热：5% SoftJitterReheat + 15% GlobalShakeupReheat（:1545-1613），停滞按 sinceBest 分档升温。
- 同分平台随机游走 `randomizePlateauSolution`（:1745-1783）：只接受不降分的交换制造同分多解，无种子靠 Fisher-Yates 无偏 + 统计测试。
- 邻域：`trySingleMove` Metropolis（:1991-2035）+ `tryPairMove` 4 人循环交换保持同桌绑定（:1936-1989）。
- 选学生偏置（:1859-1866）：优先违规名单→规则覆盖学生→全体。
- 初始解生成（:1255-1411）：REQUIRED 排/组→PREFER 梯度→REQUIRED 同桌→选区标签→随机。
- 几何冷却 + bestScore===0 提前退出。

### 1.3 Fill-in-Order 管道 + 依赖策略三态（Seatflow）
- `ISeatingStrategy` 按 Priority 降序执行、先到先得、无覆盖、IsFixed 天然保护（ISeatingStrategy.cs:8-24）。
- 内置优先级：FixedSeat(100)→FrontRowRotation(50)→DeskMate(50 依赖)→RandomFill(1)→Defrag(0)。
- `IDependentSeatingStrategy` 三态 Approve/Reject(重掷)/Handled(自完成)（:19-118），嵌入 RandomFill 随机分配循环内评估每个 (student,seat) 对。
- **约束学生优先**（RandomFillStrategy.cs:148-168）：`GetConstrainedStudentIds` 分组各自洗牌放最前，从源头减少无效重掷。
- Handled 后防死循环安全检查（:259-270）：声称已处理但未实际分配则补调 TryAssignSeat。
- 重掷上限 10 + 耗尽兜底强制分配 + 日志（:279-300）。
- FrontRowRotation 需求加分-历史惩罚+Fisher-Yates（FrontRowRotationStrategy.cs:82-97）：座位与学生各自洗牌，防止「分最高者总坐最左」。
- NoRepeatDeskMate 规范化 `(小ID,大ID)` 对查重（NoRepeatDeskMateStrategy.cs:182-183）。
- Defrag IsFixed 保护 + 前移聚拢 + **分配失败回滚旧座**（DefragStrategy.cs:134-194）。
- DeskMate 腾挪 `updateHistory:false`（DeskMateStrategy.cs:256）：中间腾挪不写历史，只有最终分配才记录。
- GenderRestrictedSeat 重定向优化（:125-153）：性别不匹配先查匹配性别的受限空座随机直分配（Handled，不耗重掷）。
- 每个策略 `ValidateConfiguration`（如拒绝负 HistoryWeight、同桌组≥2 人）。

### 1.4 分块洗牌 + 重试上限（RandomSeatGenerator-JE）
- 分块洗牌保持身高带（SeatGenerator.java:149-163）：名单按身高排序后只在相邻几排内洗牌不跨带。
- do-while 重试上限 65536 + **前置可行性校验**（:139-147）：先校验必要条件，再设上限制止死循环。
- seed 贯穿生成-展示-导出闭环（导出 xlsx 内嵌种子行）：任意结果可复现/可审计。

---

## 2. 规则 / 约束系统域

### 2.1 8 约束 + 3 标签规则 + 编译映射（open_fuckseats）
- `CONSTRAINT_TYPE_DEFINITIONS` 声明式注册表（constraints.py:14-87）：用 needs_row/needs_col/needs_target/needs_distance 声明字段依赖，是表单的唯一事实来源。
- 8 种学生级约束：MUST_SEAT/FORBID_SEAT/MUST_ROW/FORBID_ROW/MUST_COL/FORBID_COL/MUST_TOGETHER(曼哈顿≤dist)/FORBID_TOGETHER(>dist)。
- 3 种标签规则（constraints.py:108-131）：MUST_AREA(只能坐区域)/FORBID_AREA(禁坐区域)/SEPARATE_SAME_TAG(同标签两两保持距离，一次规则实现 N 方两两约束)。
- `normalize_constraint_payload` 写入前清洗（:294-364）：跨班校验、行列不超布局、MUST_SEAT 目标必须可用座、双人物理可行性预检、pair 归一。
- `validate_constraint_candidate`（:724-750）：与全部启用约束两两比对 duplicate/conflict，从源头杜绝自相矛盾。
- `compile_constraint_maps` 预编译（:809-870）：8 张按 student_id 索引的哈希映射，算法判座降为查表。
- `_apply_tag_rule_to_maps`（:767-806）：标签规则在编译层展开进映射，算法无需感知标签来源。
- `build_constraint_diagnostics`（:661-721）：结构有效→两两冲突→当前违规三层汇总，**四级状态** disabled/error/warning/ok，把「该修规则」与「该调座位」分开。
- issue 按严重度排序（invalid<conflict<duplicate<violated）。
- `_current_violation_message`（:620-658）：pair 有人未入座报「未同时入座」而非距离错误。
- 约束 CRUD 全可撤销（views.py:13353-13481）。

### 2.2 谓词库 + 自然语言渲染 + 冲突检测（SeatingChartEditor2）
- `PREDICATE_META` 声明式谓词元数据（ruleTypes.js:128-304）：位置（IN_ROW_RANGE/NOT_IN_COLUMN_TYPE/IN_ZONE）、对关系（MUST_BE_SEATMATES/DISTANCE_AT_MOST/NOT_BLOCK_VIEW/MUST_BE_SAME_GROUP/MUST_BE_ADJACENT_ROW）、分组分散（DISTRIBUTE_EVENLY/CLUSTER_TOGETHER）、数值参考（ATTRIBUTE_ROW_GRADIENT/ATTRIBUTE_GROUP_BALANCE/ATTRIBUTE_DISTRIBUTE_BANDS）。
- 自然语言渲染 `renderRuleText`（useSeatRules.js:201-644）：规则渲染成「张三必须坐在第 1 至 3 排。」级别中文句子；所有消费方统一走它，不暴露谓词名。
- 权重：required 10 万/prefer 1000/optional 10。
- 冲突检测 `detectConflicts`（:489-644）：仅遍历 enabled 规则两两组合，**先判作用域重叠再判矛盾**（避免无关规则误报），WeakMap 缓存展开集合；8 类谓词对冲突 + 2 类 zone 几何冲突；120ms 防抖自动扫描。
- RuleBuilder 表单（1183 行）：谓词下拉按 optgroup 分类、选中自动填参、实时反馈、120ms 防抖。

### 2.3 Capability 能力系统（Seatflow）
- Capability.cs:16-54：manifest 声明能力 → Facade 注册 → Workspace 运行时校验 → 接口调用；内置策略受信任可直改，插件强制走声明-校验（安全模型刻意不对称）。

---

## 3. 轮换 / 历史 / 快照域

### 3.1 自包含快照 + 完整性校验（Seatflow）
- 快照嵌入布局 + venueHash（ApplicationFacade.cs:596-605）：`venueHash`+`venueFile` 原始 JSON，自包含可移植。
- 回滚自动备份（:626-630）：回滚前先存快照，两级可撤销。
- 三色警告栏（SnapshotHistoryViewModel.cs:242-293）：venueHash/studentHash 双重校验，会场被删→从快照 venueFile 恢复、会场变更→询问导入为新会场。
- 固定座位豁免（SeatingWorkspace.cs:133-165）：ApplySnapshot 先清空非固定座，固定座不被清不改。
- 跨会话历史恢复（FrontRowHistoryLoader.cs:29-99）：从最近 N 个快照反向遍历填充 RecentSeatHistory。
- 快照轮转（ApplicationFacade.cs:531-545）：MaxSnapshotsPerVenue 默认 30。
- CircularHistory 去重 + 动态扩容（CircularHistory.cs:35,68-82）。

### 3.2 撤销/重做（open_fuckseats + SCE2 + Seatflow）
- open_fuckseats：单 `is_applied` 布尔翻转 undo/redo（models.py:458）；**约 50 种 action_type 几乎每个写操作可撤销**；历史状态 zlib 压缩入 JSON；前后状态相等不入栈；新动作清空 redo 栈；undo 后 created_at 回档；`.seats` PK 重映射让历史跨教室回放。
- SCE2：整次排位=单条 undo 组（前快照→clearAll→逐条 recordUndo=false→后快照→recordBatch）；历史上限可配 10-100；撤销后受影响座位 2s 灰色脉冲高亮。
- Seatflow：命令双栈（成功才入栈并清空 redo、Undo 先 Peek 验后弹）；UI 撤销用赋值字典快照而非命令回放（避免双重累积）。

### 3.3 选区/区域轮换（SCE2）
- 圈选座位成 zone、绑标签、cycle/swap/位移三种轮换；方向十字面板 + 「向后 N 行，向右 M 列，溢出列移 X」自然语句实时拼接。
- 位移轮换 shiftSeats 环形位移（useSeatChart.js:496-525）。

### 3.4 自动保存 + 崩溃恢复（SCE2）
- 固定间隔轮询 + 签名去重（删 meta.createdAt 后序列化比对）；`handled-time` 保证每个备份只提示一次；恢复成功记入最近工作区。
- 运行时快照 `captureRuntimeSnapshot`（打包 workspace+undo 栈+全部 UI 模式），加载三段式失败全量回滚。

---

## 4. 数据 / 导入导出域

### 4.1 Excel 智能导入（open_fuckseats + SCE2 + Seatflow）
- open_fuckseats：三段式 upload→preview→confirm（UUID 临时文件）；表头归一化（去空格/下划线/中括号）；别名集合+权重自动定位表头行与列（无需手选）；自定义列映射 custom_data（40 列上限）；replace/match 双模式；性别中英文兼容、成绩 pd.to_numeric 容错；班级组整组导入按班级列分桶。
- SCE2：全角转半角预处理；学号/姓名按位置查找非固定前两列；类别/数值关键词；**0/1 编码列不误判数值**（非 0/1 数值比例≥0.85 才推断）；标签真值语义；materializeAttributes 自动建缺失属性；可编辑预览表（每格可改实时重算）；模板下载。
- Seatflow：FuzzyColumnMatcher 三阶段（逐单元格扫描→标准模板判定→方向判定）；**2-连续空终止（各列独立）**；双列名单锚点分组（`|名字|身高|名字|性别|`）；field_mappings.json 别名库（Name 41 个别名）；EPPlus 合并单元格值扩展。

### 4.2 Excel 座位布局导入（open_fuckseats）
- 合并单元格还原（讲台大字）；中文关键词分类（讲台/教师/黑板/走廊/过道/空位…）；姓名启发式（2~5 个非数字字符）；自动定位讲台行 + rotate_180 检测；flip_ud/flip_lr/rotate_180 变换；预览只取首 2 末 2 行；`_sync_seats` 按姓名匹配已有学生事务原子。

### 4.3 导出
- open_fuckseats：
  - xlsx A4 横向 fitToWidth/Height 单页；边框只加在有学生的座位。
  - SVG：9 开关 + 三套主题（classic/minimal/contrast）；长名自动缩字号。
  - PPTX 16:9 (13.333×7.5 英寸)；圆角矩形 + 手动注入外阴影；像素→磅换算；同时设 latin/ea/cs 三 typeface 强制中文。
  - 小组作业登记表：组长红字置顶；**双栏平衡拆分**（按行权重累计一半处分栏，跨栏插「(续)」表头）；A4 纵向单页、行高自适应；「姓名 | 5 个打分框」登记行。
  - CSIS/CSLS 标准交换导出（与外部学生信息标准工具互操作）。
  - 快照 load：include_students/replace_students 双模式；`_find_student` 按 PK→学号→姓名三级兜底。
- SCE2：
  - 高清 PNG A4@300DPI（2480×3508）；**MAX_CANVAS_PIXELS 内存保护**（超限抛错防 iPhone 崩溃）；黑白/灰度自适应（亮度公式 + pureBw 反相）；roundRect 回退；toBlob→ObjectURL 避免 base64 体积。
  - 富样式 Excel：`%n\n%i` 富文本模板 + 内联样式（姓名学号同格不同样式）；合并单元格；编号方案 arabic/alpha/roman/circled/中文小写/中文大写（含万/亿分组）；翻转视图只改导出不改数据；标签统计表；旧配置迁移。
  - SDES 标准交换格式：`format='student-data-exchange-schema'` + version + manifest + classes；结构化名字；坐标系 origin 四角 + xDirection/yDirection；**转换报告**（skippedStudents/unsupportedGuards/丢失前导零）；grid 导入空列当分隔符推断分组。
  - 打印链路 = A4 优化 PNG + 黑白模式。
- Seatflow：
  - CSV/Excel 多 Sheet+样式/PDF QuestPDF 动态页面尺寸 A4横向~A0/PNG SkiaSharp **动态 CJK 字体匹配**（MatchCharacter('中')）。
  - 教师视角镜像反转（Rows.Reverse + Cells.Reverse，一行实现讲台移底+左右镜像）。
  - 匿名化选项。
  - 文件被占用自动降级 CSV。
- RandomSeat：xlsx 组长黄色高亮 + 种子行；按扩展名分发导出（xlsx/xls/csv）；日期命名输出文件。

### 4.4 完整备份 / 交换
- open_fuckseats `.seats` bundle v2.0：snapshot+current_state+完整操作历史（zlib 压缩）+ AI 配置；**PK 重映射体系**让历史跨教室回放；全量/legacy 双模式。
- Seatflow `.seatsets` 存档包：双层 SHA256 + **路径穿越 `..` 拒绝** + 200MB 上限 + **全项目唯一 temp+rename 原子写** + 首次启动自动发现。
- SCE2 SDES 交换 + FuckSeats 导入（SSRF 白名单：localhost/白名单端口/白名单路径/无 query + 前端后端双重校验）。
- open_fuckseats BSCE 导入：浏览器指纹伪装（4 组 UA 池）、PBKDF2 100k 迭代 + AESGCM 加密登录密码、SimpleCookie 会话维持、证书失败降级一次。

---

## 5. 数据安全 / 隐私域

### 5.1 整库加密（open_fuckseats）
- SQLCipher AES-256 整库加密 + keyring 密钥（database_security.py:82-119）+ 明文→加密原子迁移（崩溃恢复状态机 relocating/encrypting/verifying/committing/ready）+ 更新前加密备份。
- keyring 写入后立即回读校验；keyring ImportError 抛错而非退化明文；frozen 版禁止关闭加密。
- 多实例文件锁 + 目录 chmod 0o700。
- 云 E2EE：RSA-OAEP-SHA256 + AES-256-GCM 信封；key_id 派生（sha256(公钥PEM)hex 前 32 位）；双向密钥绑定防中间人；服务端密钥轮换。
- 数据分享 opt-in + install_id 匿名 + 上传域名锁定 + 每批≤50 + 熔断退避。
- 前端 KVStore：monkey-patch `Storage.prototype` 让 localStorage 落库加密，对既有代码零侵入。
- OpenAPI/MCP 工具元数据：read_only/danger_level/oral_confirmation。
- 常数时间比较（secrets.compare_digest）；云同步墓碑 + 版本乐观锁 + operation_time。

### 5.2 其它
- SCE2：FuckSeats SSRF 白名单（前端+后端）；登录 PBKDF2 10 万次 + AES-256-GCM；双提交 CSRF Cookie；passwordValidator；xss.js 转义 + colorContrast 完整 WCAG 实现。
- Seatflow：EPPlus CVE 覆盖；内容哈希唯一入口；确定性构建 + PathMap。
- RandomSeat：Gradle wrapper 校验 CI（供应链）；CVE 注释规避。

---

## 6. UI / UX 域

### 6.1 可视化交互编辑（SCE2，本项目最强）
- 5 种编辑模式 NORMAL/SWAP/CLEAR/EMPTY_EDIT/ZONE_EDIT（useEditMode.js:4-10），模式即状态机。
- 原生拖影屏蔽（setDragImage 1×1 透明 div）+ Teleport 浮动预览（每座一张卡按真实 gap 排布）。
- moveSelection 多选整体平移 + **6 步挤兑算法**（越界拖拽不丢人，被挤学生沉降进腾出的源位）。
- 拖入落点高亮投影 + 脉冲动画；多选落点 computed 目标集（越界/护法剔除）。
- 框选（Shift+右键矩形 <5px 误触）/涂抹（右键拖动 >3px）/触屏涂抹 ADD/REMOVE 方向。
- 触屏长按 300ms + navigator.vibrate(50) + 移动>10px 取消防误触 + elementFromPoint 定位 + CustomEvent 冒泡。
- `lastPointerWasTouch` shallowRef 记录，解决触屏笔记本误判。
- 缩放平移：0.2-3.0、Ctrl/CMD+滚轮缩放、rAF 节流、纯 CSS transform 不触发重排、fitToViewport 去 transform 测真实尺寸。
- 触屏 pinch + 单指平移 + 贴边 54px 自动滚屏 + `touch-action:none`。
- 双击按设置分流（edit/random 移出）。
- 二次点击确认 useConfirmAction（3s 超时自动作废）替代 modal，全站 8 处。
- 撤销/重做后受影响座位 2s 灰色脉冲高亮。
- 学生卡标签三态显示（dot 空心点画法一眼看出缺标签）。
- 右键涂抹抑制 contextmenu 5000ms。
- 三栏响应式（<1440 折叠标签、<1024 移动端抽屉 + 下滑 >64px 关闭 + 横屏锁定）。
- focusSeatContext 拖完即看。
- 空护法默认不占位、单侧护法/顶部讲台场景齐全。

### 6.2 教师工作流引导（open_fuckseats + SCE2 + Seatflow）
- open_fuckseats：命令面板 + 斜杠命令（中文别名四匹配：英文/中文/全拼/压缩 token）；智能建议条（一键 apply/忽略）；气泡式新手引导（自动建示例班级走全流程）；3D Toast 栈（最多4/排队8/滑动关闭）/水波纹/Spotlight/灵动岛；桌面端原生保存对话框 + 原生右键菜单桥。
- SCE2：onboarding 一次性弹窗 + 应用内完整手册（userManual.ts 12 section，README 规定用户可见变更必须同步）。
- Seatflow：JSON 驱动 Onboarding（9 阶段 + seedData 注入 + 引导后清理还原）；10 页导航 + 脏检测守卫 + 淡入淡出；点击/拖拽区分（DoDragDropAsync 返回 None）；快捷键跨页分派 + 6 开关可关；中文输入归一化（全角→半角）；空状态大图标+双行提示；致命错误跨平台弹窗（MessageBoxW/zenity/osascript）；跨平台头像。

### 6.3 其它（RandomSeat）
- Apply 按钮按改动启用（@EqualsAndHashCode 对比）；Material You monet CSS 变量主题（明暗只换变量表）；种子交互（回车生成/未变重随机/Random/Time）；平台感知快捷键；空状态先画网格；崩溃报告复制堆栈；IntegerField 数字输入。

---

## 7. 工程化 / 质量域

### 7.1 文档与 Agent 协作（SCE2）
- 单一真源：scripts/sync-agent-docs.js 从 shared-agent-guide.md 生成 CLAUDE.md/AGENTS.md/.trae 三入口 + check-agent-docs.js 校验漂移。
- `.agents/features/` 11 篇深度功能文档（统一结构：frontmatter→职责→源码入口表→数据结构→关键实现节点→「AI 开发提示/防坑」）。
- userManual.ts 强制随功能同步。
- agent-reuse-kit：bugfix-workflow（0 启动→1 边界→2 rg 定位→3 分层诊断→4 最小修复→5 最小测试→6 文档沉淀→7 收尾）编码成可复用技能。
- 版本单一真源：set-release-version.js 从单个 UTC 时间戳派生 SemVer/package/Cargo/MSI 四段/Release tag。
- CI 三件套：test.yml（type-check+vitest+e2e）、build-release.yml（Windows NSIS/MSI 自动发 Release）、deploy.yml（分支分流 + 3 次重试）。
- Playwright 双视口（desktop-chromium + mobile-chromium 同一 spec）；vitest 覆盖率阈值 70/70/60/70（只统计 composables）。
- vite.mock.plugin.js 在 dev server 完整模拟后端 PHP（含双提交 CSRF 校验）；patch-test-env.js 测试环境强区分。

### 7.2 发布与自动化（Seatflow + RandomSeat）
- Seatflow：409 测试（Core 200/App 84/Infra 125）零 Theory；unit-tests.yml 路径过滤（改动哪里测哪里）；release.yml（OSS 原子发布 + SHA256 表 + 版本强制递增 + RELEASE.md 校验）；**dependabot-auto-release 全自动安全发布闭环**；文件版本迁移管线（8 类文件版本）；11 篇 ADR（含 Fill-in-Order 4 次修订的诚实复盘）；docs/INDEX.md 职责矩阵 + 变更场景联动表；看门狗 + 单实例 + 命名管道文件转发 + 窗口关闭同步写。
- RandomSeat：Shadow fat jar；license 头自动维护；wrapper-validation CI；tag 触发自动发版；构建期注入版本。

### 7.3 桌面打包（open_fuckseats）
- PyInstaller onedir 精心配置（hidden-import 清单、剔除嵌入数据库、环境变量脱敏）；macOS codesign+pkgbuild+notarytool+stapler；更新状态机 + 128KB 分块下载 + 危险静默参数正则拦截 + macOS team_id 强制核验 + 更新前数据库备份。

---

## 8. 业务功能域

### 8.1 班级组 / 多班工作流（open_fuckseats）
- ClassroomGroup（班级集合）+ ClassroomGroupStudent（待分配学生池，镜像 Student 字段）+ 批量建班（至多 100 个，复用布局）。
- 跨班级组排座（fill_classrooms/prefer_original/占用率均衡三种人数分配模式）。
- 自动编组：参照组样式识别（vertical/horizontal/nearby）+ 形状画像（block/corner/line/irregular + 90°×4 旋转变体）+ 就近聚簇打分 + remainder 三策略（skip/new_group/merge_prev）。
- 组轮换 rotate_groups（质心排序循环平移）、合并（组长继承）、组长跟随（follow 模式）、组长自愈（不在组内自动清空）、智能续号命名。

### 8.2 护法座 / 讲台（open_fuckseats + SCE2）
- 护法 = 布局派生而非数据（讲台左右第一把椅子即护法），显式字段只是旧数据兜底，「布局即事实」。
- SCE2：护法座固定 id guard-left/right、坐标 -1、可坐人但默认不参与排位、讲台在顶时左右互换。

---

## 9. SeatTrellis 差距矩阵与建议

按「价值 × 成本 × 与 Rust-first 本地优先的契合度」综合排序（全部保留，不做预设排除）：

### 第一批：低成本高价值，可直接做
1. **规则自然语言渲染 + 冲突检测**（SCE2 useSeatRules）→ React 工作台规则面板；成本中。
2. **快照自包含 + venueHash 完整性 + 回滚前备份**（Seatflow）→ 给历史轮换加固；成本低。
3. **排座预检查门禁 + 审计报告点击定位**（SCE2 assignmentPrecheck）→ 成本低-中。
4. **约束冲突实时诊断**（open_fuckseats constraints.py:661）→ 成本低。
5. **Excel 智能列识别导入**（三项目共识）→ 成本中。
6. **自动保存 + 运行时快照回滚**（SCE2 useWorkspace）→ 成本低。
7. **undo/redo 栈**（Seatflow CommandHistory + open_fuckseats）→ 待 React 编辑器有交互后。

### 第二批：大工程，值得规划
8. **可视化交互编辑器**（SCE2 拖拽/框选/多选移动/缩放平移）→ React 2-3 周，roadmap 已立项。
9. **SQLCipher 整库加密 + keyring**（open_fuckseats）→ Rust rusqlite+sqlcipher。
10. **班级组/多班工作流**（open_fuckseats）→ 需聚合模型 + UI。
11. **选区 + 区域轮换可视化**（SCE2）→ 与逐期快照轮换互补。
12. **依赖策略三态裁决 + 约束学生优先**（Seatflow）→ 求解器可解释性增强。
13. **Defrag 后处理 + IsFixed 保护**（Seatflow）→ 填前空隙。
14. **Capability 能力声明系统**（Seatflow）→ 规则/策略能力声明。
15. **MCP 工具注册中心**（open_fuckseats open_api）→ 在现有后端包一层。

### 第三批：需重新评估（此前被过早否定）
16. **第三方迁移互操作**（SCE2 FuckSeats 导入 + open_fuckseats BSCE 导入）→ 方向是「从其它排座工具迁移」，SSRF 白名单样板可直接复用；竞品都做了互操作，这是生态位。
17. **可选云同步**（open_fuckseats E2EE + SCE2 WebDAV）→ 本地优先不冲突；E2EE 信封方案可学。
18. **插件/扩展** → Rust 端动态插件难，但**声明式策略 manifest + i18n 词典消息**（Seatflow）零插件也可做。
19. **遥测** → 改为 opt-in 崩溃报告 + 匿名用量（熔断退避）。
20. **声明式自定义排序 + 拼音/自然排序**（open_fuckseats sorting.py）→ 前端可做。
21. **serde 字段别名兼容 + builtin 回退 + LENIENT 解析**（RandomSeat）→ 配置升级兼容。
22. **内置 CJK 字体打包**（RandomSeat 19M ttc）→ 导出中文渲染一致性。

### 工程化可借鉴（低成本高杠杆）
23. agent 文档单一真源（SCE2 sync-agent-docs.js）。
24. 版本单一时间戳派生 + Release 自动化（SCE2 + Seatflow + RandomSeat）。
25. Playwright 双视口 E2E + vitest 覆盖率阈值。
26. release.yml 原子发布 + SHA256 表 + 版本强制递增。
27. ADR 机制 + docs/INDEX 职责矩阵。

---

## 10. 明确不需要做的（有依据，非预设）

- **全套 cookie 账号体系 + Retinbox 专属部署链**（SCE2）：绑定具体商业后端，与本地优先无关；需要的是通用 E2EE 云同步模式而非账号系统。
- **SQLCipher 的云端双库强制**（open_fuckseats backend）：桌面单机场景不需要服务端静态加密基建。
- **RandomSeat 的 JDK25/Shadow fat jar**：技术栈差异，无借鉴价值。
- **open_fuckseats 的 PyInstaller 打包链**：已被 Rust-first 取代。
- **AI 对话/未来模式**（open_fuckseats FutureModeConfig 且已关闭）：OpenAI 集成与本地优先冲突，且他们自己已关闭。
- **遥测热力图/数据分享**（open_fuckseats data_sharing）：仅 opt-in 崩溃报告有价值。
