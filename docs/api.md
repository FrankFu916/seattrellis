# Python API

稳定入口位于 `seattrellis.service` 和 `seattrellis.service_types`。

## 主要工作流

- `solve_service(SolveInput) -> SolveOutput`
- `validate_service(ValidateInput) -> ValidateOutput`
- `history_report_service(HistoryReportInput) -> HistoryReportOutput`
- `pair_report_service(PairReportInput) -> PairReportOutput`
- `project_info_service(ProjectInfoInput) -> ProjectInfoOutput`

导出统一通过 `seattrellis.exporters.export_snapshot`。Web helper 位于 `seattrellis.web.workflow`，且不导入 Streamlit。

模型可用于读取和生成 snapshot、candidate set、project 与 rules。调用方必须保留并检查 `schema_version`；不应依赖以下划线开头的内部函数。

完整字段定义见[输入格式](input-format.zh.md)，兼容承诺见[版本策略](versioning.md)。

