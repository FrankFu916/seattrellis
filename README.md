# 席序 SeatTrellis

[![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)

**简体中文 | [English](README.en.md)**

席序 SeatTrellis 是一个本地优先的课堂排座工具，用虚构示例数据展示可复现的座位安排流程。它从学生名单、教室座位节点和规则文件生成 JSON snapshot，并可导出 Excel、PNG、HTML。

项目默认在本机处理数据。不要把真实学生名单、学号、成绩、班级、学校、座位偏好或历史座位快照提交到公开仓库。

![Demo seating chart](docs/assets/demo-seating.png)

## 快速开始

```bash
python -m pip install -e ".[dev,web]"
seattrellis init-demo
seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
seattrellis export --snapshot outputs/latest.snapshot.json --format html
```

导出文件会写入 `outputs/`。该目录已被 `.gitignore` 忽略。

## 安装

macOS / Linux:

```bash
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis
python -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -e ".[dev,web]"
pytest
```

Windows PowerShell:

```powershell
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install --upgrade pip
python -m pip install -e ".[dev,web]"
pytest
```

## CLI

```bash
seattrellis --help
seattrellis init-demo --force
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --output outputs/demo.snapshot.json
seattrellis export --snapshot outputs/demo.snapshot.json --format html --output outputs/demo.html
```

`init-demo` 默认不会覆盖已有示例文件；需要覆盖时使用 `--force`。旧命令名 `seatplanner` 仍作为兼容别名保留，新文档统一使用 `seattrellis`。

## 输入与规则

- 学生名单支持 CSV、`.xlsx`、`.xlsm`，至少需要 `student_id` 或 `name`。
- 教室布局使用 JSON seat nodes，支持 `enabled=false` 的不可用座位。
- 规则文件分为 `hard` 和 `soft`。
- 详细格式见 [输入格式](docs/input-format.zh.md) 和 [规则说明](docs/rules.zh.md)。

## 求解器

默认使用内置 deterministic fallback solver，确保示例和小型排座流程无需重依赖即可运行。可选 OR-Tools CP-SAT 支持保留在 `solver` extra 中：

```bash
python -m pip install -e ".[solver]"
SEATTRELLIS_USE_ORTOOLS=1 seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
```

如果本地 OR-Tools 导入失败，项目会回退到内置 solver。

## 本地网页端

```bash
python -m pip install -e ".[web]"
streamlit run src/seattrellis/web/app.py
```

## 当前支持

- CSV / Excel 学生名单导入；
- JSON 教室布局、规则和 snapshot；
- seat nodes 和 adjacency graph；
- 固定座位、必须相邻、禁止相邻、最小距离；
- 视力靠前、高个靠后、随机扰动、邻座成绩偏好；
- Excel、PNG、HTML 导出；
- CLI、本地 Streamlit UI、虚构示例数据、pytest 和 GitHub Actions。

## 隐私说明

- `examples/` 只能包含虚构数据。
- `outputs/`、`exports/`、`snapshots/`、`private/`、`data/`、`real_students/`、`real_classes/` 和 `.env` 已被忽略。
- 分享 Issue、PR、截图或测试数据前，请删除姓名、学号、成绩、备注、班级、学校和任何可识别信息。

## 发布

v0.1.0 准备事项见 [release checklist](docs/release-checklist.md)，变更见 [CHANGELOG.md](CHANGELOG.md)。

## 许可证

Apache License 2.0。详见 [LICENSE](LICENSE)。
