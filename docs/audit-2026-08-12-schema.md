# Schema 契约链一致性审计（2026-08-12）

审计范围：`crates/seattrellis-schema/`（v2 工件注册表）→ xtask 生成链（`schemas/*.v2.schema.json`、
OpenAPI、generated.ts）→ `clients/web/src/api/types.ts` 三方对照；契约版本
（api_version / EDITOR_PROTOCOL_VERSION）；v1→v2 migration round-trip。
审计人基于代码 + 现有测试完成，未改动被其他 agent 占用的目录（cli/export/repair.rs/scripts/fixtures）。

## 1. 生成链说明

```
crates/seattrellis-schema/src/dto/*.rs   (schemars JsonSchema derive, deny_unknown_fields)
        │  cargo run -p xtask -- contract schemas     (xtask/src/main.rs::schema_artifacts)
        ▼
schemas/{student-roster,classroom-layout,ruleset,snapshot,project,
        project-bundle-manifest,candidate-set,plan-comparison-report}.v2.schema.json   (8 个)
        均为 ArtifactEnvelope<T> 全文档 schema（kind/schema_version/data/extensions + $defs）

xtask/src/spec.rs  ──contract openapi──▶ docs/api-v1-openapi.json
xtask/src/main.rs::interfaces()+endpoints() ──contract ts──▶ clients/web/src/api/generated.ts
seattrellis-rules::rule_registry_json() ──contract rules──▶ clients/web/src/api/ruleRegistry.generated.ts
`xtask contract check` 对上述全部产物做字节级 drift 校验（CI rust.yml 已接线）。
```

要点：

- **8/12 注册表种类有生成 schema**：`registry.rs` 的 `ArtifactKind` 声明 12 种、`V2_ARTIFACT_VERSION = 2`；
  xtask `SCHEMA_ARTIFACTS` 只覆盖 8 种。RotationPlan / HistoryArchive / EditingOperationLog / ExportPreset
  无 DTO、无生成 schema（`check_version` 会接受它们的 version=2 envelope，但 `data` 只能是不透明 JSON）。
  ledger §19.18 已登记“共 8 个”，属于已知状态。
- **设计形态**：v2 envelope（`kind` 为 ArtifactKind 的 snake_case 拼写，`schema_version=2`）内嵌
  **v1 版本号的 payload**（project/ruleset 的 `data.schema_version=1`，snapshot/candidate-set/
  plan-comparison 的 `data.schema_version="0.2.2"` 字符串）；payload 级 `kind`（如
  `seattrellis_project`、`plan_comparison_report`）与 envelope 级 `ArtifactKind` 拼写（`project`、
  `plan_comparison`）不同，是 v1 模型镜像设计，非 bug。
- **生成 schema 与 Rust DTO 一致性**：直接由 DTO derive，`xtask contract check` 通过
  （“contract artifacts are up to date”），并有 metaschema 校验 + unknown-field 拒绝测试，一致性可靠。
- **generated.ts 的手工接口与 spec.rs 之间没有自动一致性校验**：`contract check` 只比较
  “生成器输出 vs 已提交文件”，而 `interfaces()`（main.rs）与 `schemas()`（spec.rs）是同一文件里
  的两套手工清单，二者互相不一致不会被检出（见 M1/M2）。

## 2. 抽查 DTO 三方对照结论（Rust serde / 生成 schema / types.ts）

| DTO | Rust 定义 | 生成 schema / spec.rs | types.ts | 结论 |
|---|---|---|---|---|
| ArtifactEnvelope | `{kind, schema_version: u32, data, extensions?}` 严格解析 | 生成 schema 一致（envelope 包装 + `$defs`） | n/a（前端不用） | 一致 |
| StudentRoster / RosterStudent | snake_case 字段（student_id/name/height_cm/…） | 一致（`type: [string,null]` 等） | `Student` 用 camelCase（`id`/`heightCm`），为**展示层别名**；`RosterDraftResponse` 用 snake_case 与 Rust 逐字段一致 | 一致 |
| EditorState | domain/editing.rs `{kind, protocol_version, draft_id, revision, candidate_id?, undo_depth, redo_depth, students, seats}`，seat 含 row/col/enabled/student_key/locked | spec.rs：字段全，但 `candidate_id` 非 required、`seat_id` 非 nullable、seats items 无字段（宽松文档） | 与 Rust 逐字段一致（`candidate_id: string \| null`） | 一致（spec 宽松，见 L3） |
| EditorCommandEnvelope | 与 types.ts `EditorCommand` 一致；action=apply/undo/redo | 一致 | 一致 | 一致 |
| RosterDraftResponse | io/roster.rs 逐字段一致（draft_id/source_format/…/mapping_issues） | spec.rs 有 schema，字段全 | 一致 | 一致 |
| GenerateClassResponse | server 恒发 `goal{goal_id,title,description,preset_name}` + solved/unsolved 两态 | spec.rs 两态字段齐全（含 goal） | 缺 `goal` 字段（已修，L2） | 修复后一致 |
| RotationPlan | application/rotation.rs 生成 `{schema_version:"0.2.2", kind:"rotation_plan", name, periods, base_history_count, fairness_summary, pair_repeat_summary, warnings, metadata}` | 无 v2 schema（v1 `rotation-plan.schema.json`） | 与 Rust 一致（metadata 恒发、created_at 恒不发，types.ts 均 optional） | 一致（无 v2 schema，见 M3/信息项） |
| ExportDraftRequest | 服务端读 `privacy` 为**布尔对象**（hide_scores 等 6 键），另接受 paper_size/margin_mm | spec.rs/generated.ts 写 `privacy?: string` **错误** | `privacy: ExportPrivacyOptions` 正确；缺 paper_size/margin_mm | **spec/generated 与实现不一致（M1）** |
| ProjectHistoryResponse | io/projects.rs `ProjectHistory{api_version, project_name, project_path, history, outputs, warnings}`；artifact 不含 operation_history（Rust 侧不序列化） | spec.rs 无组件定义（宽松） | 一致（operation_history 为 optional） | 一致 |
| DraftAuditReport | application/draft_audit.rs `{api_version:"1", draft_id, feasible, score, audit}` | 无定义 | `api_version: string` 等一致 | 一致 |
| RosterUpdatePreviewRequest | io/roster.rs 与 types.ts 一致；`Student` 反序列化接受 camelCase 别名（id/heightCm） | spec.rs 有 | 一致 | 一致 |

版本契约核对：

- **EDITOR_PROTOCOL_VERSION**：Rust `editing.rs:41 = "1.0"`，前端 `client.ts:45 = "1.0"`，
  v1 oracle `schemas/editor-state.schema.json` `protocol_version const "1.0"` —— 三方一致。
- **api_version**：v1 端点统一字符串 `"1"`（health/catalogs/projects/audit 等，server 测试与实现一致）；
  `SessionResponse.api_version = 1`（整数）；CoreSolveRequest/Response `api_version = 2`（整数）。
  spec.rs/generated.ts 与实现一致。types.ts 不声明 Session/CoreSolve 类型，无冲突。
- **工件版本**：注册表 `V2_ARTIFACT_VERSION = 2` 与 envelope `ArtifactEnvelope::new` 的 `schema_version: 2`
  一致；`check_version` 拒绝未来版本有测试覆盖。

## 3. 问题清单（按严重度）

### 高

- **H1（已修复）seattrellis-schema 测试在 MSRV 1.88 无法编译**
  - 位置：`crates/seattrellis-schema/tests/property_migration.rs:99,127`
    （自 `07da946` 引入，从未在 1.88 编译过）。
  - 差异：`prop_assert_eq!(target.get("kind"), Some(&json!("student_roster")))` 在 1.88 触发
    E0716（temporary value dropped while borrowed）；stable 可编译。`cargo +1.88.0 test -p seattrellis-schema`
    直接失败，与 AGENTS.md 的 MSRV 声明冲突。
  - 根因：CI（`.github/workflows/rust.yml`）只跑 `-p seattrellis_core / seattrellis_cli / seattrellis_app`，
    **未跑 seattrellis-schema 测试**，MSRV job 亦然。
  - 修复：改为 `target.get("kind").and_then(Value::as_str)` 与字符串字面量比较（两处）。
  - 建议：rust.yml 增加 `cargo test --locked -p seattrellis-schema`（至少 MSRV job）。

### 中（drift 校验内的契约与实现不一致——`contract check` 检不出，因为错误同时存在于 spec 与生成物）

- **M1 ExportDraftRequest.privacy 类型错误（string vs object）**
  - 位置：`xtask/src/spec.rs:682`、`xtask/src/main.rs` interfaces()（generated.ts `privacy?: string`）。
  - 差异：实现 `crates/seattrellis-application/src/export.rs:279-284` 把 `privacy` 当作
    布尔对象读取（`hide_scores/hide_notes/hide_special_needs/anonymize/show_height/show_vision`）；
    spec/生成物写成 `type: string`。types.ts `ExportPrivacyOptions` 才是正确形状。
  - 建议：spec.rs 与 interfaces() 改为 object 形状（与 types.ts 对齐），重新生成
    `docs/api-v1-openapi.json` / `generated.ts`；顺带补 `paper_size`、`margin_mm` 两个真实字段。
- **M2 GenerateClassRequest.goal：spec.rs 的 `custom` 字段不存在**
  - 位置：`xtask/src/spec.rs:524`。
  - 差异：实现读 `draft.goal.custom_rules`（`class_generation.rs:292-298`），types.ts 也是
    `custom_rules`；spec.rs 写的是 `custom`。另外 spec.rs 的 draft 缺 `name`
    （types.ts 必填，服务端用于 class_name）。
  - 建议：spec.rs 修正为 `custom_rules` 并补 `draft.name`。
- **M3 注册表 12 种 vs 生成 schema 8 种 vs CLI schema-export 口径不一**
  - 位置：`crates/seattrellis-schema/src/registry.rs`（12 种）、`xtask/src/main.rs:41-53`（8 种）、
    `crates/seattrellis-cli/src/commands.rs:824-853`（schema-export 映射）。
  - 差异：`schema-list` 声称 12 种全部 v2；`schema-export` 对 `candidate_set` 输出
    **v1** 文件 `schemas/candidate-set.schema.json`（`.v2.` 文件明明存在），对 `rotation_plan`
    输出 v1 `rotation-plan.schema.json`；`plan_comparison / history_archive /
    editing_operation_log / export_preset` 直接报错“no v2 JSON Schema embedded”。
  - 建议：`candidate_set` 改指 `candidate-set.v2.schema.json`；4 个无 DTO 的 kind 在
    schema-export 中明确报“无 typed DTO（M2-03 未覆盖）”，或补 DTO/生成 schema。
    （CLI 由其他 agent 修改中，本次仅报告。）

### 低（文档性缺口，无运行时破坏）

- **L1（已修复）types.ts HealthResponse 过时**：原 `{status:"ok", version?: string}`，服务端
  实际发 `{status, service, api_version:"1"}`，`version` 从来不存在。已改为与服务端一致并同步
  `demo.ts`（`service: "seattrellis-demo"`）。generated.ts/spec.rs 原本就正确。
- **L2（已修复）types.ts GenerateClass 响应缺 `goal`**：服务端 solved/unsolved 恒发
  `goal{goal_id,title,description,preset_name}`，spec.rs 也要求；types.ts 两种响应都缺该字段。
  已新增 `GenerateClassGoal` 并加入两个响应类型（纯增量，无消费方破坏）。
- **L3 spec.rs/generated.ts EditorState 精度不足**：`candidate_id` 应为 `string | null`
  （Rust `Option<String>` 序列化为 null；generated.ts 写 `candidate_id?: string`）；
  students[].seat_id 未标 nullable；seats items 无字段定义
  （实际 `{seat_id,row,col,enabled,student_key,locked}`）。与 spec.rs 顶部
  “complex documents are additionalProperties: true”的已知宽松策略一致，M6 typed DTO 收敛时修正。
- **L4 服务端额外字段未进 types.ts**：rotation solved 响应恒发 `period_editors`（types.ts 为
  optional，兼容）；editor command 响应会附加 `validation` 对象；ProjectPrivacyResponse 有
  `verdict` 字段未声明。均为“多余字段被 TS 忽略”，无破坏；建议 M6 收敛时补齐。
- **L5 RotationPlan 仍是 v1 工件契约**：服务端写 `schema_version:"0.2.2"` + `kind:"rotation_plan"`
  （v1 oracle 形态），注册表却把 RotationPlan 登记为 v2 且无 typed DTO/schema。
  当前 types.ts 与服务端一致，功能上闭环；属 M2-03 未覆盖的已知项，建议在 ledger 登记为
  RUST_PARTIAL 的明确原因。

### 信息

- v2 plan-comparison envelope 的 `kind = "plan_comparison"`，而 payload 必填
  `data.kind`（v1 值为 `plan_comparison_report`）；生成 schema 对 `data.kind` 只约束为 string，
  不校验与 envelope kind 的一致性。v1 语义靠镜像保留，无交叉校验（可接受，但值得在
  migration golden 中固化样例）。
- `schema-migrate` 要求输入文档含 `kind` 字段，而裸 v1 roster/layout 文档没有 kind
  （v1 schema 无此字段）——CLI 当前对裸 v1 文档可能报“artifact is missing a 'kind' field”。
  由 CLI agent 确认是否有包装约定。（观察项，未验证完整 CLI 行为。）

## 4. 已修复项汇总

| 文件 | 修改 | 验证 |
|---|---|---|
| `crates/seattrellis-schema/tests/property_migration.rs` | 两处 `Some(&json!(...))` 改 `and_then(Value::as_str)` 字符串比较，修复 1.88 E0716 | `cargo +1.88.0 test -p seattrellis-schema`：56 通过（45 lib + 2 envelope 相关 + 6 fuzz + 3 proptest） |
| `clients/web/src/api/types.ts` | HealthResponse 与服务端一致；GenerateClass 两种响应补 `goal: GenerateClassGoal` | `npx tsc -b` 通过；`npm test` 29 文件 152 测试全绿 |
| `clients/web/src/api/demo.ts` | demoBootstrap.health 补 `service`/`api_version` | 同上 |

## 5. 测试结果

- `cargo +1.88.0 test -p seattrellis-schema`：56/56 通过（修复 H1 后）；
  含 property_migration 3 门（migrate→validate、round-trip、幂等、字段保全）——
  **v1→v2 migration 语义保持有测试背书**（roster/layout 无损，未知 v1 字段阻断迁移，
  未注册 kind 显式报错）。
- `cargo +1.88.0 run -p xtask -- contract check`：通过，8 个 v2 schema + openapi + generated.ts
  + ruleRegistry 无 drift。
- `cd clients/web && npx tsc -b`：通过；`npm test`：29 文件 / 152 测试通过。
- 未运行：`-p seattrellis-cli`（其他 agent 修改中）、`-p seattrellis-export`（范围外）。

## 6. 建议

1. CI rust.yml 补 `cargo test --locked -p seattrellis-schema`（MSRV job 必跑）——H1 的唯一防线。
2. M1/M2 的 spec.rs/生成物修正应尽快做（改动小、drift 校验可自动兜底）；M3 的 CLI 映射与
   4 个无 DTO kind 的处置需与 CLI agent 对齐后落地。
3. L3–L5 随 M6 typed-DTO 收敛一并处理；L5 建议在 parity ledger 登记 RotationPlan
   “v2 无 typed DTO”为 RUST_PARTIAL 的明确原因。
