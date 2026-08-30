# CLI 命令行完全参考手册

[English](cli.md) · [简体中文](cli.md)

**席序（SeatTrellis）v2.0.0** 提供了功能完备的命令行工具，共包含 **27 个功能子命令** 与完整的诊断自检体系。所有命令均遵循语义化退出码设计，便于在脚本自动化、持续集成（CI）与批处理任务中稳定集成。

---

## 🧭 1. 命令概览索引

| 命令分类 | 子命令 | 功能描述 |
| :--- | :--- | :--- |
| **环境与诊断** | `doctor` | 检查 CLI 二进制版本、Core API 版本及临时目录写权限。 |
| **单文件核心工作流** | `validate` | 校验问题文件（`CoreSolveRequest`）与规则合法性（不运行求解）。 |
| | `precheck` | 诊断学生候选座位域与不可行原因。 |
| | `solve` | 执行智能求解并输出排座方案快照。 |
| | `audit` | 审计已求解方案的硬约束合规性与各项软偏好得分。 |
| | `score` | 对指定的排座分配矩阵进行即时打分。 |
| | `candidates` | 一次性生成多套满足硬约束的差异化候选方案集。 |
| | `export` | 将已求解方案渲染为图片、矢量图或办公文档。 |
| **微调与智能修复** | `edit` | 对方案快照应用互换、移动、锁定等手动微调操作。 |
| | `repair` | 保持关键锁定座位不变，仅对冲突学生进行局部智能重排。 |
| **历史与搭档分析** | `history-report` | 汇总学生的历史位置区域分布（前排/后排/靠窗/角隅等）。 |
| | `pair-report` | 分析两两学生之间的历史同桌与相邻频次。 |
| **班级项目工作流** | `project-init` | 在包含名单和布局的目录下初始化班级项目清单。 |
| | `project-list` | 扫描并列出本地的所有班级项目。 |
| | `project-info` | 查看项目配置与引用文件的解析状态。 |
| | `project-validate`| 严格校验班级项目及其引用的全部文件。 |
| | `project-solve` | 针对班级项目执行排座求解。 |
| | `project-export` | 导出班级项目中的已存方案（直接渲染，不重复计算）。 |
| | `project-rotate` | 预测并生成未来多期公平轮换方案（1~20 期）。 |
| | `project-edit` | 对班级项目方案执行交互式微调。 |
| | `project-repair` | 对班级项目方案执行锚定局部修复。 |
| | `project-privacy` | 扫描班级项目中是否存在未脱敏的个人隐私字段。 |
| | `project-pack` | 将班级全量数据打包为 `.seattrellis.zip` 备份。 |
| | `project-restore` | 从备份压缩包中完整恢复班级工作区。 |
| **Schema 与迁移** | `schema-list` | 列出系统支持的所有 v2 数据 Schema 类型。 |
| | `schema-export` | 导出指定 Schema 的标准 JSON Schema 定义文件。 |
| | `schema-migrate` | 将 v1 格式的旧版文件就地或导出迁移至 v2。 |

---

## 🚦 2. 进程退出码规范（Frozen Exit Table）

SeatTrellis v2 采用冻结的退出码语义，严格区分各类结果：

| 退出码 (Exit Code) | 状态标识符 | 含义与判定条件 |
| :---: | :--- | :--- |
| `0` | `Solved` | **求解成功**：成功生成了 100% 满足所有硬约束的合法方案。 |
| `2` | `InvalidInput` | **输入错误**：命令行参数非法、文件不存在或 JSON 格式校验失败。 |
| `3` | `ProvenInfeasible` | **证明不可行**：数学上已严格证明当前硬约束组合不可能存在可行解。 |
| `4` | `Timeout` | **运算超时**：在限定时间（`--time-limit`）内未找到任何合法方案。 |
| `5` | `Unknown` | **结果未知**：算力搜索预算耗尽但未能严格证明无解（绝不伪装为不可行）。 |
| `70` | `InternalError`| **系统内部错误**：内存异常或底层 I/O 故障。 |
| `130`| `Cancelled` | **用户中断**：收到 `SIGINT` (Ctrl+C) 中断信号。 |

---

## 💻 3. 重点命令参数与示例

### 3.1 `solve`（核心求解）
```bash
seattrellis solve \
  --problem problem.json \
  --seed 42 \
  --time-limit 5 \
  --output outputs/latest.snapshot.json
```
- `--problem <path>`：必填，问题定义 JSON 文件路径；
- `--seed <int>`：可选，随机种子，固定后结果完全确定可复现；
- `--time-limit <seconds>`：可选，最大运行时间上限（秒）；
- `--output <path>`：可选，将完整求解快照保存至指定文件。

---

### 3.2 `candidates`（多候选方案生成）
```bash
seattrellis candidates \
  --problem problem.json \
  --count 5 \
  --latest-snapshot history/last_term.snapshot.json \
  > outputs/candidates.json
```
- `--count <n>`：生成的候选方案数量（1 ~ 20，默认为 5）；
- `--latest-snapshot <path>`：提供最近一期的历史快照，用于评估方案稳定性得分。

---

### 3.3 `edit`（交互式手工调整）
```bash
seattrellis edit \
  --snapshot outputs/latest.snapshot.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R1C1 \
  --output outputs/edited.snapshot.json
```
支持的微调操作（`--operation`）：
- `swap:<student_a>:<student_b>`：互换两位同学；
- `move:<student>:<seat_id>`：将同学调动至指定空位；
- `lock-student:<student>` / `unlock-student:<student>`：锁定/解锁学生；
- `lock-seat:<seat_id>` / `unlock-seat:<seat_id>`：锁定/解锁座位；
- `batch-move:STU01=R1C1,STU02=R1C2`：原子化批量位置迁移。

---

### 3.4 `repair`（约束感知局部修复）
```bash
seattrellis repair \
  --problem problem.json \
  --snapshot outputs/edited.snapshot.json \
  --affected STU002 \
  --lock-seat R4C3 \
  --output outputs/repaired.snapshot.json
```
- `--affected <student>`：限定只允许重新分配座位的受影响学生（可重复指定）；
- `--lock-student` / `--lock-seat`：追加临时锚定锁定；
- `--ignore-saved-locks`：忽略快照中原有的锁定标记。

---

## 📖 相关文档

- [快速上手指南](quickstart.zh.md)
- [输入数据格式规范](input-format.zh.md)
- [排座规则手册](rules.zh.md)
- [班级项目工作流](project.zh.md)
