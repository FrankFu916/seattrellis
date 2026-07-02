# 架构

SeatTrellis 按职责分为四层：

1. **Models / I/O**：Pydantic 数据模型、CSV/Excel/JSON 读取与校验；
2. **Solver / Scoring**：fallback 与可选 OR-Tools 求解、历史统计和候选评分；
3. **Service**：稳定的输入输出 dataclass 和工作流编排；
4. **Adapters**：CLI、Streamlit Web 与 exporters。

CLI 和 Web 必须调用 service 层，不能复制规则合并、校验、求解或候选选择逻辑。Optional extras 使用延迟导入，保证最小安装无需加载重依赖。

所有文件操作默认发生在本机。未来桌面端应继续复用 service API，而不是在 UI 中重新实现业务规则。

