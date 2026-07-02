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
导出使用 `seattrellis.exporters.export_snapshot`。

Web 页面调用 `seattrellis.web.workflow`。这个模块不依赖 Streamlit，可以
单独测试。

读取 snapshot、candidate set 或 project 时应检查 `schema_version`。
以下划线开头的函数属于内部实现，不在兼容承诺内。字段定义见
[输入格式](input-format.zh.md)，版本策略见[版本与兼容](versioning.md)。
