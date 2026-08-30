# 输入数据与 Schema 格式规范

[English](input-format.md) · [简体中文](input-format.zh.md)

**席序（SeatTrellis）v2.0.0** 围绕**学生花名册（Roster）**、**教室网格布局（Classroom Layout）**与**排座规则集（RuleSet）**三大核心数据展开。

无论是通过班级项目文件（`*.project.json`）管理，还是内联至单文件 `problem.json` 中直接求解，所有输入均经过 Rust 本地严格类型与边界校验。

---

## 👥 1. 学生花名册（CSV / Excel）

系统支持标准的 `.csv` 文件以及 Excel `.xlsx` / `.xlsm` 工作簿，由原生 Rust 解析器直接读取，无需安装 Python 或 Office 运行库。

### 字段定义与表头映射

| 字段名称 | 标识符 (`key`) | 类型 | 必填性 | 说明与示例 |
| :--- | :--- | :--- | :--- | :--- |
| **学号/唯一编码** | `student_id` | 文本 | 推荐 | 学生的稳定唯一编号（如 `STU001`）。若省略，系统将自动以 `name` 作为内部标识。 |
| **学生姓名** | `name` | 文本 | 核心 | 学生的展示姓名（如 `张三`）。 |
| **性别/分组** | `gender` | 文本 | 可选 | 用于性别均衡或组别划分（如 `男`/`女`、`M`/`F`）。 |
| **身高 (cm)** | `height_cm` / `height` | 正数 | 可选 | 学生的实际身高，用于高个靠后排序（如 `168.5`）。 |
| **综合/学业成绩** | `score` | 数值 | 可选 | 成绩或加权综合分，用于学业分层互助（如 `89.5`）。 |
| **视力情况** | `vision` / `vision_score` | 文本/数值 | 可选 | 视力状态（如 `poor`、`0.6`、`近视`），用于排座前排关照。 |
| **个性化标签** | `tags` | 文本 | 可选 | 支持以逗号、分号、竖线分隔（如 `班长, 组长, 实验员`）。 |
| **特殊关照需求** | `needs` | 文本 | 可选 | 特殊需求说明（如 `靠窗, 靠前排`）。 |
| **教师内部备注** | `notes` | 文本 | 可选 | 教师日常排座参考备注。 |

### Excel（.xlsx / .xlsm）读取边界规范
- **默认读取第一个工作表**：仅解析工作簿中的 Sheet 1。
- **纯文本与前导零保留**：诸如 `001`、`020` 等学号格式将被严格保留为文本，不会误转为整数。
- **公式解析规则**：支持解析已包含计算缓存结果的公式单元格；若遇到无缓存值的裸公式将提示明确错误。
- **安全规格上限**：单个文件最大支持 20 MiB，最大支持 10,000 行学生数据及 256 列。

---

## 🏫 2. 教室网格布局 JSON（`layout.json`）

教室布局由离散的“座位节点（Seat Nodes）”拓扑构成，**不局限于规则矩形**，可完美适配 L 型、多边形或缺角异形教室。

```json
{
  "layout_id": "class-room-a",
  "name": "三年二班主教室",
  "seats": [
    { "seat_id": "R1C1", "row": 1, "col": 1, "enabled": true },
    { "seat_id": "R1C2", "row": 1, "col": 2, "enabled": false, "zone": "aisle" },
    { "seat_id": "R1C3", "row": 1, "col": 3, "enabled": true, "near_window": true }
  ],
  "adjacency": {
    "include_horizontal": true,
    "include_vertical": false,
    "include_diagonal": false,
    "custom_edges": []
  }
}
```

### 座位节点属性说明

| 属性 | 类型 | 说明 |
| :--- | :--- | :--- |
| `seat_id` | 字符串 | 必填。教室内的唯一座位编码（如 `R1C1`、`A-01`）。 |
| `row` / `col` | 正整数 | 必填。座位的逻辑排号与列号（从 1 开始计）。 |
| `enabled` | 布尔值 | 可选。默认为 `true`；设为 `false` 表示该位置为走廊、立柱或不可用空位。 |
| `zone` | 字符串 | 可选。区域标签（如 `front`、`middle`、`back`、`aisle`）。 |
| `near_window` / `near_door` / `near_ac` / `near_platform` | 布尔值 | 可选。标识该座位是否临窗、临门、临空调或靠讲台。 |
| `group_id` | 字符串/数值 | 可选。所属小组标识，用于小组分层排座。 |

---

## 🎒 3. 班级项目描述文件（`seattrellis.project.json`）

用于将班级的所有关联资源以相对路径集中管理：

```json
{
  "kind": "seattrellis_project",
  "schema_version": 1,
  "name": "高一（3）班",
  "students": "students.csv",
  "layout": "classroom.json",
  "rules": "rules.json",
  "history_dir": "history",
  "outputs_dir": "outputs",
  "default_candidates": 5,
  "default_candidate": "recommended",
  "default_export_format": "html"
}
```

- **纯相对路径**：所有文件路径均相对于项目文件所在目录解析，便于整班文件夹拷贝与跨机器分享。
- **元数据安全性**：项目文件本身仅保存配置路径，不包含真实敏感的学生名单或成绩，可安全进行版本控制。

---

## 📸 4. 历史快照（Historical Snapshot JSON）

历史快照是系统进行公平轮换和搭档回避分析的核心依据。

```json
{
  "schema_version": 2,
  "snapshot_id": "2026-term1-w01",
  "timestamp": "2026-09-01T08:00:00Z",
  "assignment": {
    "STU001": "R1C1",
    "STU002": "R1C3"
  },
  "metrics": {
    "solved": true,
    "score": 92.5
  }
}
```

- 系统按文件名时间序加载历史快照（如 `examples/history/*.snapshot.json`）；
- 若历史快照中缺少某位新转入的学生，系统将自动跳过该学生并在指标中提示，不影响整体求解。

---

## 📖 相关文档

- [排座规则手册](rules.zh.md)
- [快速上手指南](quickstart.zh.md)
- [班级项目工作流](project.zh.md)
