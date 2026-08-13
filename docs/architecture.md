# 架构

SeatTrellis v2 是纯 Rust 工程。领域逻辑按分层 crate 组织，React 只是展示层；
规则编译、合法性、编辑状态机、migration、隐私和求解状态只由 Rust 决定，前端
不得复制领域规则。

## 分层结构

| crate | 职责 |
|---|---|
| `seattrellis-schema` | 版本化 JSON 契约与产物注册表 |
| `seattrellis-rules` | 规则 DSL / registry（goal rules、preset 场景） |
| `seattrellis-domain` | 编辑状态机、layout 草稿、轮换与分组领域模型 |
| `seattrellis-application` | 用例编排（生成、导出请求、草稿审计） |
| `seattrellis-io` | roster CSV/Excel 导入、migration、rotation、roster 草稿 |
| `seattrellis-export` | 八种格式渲染器（svg/html/print-html/png/pdf/xlsx/docx/pptx） |
| `seattrellis-server` | loopback HTTP 传输层（axum），嵌入 React 工作台资源 |
| `seattrellis-core` | 求解器宿主：hard search、local search、candidates、audit、evaluator、validator |
| `seattrellis-cli` | 28 个子命令的 CLI 适配器 |

`app/` 是薄 facade（`seattrellis_app`），复用 `seattrellis-server` 启动本地
服务；`app/src-tauri/` 是 Tauri 2 壳，只负责窗口生命周期，不承载第二套排座规则。

## 运行时形态

```text
React 工作台（clients/web）
          │
seattrellis_app（loopback HTTP，127.0.0.1:8765）/ Tauri 2 壳
          │
seattrellis-server → seattrellis-application → seattrellis-core（求解/校验/审计）
          │
本地 project 与 export I/O（seattrellis-io / seattrellis-export）
```

`seattrellis_core` 与 App 使用版本化、粗粒度的 JSON DTO（`CoreSolveRequest` /
`CoreSolveResponse`）；前端不会逐座位调用底层求解器。App 构建时把 React 工作台
的生产资源嵌入二进制，开发时可用 `SEATTRELLIS_WEB_STATIC` 覆盖资源目录。

## 跨界面编辑协议

跨界面编辑使用版本化的 `EditorCommandEnvelope` 和 `EditorStateEnvelope`
（`protocol_version: "1.0"`，实现在 `seattrellis-domain::editing`）。每份草稿
有独立 `draft_id` 和单调递增 revision；命令还带有唯一 `command_id`。服务端会
在写入前拒绝错误草稿、重复命令和旧 revision，并把同一命令中的多项操作作为
一个原子撤销批次。状态协议只包含姓名、学生/座位关联、锁和约束诊断，不携带
成绩、备注、特殊需求、身高或视力。具体格式见[编辑器协议](editor-protocol.md)。

## 求解器

- hard 约束（fixed seats、must/cannot adjacency、min distance、groups）先做
  静态冲突校验，再进入候选域构建与匹配搜索；
- 求解状态词汇表冻结为 `Solved / ProvenInfeasible / Timeout / Unknown /
  InvalidInput / Cancelled / InternalError`；启发式耗尽只能是 `Unknown`，
  绝不伪装成 `ProvenInfeasible`，有合法 incumbent 时即使超时也是 `Solved`；
- 所有 solve/edit/repair/rotation/export 产物必须经独立 validator 复核，
  禁止硬编码 `feasible=true`。

## HTTP 与安全边界

`/api/*` 全部要求 `Authorization: Bearer <token>`（`/api/v1/session` 引导端点
除外）；Host 必须为 loopback 名 + 绑定端口（防 DNS rebinding）；Origin 存在时
必须同源（防 CSRF）；响应含 CSP / X-Frame-Options: DENY / Referrer-Policy:
no-referrer。token 由 Server 启动时生成（256-bit），Tauri 用
initialization_script 注入 `window.__SEATTRELLIS_SESSION__`，浏览器工作台经
`GET /api/v1/session` 引导获取。body 限制、并发上限和优雅停机在
`seattrellis-server` 中统一实施，新写路径不得绕过。

所有文件操作默认发生在本机，不包含遥测或云同步。v1（Python）行冻结在
1.9.0（`v1.x-maintenance` 分支），只作为行为基准的 oracle，不在 v2 树中。
