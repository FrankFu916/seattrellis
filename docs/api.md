# Python API

业务入口集中在 `seattrellis.service`，请求和返回类型定义在
`seattrellis.service_types`。

## 内存接口

这些函数接收已经加载好的模型，不读写文件：

- `compute_solve(SolveInput) -> SolveOutput`
- `compute_validate(ValidateInput) -> ValidateOutput`
- `compute_edit(EditInput) -> EditOutput`
- `compute_repair(RepairInput) -> RepairOutput`
- `compute_history_report(HistoryReportInput) -> HistoryReportOutput`
- `compute_pair_report(PairReportInput) -> PairReportOutput`
- `compute_rotation_plan(RotationInput) -> RotationOutput`
- `compute_project_info(ProjectInfoInput) -> ProjectInfoOutput`

## 文件接口

`solve`、`solve_with_report`、`edit_snapshot`、`repair_snapshot`、`run_validate`、
`run_history_report`、`run_pair_report` 和 `project_*` 函数接受文件路径，
供 CLI 和 Web 共用。

`generate_rotation_plan` 和 `project_rotate` 按顺序生成未来时段：上一时段的
snapshot 会加入下一时段的历史输入，因此已有的公平轮换和重复邻座规则会继续生效。
结果是带 `schema_version` 的 `RotationPlan`，每个 period 仍然是普通
`SeatingSnapshot`，可以直接编辑、导出或写入历史目录。

本地项目可用 `project_bundle.pack_project` 创建 `.seattrellis.zip`，用
`restore_project_bundle` 恢复。打包前的 `scan_project_privacy` 只返回文件名和
字段名，不返回学生数据；恢复会拒绝绝对路径、`..` 路径和符号链接，并在写入目标前
验证 manifest 和 project JSON。

本地 React 工作台使用同一套项目服务：

- `GET /api/v1/projects/recent?root=...&limit=...` 返回最近项目的名称、路径和修改时间；
- `POST /api/v1/projects/history` 返回历史/生成文件的元数据，不返回学生记录；
- `POST /api/v1/projects/artifacts/compare` 比较两个历史或输出文件，只返回隐私安全的
  方案摘要和结构变化数量；
- `POST /api/v1/projects/artifacts/restore` 从一个历史或输出文件创建新的输出 snapshot，
  不覆盖原始文件；
- `POST /api/v1/classes/rotation` 根据一个班级草稿生成多个未来 period，并返回第 1 期的
  编辑草稿；
- `POST /api/v1/projects/privacy` 执行分享前敏感字段检查；
- `POST /api/v1/projects/bundle` 下载 `.seattrellis.zip`；
- `POST /api/v1/projects/restore` 接收本地 bundle 路径或 multipart 上传并恢复到指定目录。

这些接口只绑定本机服务。上传恢复仍受项目包总大小、manifest、路径遍历和符号链接
校验限制；错误响应不会包含学生数据。
导出使用 `seattrellis.service.export` 或
`seattrellis.exporters.export_snapshot`。新的适配器应构造
`ExportRequest`，而不是自行组合模板和页面参数：

```python
from seattrellis.service import export
from seattrellis.service_types import ExportRequest, PageOptions

export(
    snapshot_path="outputs/candidates.json",
    request=ExportRequest(
        output_format="print-html",
        output_path="outputs/report.html",
        template="report",
        candidate_id="recommended",
        page=PageOptions(orientation="landscape", scale=0.9),
    ),
)
```

`PrivacyOptions.for_template("public")` 默认隐藏成绩、备注、特殊需求、身高和
视力。`candidate_scope="all"` 可把 candidate set 导出为完整候选比较 HTML
报告；snapshot 或非 HTML 格式会明确拒绝该值，不会静默只导出一个方案。

## 人工调整草稿

`seattrellis.editing` 提供 UI 无关的人工调整模型。Web、桌面端或未来的
React 编辑器应通过 `EditingSession` 执行交换、移动、移出座位、锁定和撤销/重做，
不要把这些规则散落在界面状态中：

```python
from seattrellis.editing import EditingSession

session = EditingSession.from_snapshot(snapshot)
session.lock_seat("R1C1")
summary = session.swap_students("S001", "S018")
session.batch_move({"S002": "R2C2", "S003": "R2C3"})

if not summary.satisfied:
    print(summary.violations)
```

编辑草稿允许临时出现未入座学生，但会拒绝重复座位、重复学生、未知学生和禁用座位。
每次成功操作都会返回 hard constraint 诊断；当前锁状态通过 `EditingLockState` 和
`metadata.lock_state` 在内存和文件工作流中保持一致。局部自动修复由 service 层完成，
不在编辑层直接实现。

`batch_move` 会先验证全部映射再一次更新 assignments。学生和目标座位必须唯一；
占用目标座位的学生必须也参与批次，因此可表达循环换位，但不会隐式移出第三方。
任一锁定、未知或冲突映射都会拒绝整个命令，undo/redo 也把它视作单条操作。

适配器如果已经把 UI 操作整理成命令序列，可以直接调用服务层：

```python
from seattrellis.editing import EditingOperation
from seattrellis.service import compute_edit
from seattrellis.service_types import EditInput

result = compute_edit(
    EditInput(
        snapshot=snapshot,
        operations=[
            EditingOperation(
                kind="swap_students",
                payload={"first_student": "S001", "second_student": "S018"},
            ),
            EditingOperation(kind="lock_seat", payload={"seat_id": "R1C1"}),
        ],
    )
)
```

`EditOutput.snapshot` 是新的草稿 snapshot；`locked_students`、`locked_seats`、
`unseated_students`、`lock_state` 和 `hard_constraints` 可直接用于界面状态和实时诊断。
文件接口 `edit_snapshot` 可直接读取普通 snapshot；如果输入是 candidate set，
默认选择 recommended candidate，也可以传入 `candidate_id` 指定候选。输出始终是
普通草稿 snapshot，并在 `metadata.manual_edit` 记录本次操作摘要。

需要锁定后重排时，调用 `compute_repair(RepairInput)` 或 `repair_snapshot`。如果提供
`affected_students`，范围会自动加入与这些学生存在 hard rule 或当前座位相邻关系的
一阶学生，其余当前已入座学生会临时固定在原座；锁定的空座会临时从求解可用座位中
移除。`RepairInput` 可接收 `EditOutput.lock_state` 和历史 snapshots，
保证纯内存 UI、CLI 和 Project 工作流具有相同锁定和公平性语义。原始 `RuleSet` 不会被
修改，请求范围、有效范围、临时固定关系、有效固定关系和输出差异写入
`metadata.repair`，供 UI 展示和审计。

Web 页面调用 `seattrellis.web.workflow`。这个模块不依赖 Streamlit，可以
单独测试。`seattrellis.web.interactive_panels` 是 Streamlit 专用适配层，只消费
workflow API 和显式页面回调；领域规则、操作重放和 repair 语义不得放入该模块。

React/SVG 或桌面编辑器应使用版本化协议读取状态并提交命令：

```python
from seattrellis.editing_protocol import (
    EDITOR_PROTOCOL_VERSION,
    EditorCommandEnvelope,
)
from seattrellis.web.editor_protocol import (
    build_editor_state_for_web,
    dispatch_editor_command_for_web,
)

state = build_editor_state_for_web(draft)
command = EditorCommandEnvelope.parse_obj(
    {
        "kind": "seattrellis_editor_command",
        "protocol_version": EDITOR_PROTOCOL_VERSION,
        "command_id": "swap-001",
        "draft_id": state.draft_id,
        "base_revision": state.revision,
        "action": "apply",
        "operations": [
            {
                "kind": "swap_students",
                "payload": {
                    "first_student": "S001",
                    "second_student": "S018",
                },
            }
        ],
    }
)
draft = dispatch_editor_command_for_web(
    draft,
    command,
    output_dir="outputs/editor",
)
```

一个 envelope 内的 operations 会原子执行，并作为一个批次撤销。错误 `draft_id`、
旧 revision 或重复 `command_id` 会抛出 `EditorProtocolConflictError`；校验或执行
失败时不会写入部分结果。字段与并发语义见[编辑器协议](editor-protocol.md)。

读取 snapshot、candidate set 或 project 时应检查 `schema_version`。
以下划线开头的函数属于内部实现，不在兼容承诺内。字段定义见
[输入格式](input-format.zh.md)，版本策略见[版本与兼容](versioning.md)。
