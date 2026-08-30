# 本地 REST API 接口完全参考

[English](api.md) · [简体中文](api.md)

**席序（SeatTrellis）v2.0.0** 通过本地服务（`seattrellis_web` / `seattrellis-server`）暴露了高性能、类型安全的 HTTP API。完整 OpenAPI 规范可参考 [`api-v1-openapi.json`](api-v1-openapi.json)。

---

## 🔒 1. 鉴权与安全访问规范

- **监听地址**：严格且默认仅监听 `127.0.0.1:8765` 本地回环地址；
- **令牌认证**：除 `GET /api/v1/session` 引导端点外，所有 API 必须携带请求头 `Authorization: Bearer <session_token>`；
- **请求头拦截**：严格校验 `Host: 127.0.0.1:*` 与 `Same-Origin`，阻止 DNS Rebinding 与 CSRF 攻击。

---

## 📡 2. 核心 API 端点一览

### 2.1 求解与算法接口

| 方法与路径 | 功能描述 | 请求与响应特点 |
| :--- | :--- | :--- |
| **`POST /api/v2/solve`** | **核心无副作用求解** | 接收 `CoreSolveRequest`，输出 `Solved`、`ProvenInfeasible`、`Timeout` 或 `Unknown` 状态及完整快照。 |
| **`POST /api/v1/classes/generate`** | **工作台生成接口** | 支持直接从花名册草稿、教室模板与偏好目标一键求解并创建编辑草稿。 |
| **`POST /api/v1/classes/rotation`** | **多期轮换求解** | 接收期数参数，生成多期连续排座序列及同桌重复统计。 |

---

### 2.2 花名册与教室草稿接口

| 方法与路径 | 功能描述 |
| :--- | :--- |
| `POST /api/v1/rosters/drafts` | 上传 CSV/Excel 花名册并创建解析草稿。 |
| `GET /api/v1/rosters/drafts/{id}` | 获取花名册解析结果与字段映射状态。 |
| `POST /api/v1/layouts/drafts` | 从排数列数或预设模板创建教室布局草稿。 |
| `POST /api/v1/layouts/drafts/{id}/commands` | 对教室布局应用增删座位、设置走廊等指令。 |

---

### 2.3 交互式编辑与微调接口

| 方法与路径 | 功能描述 |
| :--- | :--- |
| `GET /api/v1/editing/drafts/{id}` | 获取当前排座草稿的实时座次分布与锁定状态。 |
| `POST /api/v1/editing/drafts/{id}/commands` | 提交微调操作（互换/移动/锁定/撤销/重做），返回版本号与硬约束复核诊断。 |
| `GET /api/v1/editing/drafts/{id}/audit` | 对当前草稿执行多维度雷达图打分审计。 |

---

### 2.4 多格式导出与班级项目接口

| 方法与路径 | 功能描述 |
| :--- | :--- |
| `POST /api/v1/exports` | 将当前草稿渲染为二进制附件（支持全部 8 种格式，带隐私脱敏模板选项）。 |
| `GET /api/v1/projects/recent` | 索引本地指定目录下的所有班级项目清单。 |
| `POST /api/v1/projects/bundle` | 将班级项目全量打包为 `.seattrellis.zip` 下载。 |
| `POST /api/v1/projects/restore` | 上传 `.zip` 备份包并恢复至本地文件夹。 |

---

## 📖 相关文档

- [系统架构设计](architecture.md)
- [交互式编辑器协议](editor-protocol.md)
- [多格式导出与排版打印](export.zh.md)
