# Core 输入校验边界审计（2026-08-12）

审计范围：`crates/seattrellis-core/src/` 的求解器与模型（solve 入口、greedy/backtracking/local
search、候选生成、audit/score、graph 构造）。`repair.rs` 与 `tests/repair_empty_seat_lock.rs`
由另一 agent 修改，本审计未触碰。

触发背景：`docs/audit-2026-08-12-export.md` 发现极端坐标（1e300）能通过核心校验并导致导出侧
溢出，故对 core 求解器输入边界做系统性审计。

方法：全量阅读 core 源码 → 以新测试文件 `crates/seattrellis-core/tests/input_boundary.rs` 复现
嫌疑问题（debug 构建下 i32/i64 溢出即 panic）→ 对确证 bug 做最小修复并回归。

## 结论摘要

- **确证并修复 2 类 panic**（P1）+ **1 个校验缺口**（P2），共 9 处代码修改，7 个新回归测试。
- **只读报告 6 项**（P2×2、P3×4），其中 2 项（规模上限、幅值上限）属契约级决策，需产品/计划拍板。
- 修复后 `cargo +1.88.0 test -p seattrellis_core` 全绿（lib 80 + 集成套件 + 新增 7 测试），
  `cargo +1.88.0 clippy --all-targets -p seattrellis_core -- -D warnings` 干净。
- 未做任何 git commit/push；未触碰 cli/export/scripts/fixtures/clients/web。

---

## 一、已修复问题

### P1-1 极值有限坐标 → i32 行/列差运算溢出（debug panic / release 静默回绕）

**现象**：`seat_positions` 只校验 `is_finite()`（engine.rs:1206-1211），1e300 这类
"有限但巨大"的坐标直接放行。`effective_layout`（engine.rs:100-124）做
`position[1].round() as i32`，饱和到 `i32::MAX` / `i32::MIN`。随后所有 `(row - col)` 型
i32 减法溢出：**debug（测试/开发构建）panic，release 静默回绕产生错误数值**（邻接判定、
成本、报告全部失真）。Python oracle 用任意精度整数，永不溢出——这是纯 Rust 侧回归。

**复现**（修复前，7 个用例全部 panic，见 `tests/input_boundary.rs`）：

```json
{"api_version":2,"student_count":2,"seat_positions":[[0.0,1e300],[0.0,-1e300]]}
```

`solve_problem_json` → `build_cost_context` → `compile_soft_objectives` →
`build_adjacency_edges` → `are_adjacent` → `(first.row - second.row).abs()` → 溢出 panic。

**溢出点（已修复，全部改为 i64/f64 差值，常规输入语义不变）**：

| 位置 | 原代码 |
|---|---|
| objectives.rs:679（原 673-674） | `(first.row - second.row).pow(2) as f64` → f64 差值 `powi(2)` |
| objectives.rs:686-687（原 679-680） | `(first.row - second.row).abs()` → i64 差值 |
| objectives.rs:623（原 621） | `(first.col - second.col).abs() == 1` → i64 差值 |
| cost.rs:446-447（原 426-427） | `are_adjacent` 行/列差 → i64 差值 |
| cost.rs:357（原 339） | `detect_neighbor_relation_types` 行/列差 → i64 差值 |
| cost.rs:103、116（原 99、103） | `individual_cost` 的 `row - min_row` / `max_row - row` → i64 差值 |
| scoring.rs:342、389（原 340、386） | 行归一化 `(seat.row - min_row) as f64` → f64 差值 |
| reports.rs:370-371（原 368-369） | pair report 行/列差 → i64 差值 |

对比语义保持：`max_col_delta`/`within_distance` 等比较处显式 `i64::from(...)`，负数输入行为
与原来一致。

### P1-2 超大有限 `height_cm` → i64 成本链乘法溢出（debug panic）

**现象**：`height_cm` 无任何校验。`height_cm: 1e300` 经 `round_half_even` 饱和为
`i64::MAX`（cost.rs:59-74），再与 weight、行差相乘（`i64::MAX × 2 × 1`）即溢出。

**复现**：2 个座位 y=0/y=2，学生 height_cm=1e300，`height_back.weight=3` →
`cost.rs:104`（原 104）panic。

**修复**：cost.rs:103-121 成本链改为 `saturating_add`/`saturating_mul`。常规身高数值行为
与之前完全一致；荒谬输入不再崩溃，成本钳制在 i64 范围（与 Python 大整数成本存在钳制分歧，
见建议 B）。

### P2-1 `students[].score` / `students[].height_cm` 非有限值未校验（进程内 DTO 边界）

**现象**：`student_scores` 数组有有限性校验（engine.rs:1252-1255），但更完整的
`students[].score` 与 `students[].height_cm` 没有。NaN score 会污染百分位 → 软目标成本 →
`total_cost = NaN`，序列化后变成 JSON `null`（serde_json 对非有限 f64 输出 null）；NaN
height 静默按 0 计成本（Python `round(nan)` 直接抛 ValueError）。

**注意**：JSON 线上边界其实挡得住 NaN/±inf——`1e999` 这类字面量在 serde 解析期即报
`number out of range`，NaN 无 JSON 字面量。因此该缺口只影响**进程内直接构造
`CoreSolveRequest` 的调用方**（repair/io/server 未来路径），属纵深防御，与既有
`student_scores` 校验口径一致。

**修复**：engine.rs:1230-1241 在 `validate_solve_request` 中新增 score/height_cm 有限性
校验（错误消息含 "invalid"，`classify_solve_error` 归类为 `InvalidInput`）。

---

## 二、只读报告项（未修复，含建议）

### P2-2 无 student_count / seat_count 上限 → O(V²) 距离矩阵 DoS

- `validate_solve_request`（engine.rs:1341-1342）**无条件**调用
  `build_graph_distance_matrix`（O(V²) 内存、O(V·E) 时间），即使请求没有任何
  `min_distance` 规则。V=10k 座位即约 900MB 内存；solve/precheck/scoring/audit 全走该校验。
- `backtrack`（engine.rs:1023-1048）每节点 `domains.to_vec()` 全量拷贝 + 递归深度 =
  student_count；greedy 尝试数 `student_count * 12`（solver.rs:426）。
- 组合效果：超大图/超多学生请求可挂死进程或 OOM；`time_limit_seconds` 能兜底时间，但
  内存分配发生在限时检查之前。

**建议**：在 `validate_solve_request` 增加规模上限（如 student ≤ 500、seat ≤ 2000，产品
拍板），并让距离矩阵仅在存在 `min_distance`（graph metric）时构建。属契约变更，需过
修订版计划，本审计未改。

### P2-3 极值有限分数（±1e308）→ 差溢出 inf → 响应字段变 JSON null

**已复现**（scratch 探针，随后删除）：`student_scores: [1e308, -1e308]` + 一条 edge 时，
`evaluate_problem_json` 返回 `"peer_mixing_gap_sum": null, "peer_mixing_mean_gap": null`
（evaluation.rs:337-369 的 `gap_sum += (first - second).abs()` 溢出为 inf）。同理
`solve` 的 `total_cost`（engine.rs:229-230 score_balance 项）与 audit 的 `loss` 可为
±inf/NaN 而序列化为 null。不 panic，但破坏"字段必为数值"的契约。

**建议**：校验分数幅值（如 |score| ≤ 1e9，成绩类数据远小于此），属契约决策，本审计未改。

### P3-1 `layout.adjacency.max_distance` 等浮点配置未校验

`AdjacencyConfig.max_distance` 接受 NaN（→ 全部座位不相邻，静默空图）、±inf（→ 全连接
图，O(V²) 边）、负值。`mentor_percentile`/`learner_percentile` NaN 同样静默降级为空配对
集。不 panic、有界，但输入契约无文档。

**建议**：`validate_solve_request` 校验 `layout.adjacency.max_distance` 有限且 > 0。

### P3-2 重复 seat 未校验

`seat_positions` 允许完全相同的坐标（solver 按索引工作，与 Python 行为一致，无 panic）；
`layout.seats` 允许重复 `seat_id`（会向邻接边集写入 `(id, id)` 自环，仅影响软目标侧的
边集合查询，无硬约束影响）。与 Python oracle 行为一致，属低危，建议后续在 io/import 层
拒绝重复 `seat_id`。

### P3-3 极值坐标下 i32 饱和行/列与 Python oracle 数值分歧

1e300 坐标经 `round() as i32` 饱和为 ±2147483647，而 Python 保留 1e300 精确值；成本/
百分位等数值输出必然不同（无崩溃、无 golden 覆盖此类输入）。**建议（推荐）**：与
`audit-2026-08-12-export.md` 联动，在 `validate_solve_request` 增加坐标幅值上限
（如 |x|,|y| ≤ 1e6，课堂布局远小于此），从源头同时解决 core 分歧与导出侧溢出。属契约
变更，需产品拍板，本审计未改。

### P3-4 evaluation 入口 `student_count == 0` 未拒绝

`evaluate_problem`（evaluation.rs:103-175）的 `validate_request` 未校验
`student_count == 0`，会返回"空班评估通过"。与 solve 入口口径不一致，无 panic，低危。

---

## 三、修改文件清单（均未 commit）

| 文件 | 修改 |
|---|---|
| `crates/seattrellis-core/src/cost.rs` | individual_cost 饱和成本链（103-121）；detect_neighbor_relation_types（357-358）；are_adjacent（446-447）改 i64 差值 |
| `crates/seattrellis-core/src/objectives.rs` | relation_satisfied（623）；are_adjacent（679、686-687）改 i64/f64 差值 |
| `crates/seattrellis-core/src/scoring.rs` | 行归一化两处改 f64 差值（342、389） |
| `crates/seattrellis-core/src/reports.rs` | pair report 行/列差改 i64（370-371） |
| `crates/seattrellis-core/src/engine.rs` | `validate_solve_request` 新增 students[].score / height_cm 有限性校验（1230-1241） |
| `crates/seattrellis-core/tests/input_boundary.rs` | **新增** 7 个回归测试 |

## 四、测试结果

```
cargo +1.88.0 test -p seattrellis_core        # 全绿
  - lib: 80 passed（含既有 time_limit 测试，未改动）
  - input_boundary（新增）: 7 passed
  - 其余集成套件（property/exact_differential/fuzz_parsers/long_run_gate(ignored)/repair_empty_seat_lock）: 全绿
cargo +1.88.0 clippy --all-targets -p seattrellis_core -- -D warnings  # 干净
```

修复前 7 个新测试全部按预期失败（panic 于 objectives.rs:679、cost.rs:104、cost.rs:339，
或校验缺口导致断言失败），修复后全部通过，确认测试真实覆盖缺陷。

## 五、后续建议（未实施，需过计划/产品）

1. **坐标幅值上限**（P3-3，推荐优先）：`|x|,|y| ≤ 1e6`，同时关闭导出侧溢出与 oracle 分歧。
2. **规模上限**（P2-2）：student/seat 计数上限 + 距离矩阵按需构建。
3. **分数幅值上限**（P2-3）：保证响应数值字段永不为 null。
4. **配置字段校验**（P3-1）：`max_distance`/百分位阈值有限性。
