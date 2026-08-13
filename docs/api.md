# API

SeatTrellis v2 的程序化接口是 loopback HTTP API（`seattrellis_app` /
`seattrellis-server`），完整契约见 [`api-v1-openapi.json`](api-v1-openapi.json)。
所有接口只绑定本机服务，`/api/*` 全部要求 `Authorization: Bearer <token>`
（`/api/v1/session` 引导端点除外），Host 必须为 loopback 名 + 绑定端口，Origin
存在时必须同源，响应含 CSP / X-Frame-Options: DENY / Referrer-Policy:
no-referrer。领域结果（如 `/api/v2/solve`）以 HTTP 200 携带求解状态返回。

v1 的 Python API（`seattrellis.service` / `seattrellis.service_types`）属于
遗留行：冻结在 1.9.0，源码在 `v1.x-maintenance` 分支，可通过
`pip install seattrellis==1.9.0` 安装；v2 不再提供 Python 入口。

## HTTP 接口

本地 React 工作台和桌面端共用以下接口：

- `GET /api/v1/session`：引导端点，浏览器工作台用它获取会话 token；
- `POST /api/v2/solve`：稳定的、无副作用的求解契约；
- `GET /api/v1/projects/recent?root=...&limit=...` 返回最近项目的名称、路径和修改时间；
- `POST /api/v1/projects/history` 返回历史/生成文件的元数据，不返回学生记录；每个文件还
  可带有隐私安全的 `provenance` 摘要（来源类型、上一级文件名和操作次数），不会在
  摘要中返回原始操作命令或上一级文件的本机路径；`operation_history` 只包含脱敏后的
  操作序号、动作类型和调整类别；
- `POST /api/v1/projects/artifacts/compare` 比较两个历史或输出文件，只返回隐私安全的
  方案摘要和结构变化数量；
- `POST /api/v1/projects/artifacts/restore` 从一个历史或输出文件创建新的输出 snapshot，
  不覆盖原始文件；
- `POST /api/v1/projects/migration/preview` 校验项目主文件或项目内 artifact，并返回当前
  schema、默认迁移目标、字段级结构变化、迁移前校验和回滚提示；差异只包含路径和类型，
  不返回学生原始值。迁移项目主文件时还会返回每个名单、布局、规则、历史目录和输出目录
  引用的存在性与类型检查；
- `POST /api/v1/projects/migration/apply` 写入迁移结果。默认创建新的 `.migrated.json` 文件，
  只有显式传入 `in_place: true` 才会替换源文件，并先生成 `.bak` 备份；写入后会再次
  校验输出文件，并返回备份路径和字段变化摘要；
- `POST /api/v1/projects/migration/batch/preview` 一次检查多个项目主文件，返回每个项目的
  独立迁移摘要，并标出多个项目共同引用的名单、布局、规则或目录；该接口只预览，不会写入文件；
- `POST /api/v1/projects/rotation/save` 根据当前每一期的服务端编辑草稿写入新的
  `rotation-plan.json` 输出；保存会校验名单和布局与生成时一致，不覆盖已有文件，并把
  Web 编辑命令写入每一期 snapshot 的 `metadata.manual_edit`；
- `POST /api/v1/projects/rotation/load` 从项目 history 或 outputs 中安全载入一个
  `rotation_plan`，重新创建每一期的短期编辑草稿，供浏览器或桌面端继续调整；
- `POST /api/v1/projects/rotation/group-register` 从已保存的 `rotation_plan` 生成小组登记表，
  支持可打印 HTML 和带 UTF-8 BOM 的 CSV；每一期都会保留空组、名单中不存在的成员和未入座学生，
  不会改写原始项目文件；
- `POST /api/v1/projects/rotation/group-register/preview` 在下载前按期次汇总小组人数、已入座、
  未入座和名单缺失情况，并比较相邻期次的新增/移出人数。响应只含匿名引用和统计，不返回姓名或学号；
- `POST /api/v1/classes/rotation` 根据一个班级草稿生成多个未来 period，并返回每一期的
  独立编辑草稿；
- `POST /api/v1/projects/privacy` 执行分享前敏感字段检查；
- `POST /api/v1/projects/bundle` 下载 `.seattrellis.zip`；
- `POST /api/v1/projects/restore` 接收本地 bundle 路径或 multipart 上传并恢复到指定目录；
- `POST /api/v1/exports` 从当前编辑草稿生成下载文件。请求中的 `template` 可以是
  `public`、`teacher` 或 `report`，`privacy` 支持隐藏成绩/备注/特殊需求、匿名化、
  身高和视力字段，`orientation` 与 `page_scale` 控制 A4 页面。`public` 模板始终强制
  隐藏敏感字段，即使客户端提交了相反选项也不会放宽安全边界。

上传恢复仍受项目包总大小、manifest、路径遍历和符号链接校验限制；错误响应不会
包含学生数据。领域结果不会因为求解状态（如 `Timeout` / `ProvenInfeasible`）而
改变 HTTP 状态码，七种状态都是正常领域结果。

## 人工调整草稿

编辑通过版本化的 `EditorCommandEnvelope` / `EditorStateEnvelope` 协议进行
（`protocol_version: "1.0"`）：每个命令携带唯一 `command_id`、草稿 `draft_id`
和 `base_revision`；服务端在写入前拒绝错误草稿、重复命令和旧 revision，并把
同一命令中的多项操作作为原子撤销批次。支持的 operation 包括 `swap_students`、
`move_student`、`batch_move`、`seat_student`、`unseat_student`、
`lock_student` / `unlock_student`、`lock_seat` / `unlock_seat`。

编辑草稿允许临时出现未入座学生，但会拒绝重复座位、重复学生、未知学生和禁用
座位。每次成功操作都会返回 hard constraint 诊断；当前锁状态通过
`metadata.lock_state` 在内存和文件工作流中保持一致。`batch_move` 会先验证全部
映射再一次更新 assignments：学生和目标座位必须唯一，占用目标座位的学生必须也
参与批次；任一锁定、未知或冲突映射都会拒绝整个命令，undo/redo 也把它视作单条
操作。

需要锁定后重排时，`repair` 会保留 `--lock-student` / `--lock-seat` 与 snapshot
中保存的锁；`affected` 范围会自动加入与这些学生存在 hard rule 或当前座位相邻
关系的一阶学生，其余当前已入座学生会临时固定在原座。原始 `RuleSet` 不会被修改，
请求范围、有效范围、临时固定关系、有效固定关系和输出差异写入 `metadata.repair`，
供 UI 展示和审计。

状态协议只包含学生 key、显示名称、当前座位和锁定状态、座位状态、undo/redo 深度
和 hard constraint 诊断，不包含成绩、备注、特殊需求、身高、视力或任意扩展属性。
具体格式见[编辑器协议](editor-protocol.md)。

读取 snapshot、candidate set 或 project 时应检查 `schema_version`；v1 时代的
文件由迁移路径自动处理，迁移前创建备份。字段定义见[输入格式](input-format.zh.md)，
版本策略见[版本与兼容](versioning.md)。
