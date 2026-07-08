# Python API

业务入口集中在 `seattrellis.service`，请求和返回类型定义在
`seattrellis.service_types`。

## 内存接口

这些函数接收已经加载好的模型，不读写文件：

- `compute_solve(SolveInput) -> SolveOutput`
- `compute_validate(ValidateInput) -> ValidateOutput`
- `compute_edit(EditInput) -> EditOutput`
- `compute_history_report(HistoryReportInput) -> HistoryReportOutput`
- `compute_pair_report(PairReportInput) -> PairReportOutput`
- `compute_project_info(ProjectInfoInput) -> ProjectInfoOutput`

## 文件接口

`solve`、`solve_with_report`、`edit_snapshot`、`run_validate`、
`run_history_report`、`run_pair_report` 和 `project_*` 函数接受文件路径，
供 CLI 和 Web 共用。
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

if not summary.satisfied:
    print(summary.violations)
```

编辑草稿允许临时出现未入座学生，但会拒绝重复座位、重复学生、未知学生和禁用座位。
每次成功操作都会返回 hard constraint 诊断；局部自动修复仍属于后续 solver/service
功能，不在编辑层直接实现。

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
`unseated_students` 和 `hard_constraints` 可直接用于界面状态和实时诊断。
文件接口 `edit_snapshot` 可直接读取普通 snapshot；如果输入是 candidate set，
默认选择 recommended candidate，也可以传入 `candidate_id` 指定候选。输出始终是
普通草稿 snapshot，并在 `metadata.manual_edit` 记录本次操作摘要。

Web 页面调用 `seattrellis.web.workflow`。这个模块不依赖 Streamlit，可以
单独测试。

读取 snapshot、candidate set 或 project 时应检查 `schema_version`。
以下划线开头的函数属于内部实现，不在兼容承诺内。字段定义见
[输入格式](input-format.zh.md)，版本策略见[版本与兼容](versioning.md)。
