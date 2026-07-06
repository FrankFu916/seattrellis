# Python API

业务入口集中在 `seattrellis.service`，请求和返回类型定义在
`seattrellis.service_types`。

## 内存接口

这些函数接收已经加载好的模型，不读写文件：

- `compute_solve(SolveInput) -> SolveOutput`
- `compute_validate(ValidateInput) -> ValidateOutput`
- `compute_history_report(HistoryReportInput) -> HistoryReportOutput`
- `compute_pair_report(PairReportInput) -> PairReportOutput`
- `compute_project_info(ProjectInfoInput) -> ProjectInfoOutput`

## 文件接口

`solve`、`solve_with_report`、`run_validate`、`run_history_report`、
`run_pair_report` 和 `project_*` 函数接受文件路径，供 CLI 和 Web 共用。
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
视力。`candidate_scope="all"` 已保留给 v1.4 的候选集报告；当前单方案导出会
明确拒绝该值，不会静默只导出一个方案。

Web 页面调用 `seattrellis.web.workflow`。这个模块不依赖 Streamlit，可以
单独测试。

读取 snapshot、candidate set 或 project 时应检查 `schema_version`。
以下划线开头的函数属于内部实现，不在兼容承诺内。字段定义见
[输入格式](input-format.zh.md)，版本策略见[版本与兼容](versioning.md)。
