# SeatTrellis v2 — Rust 后端审计（2026-08-12）

> 审计范围：crates/seattrellis-server（http.rs/server.rs）、
> seattrellis-io（transaction/projects/roster）、seattrellis-domain
> （editing）、seattrellis-application（export/class_generation）。
> 方法：源码阅读 + 边界输入推演；不改动 cli/export/web（并行审计中）。

## 结论摘要

安全与事务面**通过**；发现 2 个内存增长类**次要问题**（低风险，
登记 + 一处已修）。

## 通过项（含证据）

| 面 | 结论 | 依据 |
|---|---|---|
| M1-05 中间件 | ✅ | Host 校验对所有请求（含 session，DNS rebinding）；Origin 存在时必须同源（CSRF）；仅 `GET /api/v1/session` 豁免 Bearer（且跨域读不到响应体——无 CORS）；其余 /api/* 全部 Bearer + 常量时间比较（http.rs:142-203） |
| 可信根文件读取 | ✅ | `trusted_relative_path`（拒绝绝对/`..`/NUL/反斜杠/盘符）+ canonical 包含性（symlink 逃逸防御）+ 8MB 上限（server.rs:951-974） |
| 事务层 | ✅ | `begin_with_roots` canonical 根；stage 同目录 temp + `sync_all` + `sync_parent` + fingerprint；崩溃恢复（journal/revision）；11 个故障注入 rollback 测试（§17.2.4） |
| HTTP 边界 | ✅ | body 64MB + 并发 64（http.rs:32-36）；oversized 413 测试 |
| 导出文件名 | ✅ | 固定 `seat-plan.{ext}`，无用户输入 → 无 CRLF/注入面 |
| roster draft store | ✅ | TTL 2h + 容量 10（roster.rs:847-857） |
| 导出 draft 校验 | ✅ | export_draft 经 solve_requests 定位，缺失返回错误 |
| 静态文件 | ✅ | safe_join（percent-decode + `..` 拒绝 + canonical 包含） |

## 发现（次要，内存增长类）

### F1. EditorDraftStore / SolveRequestStore 无界（未修，登记）

- 位置：domain/editing.rs:822-824（`Mutex<HashMap>`）、server.rs:131
- 问题：每次 generate 创建 5 候选 draft（class_generation.rs:133）+ rotation
  每期 1 个；draft_id 每次唯一（时间戳+序列），永不移除。长期使用内存
  线性增长。
- 风险评估：单用户桌面场景（一天数十次生成）低风险；`long_run_gate`
  （500 次 solve，core/tests/long_run_gate.rs）RSS 稳定已覆盖；
  10000+ 次生成才会显著。
- 建议：alpha.2 或 M7 加容量上限（如最近 200 个 draft，LRU/TTL 清理）；
  需要产品决策（undo/候选的保留时长）。

### F2. undo 栈无上限（已修，commit 见工作区）

- 位置：domain/editing.rs:768（`draft.undo_stack.push(...)` 无截断）
- 问题：每次编辑操作 push 完整快照，长会话（数百次编辑）快照累积。
- 修复：上限 100 步（超出丢弃最旧）；`undo_depth` 字段语义不变
  （100 步内行为完全一致），前端契约不受影响。
- 测试：editing 契约测试 + 新增上限测试。

## 未发现

- 生产路径 panic 面：server/io/application 的 unwrap/expect 均在测试
  模块或静态数据（entropy/模板序列化），无用户输入驱动的 panic。
- 路径穿越、CRLF 注入、无界 body、跨源读取。
