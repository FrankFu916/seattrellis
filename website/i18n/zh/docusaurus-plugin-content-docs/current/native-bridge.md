# 进程内原生桥接

工作区中的 `seattrellis-native-bridge` 通过 C ABI 向原生客户端提供共享应用层。
它生成名为 `seattrellis_bridge` 的静态库、动态库与 Rust 库，macOS SwiftUI
预览版是首个调用方。这是早期集成接口；iOS、Windows、Android、Qt 与 GTK
客户端仍需各自构建和目标平台测试。

```sh
cargo build --release --locked -p seattrellis-native-bridge
```

权威头文件是
[`bindings/include/seattrellis.h`](https://github.com/FrankFu916/seattrellis/blob/main/bindings/include/seattrellis.h)。
Windows 静态链接定义 `SEATTRELLIS_STATIC`，动态导入使用 `cdecl`。
`size_t` 与缓冲区布局跟随目标架构。

## 会话、线程与所有权

每个非零、不透明的会话 ID 独立保存编辑草稿和完整求解来源，ID 不复用。
dispatch 是同步调用，应放在客户端后台线程。同一会话的重叠调用返回
`session_busy`，不同会话可并行。取消和销毁不等待应用层或存储锁，每次调用
使用新的取消控制。调用方还须取消尚未开始的排队任务；取消空闲会话不会取消
下一次调用。

| 函数 | 约定 |
| --- | --- |
| `seattrellis_abi_version()` | 返回 ABI 主版本 1。 |
| `seattrellis_session_create()` | 创建会话；资源或配额不足返回零。 |
| `seattrellis_session_dispatch(id, input, len)` | 返回库拥有的 UTF-8 JSON，不含末尾 NUL。 |
| `seattrellis_session_cancel(id)` | 活动取消请求返回 1，空闲返回 0，无效 ID 返回 −1。 |
| `seattrellis_session_destroy(id)` | 移除会话并取消活动调用；正在执行的调用自行持有生命期直到返回。 |
| `seattrellis_buffer_free(buffer)` | 调用方复制字节后，释放返回的缓冲区。 |

每个响应都必须释放，包括错误响应；使用原样的指针和长度，不修改内容，也不
使用平台分配器释放。销毁会话不会释放已经返回的响应。`{NULL, 0}` 表示响应
分配或配额不足。

输入必须来自同一块存活、已初始化、可读、在整个调用期间不可变的内存。
空指针和过长输入会在读取前拒绝，但 C ABI 无法判断任意非空外部地址是否合法。
悬空指针、并发修改以及释放后读取都违反调用约定。可恢复 Rust panic 会在 ABI
边界内捕获；内存分配耗尽等进程 abort 不会转成可恢复响应。

## 有界协议

进程最多有 16 个活动会话，每个请求最多 8 MiB，每个响应最多 32 MiB，最多
256 个未释放响应，未释放响应总量最多 128 MiB。执行操作前预留一个响应名额和
32 MiB 响应容量，返回后按真实字节记账。因此剩余空间不足 32 MiB 时可能提前
拒绝；没有未释放响应时，最多四次 dispatch 同时运行。配额不足不会执行操作。
共享应用层还限制草稿上下文
容量。这些上限独立于 HTTP 上限；客户端应在替换当前文档前报告大小错误。

```json
{
  "protocol_version": 1,
  "operation": "state",
  "payload": { "draft_id": "a-session-owned-draft-id" }
}
```

成功响应含 `protocol_version`、`ok: true` 和 `result`；失败响应含 `ok: false`
以及具有 `code`、`message`、数字 `status` 的 `error`。普通求解结果保留共享的
七种状态，传输错误和取消也可能使用错误信封。ABI 主版本、JSON 协议、编辑协议
和保存文档版本分别管理。

| 操作 | 载荷与结果 |
| --- | --- |
| `generate` | 现有工作台或核心请求；返回状态、可行性、编辑状态和候选信息。 |
| `state` | `draft_id`；返回编辑状态和独立合法性校验。 |
| `command` | 现有编辑信封，协议为 `"1.0"`，包含 `base_revision`、动作与操作。 |
| `serialize` | 完整 `class_source` 及包含 ID/修订的 `draft_refs`；返回现有可携班级文档。 |
| `open` | 现有班级文档；恢复完整上下文和已保存的锁。 |
| `repair` | `draft_id`、`base_revision`，可选 `affected_students`；复用共享修复逻辑。 |
| `audit` | `draft_id`；返回共享审计报告。 |
| `export` | `draft_id`、`format`，可选 `options`；返回文件名、MIME、base64 字节与警告。 |
| `delete` | `draft_id`；同时移除编辑草稿和对应求解来源。 |

桥接复用应用层和领域层，不另写一套评分算法。原生导出使用明确选项和内置默认值，
不读取或修改全局导出偏好。文件选择、沙盒授权和写入由平台负责，完整来源始终与
最小化的编辑投影分开保存。

## 验证与分发

Rust 集成测试通过导出的 ABI 验证生成、非法编辑、保留锁的保存重开、审计、
修复、导出和删除，并检查会话隔离、输入上限和缓冲区所有权；还有外部语言调用
检查。macOS CI 编译 SwiftUI 客户端、运行文档测试并生成未签名预览包。未签名
CI 构建产物不等于已经签名、公证的发行版，也不等于人工无障碍验收。
