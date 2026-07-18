# ADR-0003：人工调整采用命令与操作日志模型

- 状态：接受
- 日期：2026-07-06

## 背景

拖拽换座、锁定、批量移动、未入座区、撤销/重做和局部修复会同时出现在 Web、
桌面端和未来的其他适配器中。若这些语义只存在于 UI 状态中，将无法可靠验证、
保存或复现人工修改。

## 决策

人工调整围绕与 UI 无关的 `EditingSession` 建模。每次变更是显式命令，例如：

- `SwapStudents`
- `MoveStudent`
- `UnseatStudent`
- `LockStudent`
- `LockSeat`
- `BatchMove`

成功执行的命令写入操作日志，并提供逆操作以支持撤销/重做。每次操作返回新的
约束诊断；发布 snapshot 时记录来源、锁定状态和人工修改摘要。

当前实现边界：

- `seattrellis.editing.EditingSession` 已支持交换学生、单个/批量移动、移出座位、
  重新入座、锁定/解锁学生、锁定/解锁座位、撤销和重做；
- 批量移动先验证完整映射再原子提交，支持批次内循环换位，拒绝重复目标和隐式移出
  批次外学生；
- 编辑草稿允许存在未入座学生，但不允许重复学生、重复座位、未知学生或禁用座位；
- 锁定学生会冻结该学生当前状态，锁定空座会阻止后续放入学生；
- 每次成功操作都会复用现有 hard constraint 诊断，便于 Web 和桌面端实时提示；
- `compute_edit(EditInput)` 已作为 service 层入口，适配器可以提交命令序列并取得新草稿；
  `EditingLockState` 同时保存在返回结果和 `metadata.lock_state` 中；
- `compute_repair(RepairInput)` 将锁定、受影响学生和空座预留编译为一次性求解约束；
  指定受影响学生时，未受影响的当前已入座学生自动固定；
- repair 会复用历史方案以保持公平轮换和近期邻座语义，并在写出结果前同时复核原始
  hard rules 与临时修复约束；
- 这一层不改变 project 的持久化 rules 文件，也不另写一套局部搜索算法。
- 前端适配器通过版本化 `EditorCommandEnvelope` 提交 apply、undo 或 redo，
  通过 `EditorStateEnvelope` 读取数据最小化的编辑状态；
- 每份草稿拥有独立 `draft_id` 和单调递增 revision。服务端拒绝旧 revision、错误
  草稿和重复 `command_id`，成功命令只增加一次 revision；
- 一个命令中的多项 operation 原子执行并作为一个撤销批次，失败时不落盘。

## 后果

- Streamlit、TypeScript 编辑器和桌面端复用同一服务；
- 可以测试任意命令序列后的座位唯一性、锁定不变量和 hard constraints；
- snapshot 继续表示已发布结果，编辑中的草稿和操作日志使用独立模型；
- 局部修复先通过“固定未受影响学生并调用现有 solver”实现，不另写一套规则；
- `metadata.repair` 保留临时/有效固定关系、预留空座、历史数量和实际变化学生，便于
  UI 呈现与后续操作审计。
- `draft_id` 和 revision 只解决并发覆盖，不承担身份认证或授权。若协议将来通过
  网络开放，适配器必须另外验证会话所有权和请求来源。
