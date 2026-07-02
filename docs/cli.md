# CLI 命令参考

运行 `seattrellis --help` 查看当前安装版本的完整参数。

| 命令 | 用途 |
|------|------|
| `init-demo` | 创建虚构示例文件 |
| `presets list/show/export` | 查看或导出内置规则 preset |
| `validate` | 校验输入、字段和明显规则冲突 |
| `solve` | 生成 snapshot 或 candidate set |
| `export` | 导出选定方案 |
| `history-report` | 汇总学生座位类别历史 |
| `pair-report` | 汇总同桌和邻座历史 |
| `project-init/info/validate/solve/export` | 管理本地 project 工作流 |
| `doctor` | 检查 Python、optional extras 和示例文件 |

## 退出状态

- `0`：命令成功；
- 非 `0`：输入、依赖、校验、求解或导出失败。

命令默认使用 deterministic fallback solver。安装 `solver` extra 并设置 `SEATTRELLIS_USE_ORTOOLS=1` 后才启用 OR-Tools。

更多完整示例见 [快速开始](quickstart.zh.md)。

