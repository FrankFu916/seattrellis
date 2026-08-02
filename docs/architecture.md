# 架构

SeatTrellis 按职责分为两条共享契约的运行时路径：Python 兼容路径和 Rust-first
桌面路径。两者不在 UI 中复制规则，而是通过版本化 JSON 和编辑命令协议对齐。

Python 兼容路径按职责分为四层：

1. **Models / I/O**：Pydantic 数据模型、CSV/Excel/JSON 读取与校验；
2. **Solver / Scoring**：fallback 与可选 OR-Tools 求解、历史统计和候选评分；
3. **Service**：稳定的输入输出 dataclass 和工作流编排；
4. **Adapters**：CLI、Streamlit Web 与 exporters。

CLI 和 Web 必须调用 service 层，不能复制规则合并、校验、求解或候选选择逻辑。Optional extras 使用延迟导入，保证最小安装无需加载重依赖。

Web 适配器进一步拆分为：`web/workflow.py` 提供不依赖 Streamlit 的文件和服务
适配；`web/editor_protocol.py` 连接版本化前端协议与编辑草稿；`web/components.py`
生成纯数据或 HTML；`web/interactive_panels.py` 只负责人工编辑、撤销/重做和局部
修复等有状态控件。`web/app.py` 负责页面导航、输入材料化和结果编排。状态型面板
通过显式回调获取 history 和错误展示能力，不反向导入页面模块。

交互座位图不直接修改 assignments。座位点击在
`web/interactive_panels.py` 中转换为 `move_student`、`swap_students` 或
`lock_seat`/`unlock_seat`，然后沿用 `WebEditingDraft` 的重放、撤销和持久化路径。
未来 SVG/React 画布应发送相同领域命令，不能建立第二套编辑状态机。

跨界面编辑使用版本化的 `EditorCommandEnvelope` 和 `EditorStateEnvelope`。
`WebEditingDraft` 为每份草稿分配独立 `draft_id` 和单调递增 revision；命令还带有
唯一 `command_id`。服务端会在写入前拒绝错误草稿、重复命令和旧 revision，并把
同一命令中的多项操作作为一个原子撤销批次。状态协议只包含姓名、学生/座位关联、
锁和约束诊断，不携带成绩、备注、特殊需求、身高或视力。传输模型位于
`seattrellis.editing_protocol`，Web 草稿适配位于
`seattrellis.web.editor_protocol`。具体格式见
[编辑器协议](editor-protocol.md)。

v1.4 开始，solver backend 共享 `CompiledProblem` 边界：输入模型先被解析成稳定的
学生/座位索引、启用座位、邻接边、hard rules 和候选排除关系，再交给独立的
fallback、OR-Tools 或实验 native backend 模块。`cp_sat.py` 仅保留为兼容入口和
调度层。这样可以先稳定领域语义，再逐步把验证、评分和启发式计算迁入 Rust。

## Rust-first desktop path

Rust 桌面路径复用同一套 React/TypeScript 工作台，但由 `app/` 的 loopback
服务器提供本地 API：

```text
React workbench → Rust App HTTP service → seattrellis_core → validation/solve/render
                                      ↘ local project and export I/O
```

`seattrellis_core` 和 App 使用版本化、粗粒度 JSON DTO；前端不会逐座位调用
底层求解器。App 构建时把 `src/seattrellis/web_static` 嵌入二进制，开发时仍可用
`SEATTRELLIS_WEB_STATIC` 覆盖资源目录。Tauri 只负责窗口生命周期和原生桌面集成，
不再承载第二套排座规则。

Rust 求解器当前是成本排序启发式实现，不等同于 Python OR-Tools CP-SAT；在
40/50/60 人基准、规则差分和候选质量验收完成前，Python OR-Tools 仍是兼容后端。
详细迁移阶段和发布门槛见 [Rust-first migration](rust-migration.md)。

所有文件操作默认发生在本机。桌面端复用 application/service 契约，而不是在 UI
中重新实现业务规则。
