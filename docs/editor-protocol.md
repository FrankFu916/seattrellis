# 交互式编辑器命令协议（Editor Protocol）

[English](editor-protocol.md) · [简体中文](editor-protocol.md)

**席序（SeatTrellis）v2.0.0** 采用统一的 `protocol_version: "1.0"` 交互式编辑协议。该协议定义了 React 前端、Tauri 桌面外壳与 Rust 本地服务之间的通信契约。

---

## 📨 1. 命令与状态信封（Envelopes）

协议包含两类核心数据文档：
- **`EditorCommandEnvelope`**：前端向服务端发送的操作指令（应用微调、撤销 Undo、重做 Redo）；
- **`EditorStateEnvelope`**：服务端返回的最新草稿座次、锁定状态与约束诊断结果。

### 命令示例（`EditorCommandEnvelope`）

```json
{
  "kind": "seattrellis_editor_command",
  "protocol_version": "1.0",
  "command_id": "cmd-20260830-001",
  "draft_id": "7b7359c6f9cd4e128df8b9145d012ec1",
  "base_revision": 3,
  "action": "apply",
  "operations": [
    {
      "kind": "swap_students",
      "payload": {
        "first_student": "STU001",
        "second_student": "STU018"
      }
    }
  ]
}
```

---

## 🛠️ 2. 支持的操作指令集

| 操作类型 (`kind`) | 载荷参数 (`payload`) | 功能描述 |
| :--- | :--- | :--- |
| `swap_students` | `first_student`, `second_student` | 互换两名学生的座位。 |
| `move_student` | `student_key`, `seat_id` | 将指定学生移动到目标空座。 |
| `batch_move` | `moves: [{student_key, seat_id}]` | 原子化批量移动多名学生（一步可撤销）。 |
| `seat_student` | `student_key`, `seat_id` | 将未入座学生安排至指定空座。 |
| `unseat_student` | `student_key` | 将学生移出座位，放入未分配区。 |
| `lock_student` / `unlock_student` | `student_key` | 锁定/解锁学生的当前座次。 |
| `lock_seat` / `unlock_seat` | `seat_id` | 锁定/解锁指定座位。 |

---

## 🔒 3. 并发控制与版本一致性

- **单调递增版本号（`revision`）**：每次成功的操作（Apply/Undo/Redo）使草稿版本号精准 +1；
- **防冲突拦截**：若提交的 `base_revision` 与当前草稿版本不匹配，服务端将返回 `EditorProtocolConflictError` 并拒绝写入，确保多端操作安全。
- **主动销毁**：`DELETE /api/v1/editing/drafts/{draft_id}` 同时删除编辑草稿及其对应的原始求解请求；工作台在上下文切换和页面卸载时调用该端点。

## 🧱 4. 教室布局草稿命令

布局编辑器使用独立但语义一致的 `LayoutCommand` 信封：`action` 为 `apply`、`undo` 或 `redo`，并通过 `base_revision` 做乐观并发控制。批量操作在后端一次校验、一次提交、一步撤销；任一格越界、座位编号重复或移动目标冲突时，整个命令都不会修改草稿。

| 操作类型 (`kind`) | 载荷参数 (`payload`) | 功能描述 |
| :--- | :--- | :--- |
| `set_cell` | `row`, `column`, `kind`, 可选 `seat_id` | 修改单格类型。 |
| `set_cells` | `cells: [{row, column, kind, seat_id?}]` | 原子化批量修改所选区域。 |
| `insert_row` / `delete_row` | `index` | 插入或删除一排。 |
| `insert_column` / `delete_column` | `index` | 插入或删除一列。 |
| `translate` | `row_delta`, `column_delta` | 整体平移所有非空格。 |
| `translate_cells` | `cells: [{row, column}]`, `row_delta`, `column_delta` | 原子化移动所选非空格；禁止覆盖未选中的非空格。 |
| `mirror_horizontal` / `flip_vertical` | 空对象 | 整体左右镜像或上下翻转。 |

浏览器界面支持单击单选、Shift 单击矩形选择，以及 Ctrl/Command 单击增减选择。批量改类型和移动都发送一个命令，因此撤销不会拆成多步。

---

## 📖 相关文档

- [Web 与桌面工作台指南](web.zh.md)
- [系统架构解析](architecture.md)
