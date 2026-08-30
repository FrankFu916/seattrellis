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

---

## 📖 相关文档

- [Web 与桌面工作台指南](web.zh.md)
- [系统架构解析](architecture.md)
