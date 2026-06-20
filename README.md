# 席序 SeatTrellis

**简体中文｜[English](README.en.md)**

一个注重隐私与公平的智能排座工具，让每一个座位各得其所，让每一种关系安然成序。

## 项目简介

席序 SeatTrellis 是一个本地优先的智能排座工具，适合教师、班主任、教务人员和需要处理教室座位安排的开发者使用。它可以根据学生名单、教室布局和排座规则自动生成座位表，并保存可复现的 JSON 快照，方便审阅、导出和复盘。

项目默认在本机处理数据，不上传真实学生名单、成绩、座位偏好或历史座位结果。

## 核心特性

- 本地优先、隐私友好；
- 支持 CSV 和 Excel 学生名单导入；
- 支持基于 seat nodes 的教室布局；
- 支持硬约束和软约束；
- 支持 OR-Tools CP-SAT 自动排座；
- 支持可复现 JSON snapshot；
- 支持 Excel、PNG、HTML 导出；
- 支持 CLI；
- 支持 Streamlit 本地网页端；
- 提供虚构示例数据、pytest 测试和 GitHub Actions。

## 适用场景

- 日常排座；
- 考试座位安排；
- 小组合作和邻座搭配；
- 固定座位、禁邻、间隔距离；
- 座位轮换和公平分配；
- 需要保存可复现记录的排座流程。

## 快速开始

```bash
python -m pip install -e ".[dev]"
seattrellis init-demo
seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
seattrellis export --snapshot outputs/latest.snapshot.json --format html
```

启动本地网页端：

```bash
python -m pip install -e ".[web]"
streamlit run src/seattrellis/web/app.py
```

## 安装方法

```bash
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis
python -m venv .venv
source .venv/bin/activate
python -m pip install -e ".[dev,web]"
pytest
```

Windows PowerShell：

```powershell
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -e ".[dev,web]"
pytest
```

## 基本用法

生成虚构示例数据：

```bash
seattrellis init-demo
```

生成座位快照：

```bash
seattrellis solve \
  --students examples/students.xlsx \
  --layout examples/classroom.json \
  --rules examples/rules.json
```

指定输出路径：

```bash
seattrellis solve \
  --students examples/students.csv \
  --layout examples/classroom.json \
  --rules examples/rules.json \
  --output outputs/demo.snapshot.json
```

导出结果：

```bash
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
seattrellis export --snapshot outputs/latest.snapshot.json --format html
```

当前仍保留 `seatplanner` 命令作为兼容别名，新文档和新项目统一使用 `seattrellis`。

## 输入文件格式

学生名单支持 CSV 和 Excel。`student_id` 或 `name` 至少提供一个，其他字段可选。

```csv
student_id,name,gender,height_cm,score,vision,tags,needs,notes
STU001,Student001,F,154,92,poor,leader,vision_front,
```

教室布局使用 JSON，核心是 seat node，不局限于矩阵：

```json
{
  "layout_id": "fictional-room",
  "name": "Fictional Classroom",
  "seats": [
    {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
    {"seat_id": "R1C2", "row": 1, "col": 2, "enabled": false, "zone": "aisle"}
  ],
  "adjacency": {
    "include_horizontal": true,
    "include_vertical": false,
    "include_diagonal": false,
    "custom_edges": []
  }
}
```

规则文件分为 `hard` 和 `soft`：

```json
{
  "seed": 42,
  "hard": {
    "fixed_seats": [{"student": "STU001", "seat_id": "R1C1"}],
    "must_be_adjacent": [{"students": ["STU002", "STU003"]}],
    "cannot_be_adjacent": [{"students": ["STU004", "STU005"]}],
    "min_distance": []
  },
  "soft": {
    "vision_front": {"enabled": true, "weight": 20},
    "height_back": {"enabled": true, "weight": 1},
    "randomize": {"enabled": true, "weight": 1},
    "score_balance": {"enabled": true, "weight": 1}
  }
}
```

完整示例见 `examples/students.csv`、`examples/students.xlsx`、`examples/classroom.json` 和 `examples/rules.json`。

## 输出结果

当前支持：

- JSON snapshot：默认输出到 `outputs/latest.snapshot.json`；
- Excel：座位网格和 assignment 明细；
- PNG：座位表图片；
- HTML：可在浏览器打开的本地座位表。

## 项目结构

```text
.
├── src/seattrellis/
│   ├── models/      # 学生、座位、教室布局、规则、snapshot 数据模型
│   ├── solver/      # adjacency graph 和 CP-SAT 求解器
│   ├── io/          # CSV、Excel、JSON 导入与持久化
│   ├── exporters/   # Excel、PNG、HTML 导出
│   ├── web/         # Streamlit 本地网页端
│   ├── cli.py       # CLI 入口
│   └── demo.py      # 虚构 demo 数据生成
├── examples/        # 仅包含虚构示例数据
├── tests/           # pytest 测试
└── .github/workflows/tests.yml
```

## 隐私说明

- 不要把真实学生名单、成绩、座位偏好、班级信息或历史座位快照上传到公开仓库。
- `examples/` 中只能放虚构数据。
- `outputs/`、`exports/`、`snapshots/`、`private/`、`data/`、`real_students/`、`real_classes/`、`.env` 已被 `.gitignore` 忽略。
- 项目默认在本地处理数据，不默认上传云端。
- 分享问题前请先脱敏，删除姓名、学号、成绩、备注、班级、学校和任何可识别信息。

## 开发路线

已完成：

- CSV/Excel 学生名单导入；
- JSON 教室布局、规则和 snapshot；
- seat nodes 和 adjacency graph；
- OR-Tools CP-SAT 自动排座；
- 固定座位、必须相邻、禁止相邻、最小间隔；
- 视力靠前、高个靠后、随机扰动、邻座成绩搭配；
- Excel、PNG、HTML 导出；
- CLI、本地 Streamlit UI、示例数据和自动测试。

计划中：

- SQLite 历史库；
- 历史同桌回避和座位公平轮换增强；
- 小组均衡、标签分布、区域偏好；
- Word/PDF 导出；
- Word/PDF/图片导入和可选 OCR；
- 交互式教室布局编辑器；
- 稳定 JSON schema 和 PyPI 发布。

## 参与贡献

欢迎提交 Issue 和 Pull Request。提交前请运行 `pytest`，为新规则或新导入导出行为添加测试，并确保不提交真实学生数据或隐私材料。

## 许可证

本项目使用 Apache License 2.0。详见 [LICENSE](LICENSE)。
