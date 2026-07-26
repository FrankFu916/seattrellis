# 编辑器协议

编辑器协议是 Streamlit、未来 React/SVG 编辑器和桌面端之间的传输边界。领域规则仍由
Python service 和 `EditingSession` 执行；前端只提交命令并渲染服务端返回的最小状态。

当前协议版本为 `"1.0"`，包含两个公开文档：

- `EditorCommandEnvelope`：前端发出的 apply、undo 或 redo 命令；
- `EditorStateEnvelope`：服务端返回的座位、锁定和 hard constraint 状态。

对应 JSON Schema 为 `schemas/editor-command.schema.json` 和
`schemas/editor-state.schema.json`。

## 命令格式

每个命令都必须显式携带类型、协议版本、命令 ID、草稿 ID 和基础 revision：

```json
{
  "kind": "seattrellis_editor_command",
  "protocol_version": "1.0",
  "command_id": "move-20260718-001",
  "draft_id": "7b7359c6f9cd4e128df8b9145d012ec1",
  "base_revision": 3,
  "action": "apply",
  "operations": [
    {
      "kind": "swap_students",
      "payload": {
        "first_student": "S001",
        "second_student": "S018"
      }
    }
  ]
}
```

`action="undo"` 或 `"redo"` 时不得包含 operations：

```json
{
  "kind": "seattrellis_editor_command",
  "protocol_version": "1.0",
  "command_id": "undo-20260718-001",
  "draft_id": "7b7359c6f9cd4e128df8b9145d012ec1",
  "base_revision": 4,
  "action": "undo"
}
```

支持的 operation 如下：

| kind | payload |
|---|---|
| `swap_students` | `first_student`, `second_student` |
| `move_student` | `student_key`, `seat_id` |
| `batch_move` | `moves: [{student_key, seat_id}]` |
| `seat_student` | `student_key`, `seat_id` |
| `unseat_student` | `student_key` |
| `lock_student` / `unlock_student` | `student_key` |
| `lock_seat` / `unlock_seat` | `seat_id` |

一个命令最多展开为 100 项操作。`batch_move` 中的每个映射计为一项，学生和目标座位
必须各自唯一。整个命令先完成验证和重放，再写入草稿文件；任一操作失败都不会提交
部分结果。

## Revision 与冲突

新草稿使用不可复用的 `draft_id`，revision 从 0 开始。每个成功的 apply、undo 或
redo 命令只把 revision 增加 1，即使 apply 内含多个 operation。撤销和重做以整个
命令批次为单位。

服务端在写入前依次检查：

1. `draft_id` 是否属于当前草稿；
2. `command_id` 是否已经处理；
3. `base_revision` 是否等于当前 revision。

任一检查失败都会抛出 `EditorProtocolConflictError`，不修改草稿或输出文件。客户端
收到冲突后应重新读取最新 `EditorStateEnvelope`，再根据用户意图构造新命令；不要
悄悄覆盖新状态。

## 状态格式

状态只提供编辑器实际需要的数据：

- 学生 key、显示名称、当前座位和锁定状态；
- 座位 key、行列、启用状态、当前学生 key 和锁定状态；
- undo/redo 深度；
- hard constraint 是否满足、检查数量和纯文本诊断。

成绩、备注、特殊需求、身高、视力、标签和任意扩展属性不会进入状态协议。座位不再
重复携带学生姓名，客户端应通过 `student_key` 关联学生列表。

这份状态是数据最小化结果，不是匿名数据。姓名、稳定 student key 和约束诊断仍可能
识别学生。不得把状态或命令写入远程遥测和公开日志；诊断字符串应按不可信纯文本转义，
也不能作为稳定的机器可读错误码。

`draft_id` 只用于并发检查，不是授权令牌。如果未来通过 HTTP 或 WebSocket 暴露协议，
还必须验证会话所有权，并提供 CSRF、Origin 和访问控制保护。

## 校验边界

JSON Schema 可验证字段类型、必填项、operation 结构和基础数量限制。以下跨字段或
领域约束仍以服务端 Pydantic 模型和 `EditingSession` 为准：

- apply 必须含 operation，undo/redo 不得含 operation；
- 展开后的操作总数不得超过 100；
- batch source/target 不得重复；
- 学生、座位、锁和占用关系必须在当前草稿中有效。

客户端可以先用 Schema 提供即时提示，但不能跳过服务端校验。
