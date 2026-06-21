# 席序 SeatTrellis

[![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)

**简体中文 | [English](README.en.md)**

席序 SeatTrellis 是一个本地优先的课堂排座工具，用虚构示例数据展示可复现的座位安排流程。它从学生名单、教室座位节点和规则文件生成 JSON snapshot，并可导出 Excel、PNG、HTML。

项目默认在本机处理数据。不要把真实学生名单、学号、成绩、班级、学校、座位偏好或历史座位快照提交到公开仓库。

![Demo seating chart](docs/assets/demo-seating.png)

## 快速开始

最小安装只安装核心模型、CLI 和 fallback solver：

```bash
python -m pip install -e .
seattrellis --help
seattrellis init-demo
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format html
```

导出文件会写入 `outputs/`。该目录已被 `.gitignore` 忽略。

## 安装层级

### 最小安装

```bash
python -m pip install -e .
seattrellis --help
```

最小安装支持 CLI help、CSV 输入、JSON layout/rules/snapshot、内置 deterministic fallback solver，以及不依赖重库的 HTML 导出。

### 常用本地安装

```bash
python -m pip install -e ".[excel,image]"
```

适合 CSV/Excel 输入，以及 Excel、PNG、HTML 输出。

### 完整开发安装

```bash
python -m pip install -e ".[all,dev]"
pytest
```

`all` extra 包含 OR-Tools、Excel、PNG 和 Streamlit 相关依赖；`dev` extra 包含测试和构建工具。

### 网页端

```bash
python -m pip install -e ".[web,excel,image]"
streamlit run src/seattrellis/web/app.py
```

网页端依赖 Streamlit。若要在网页端上传 Excel 或下载 PNG/Excel，请同时安装 `excel` 和 `image` extras。

## CLI

```bash
seattrellis --help
seattrellis init-demo --force
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --output outputs/demo.snapshot.json
seattrellis export --snapshot outputs/demo.snapshot.json --format html --output outputs/demo.html
```

安装 `excel` 和 `image` extras 后，也可以运行：

```bash
seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
```

`init-demo` 默认不会覆盖已有示例文件；需要覆盖时使用 `--force`。最小安装会生成 CSV/JSON demo；安装 `excel` extra 后也会生成 `examples/students.xlsx`。旧命令名 `seatplanner` 仍作为兼容别名保留，新文档统一使用 `seattrellis`。

`validate` 只检查输入文件和明显的规则冲突，不生成座位表；`solve` 会在校验通过后再生成 snapshot。错误信息会尽量指出文件、字段、行号和 hard-rule 冲突。使用 `--strict` 时，warning 也会让命令以非零退出码结束。

## 输入与规则

- 学生名单支持 CSV；安装 `excel` extra 后支持 `.xlsx` 和 `.xlsm`。旧版 `.xls` 请先另存为 `.xlsx` 或 CSV。
- 教室布局使用 JSON seat nodes，支持 `enabled=false` 的不可用座位。
- 规则文件分为 `hard` 和 `soft`。
- 未识别的规则字段会作为错误报告，避免拼写错误被静默忽略。
- 详细格式见 [输入格式](docs/input-format.zh.md) 和 [规则说明](docs/rules.zh.md)。

## 求解器

默认使用内置 deterministic fallback solver，确保示例和小型排座流程无需重依赖即可运行。可选 OR-Tools CP-SAT 支持保留在 `solver` extra 中：

```bash
python -m pip install -e ".[solver]"
SEATTRELLIS_USE_ORTOOLS=1 seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

只有设置 `SEATTRELLIS_USE_ORTOOLS=1` 时才会尝试导入 OR-Tools。若未安装 `solver` extra，CLI 会提示安装命令并以非零退出码结束。

## 当前支持

- CSV 学生名单导入，安装 `excel` extra 后支持 Excel 导入；
- JSON 教室布局、规则和 snapshot；
- seat nodes 和 adjacency graph；
- 固定座位、必须相邻、禁止相邻、最小距离；
- 视力靠前、高个靠后、随机扰动、邻座成绩偏好；
- HTML 导出，安装 `excel` / `image` extras 后支持 Excel / PNG 导出；
- 输入预检与冲突诊断、CLI、本地 Streamlit UI、虚构示例数据、pytest 和 GitHub Actions。

## 隐私说明

- `examples/` 只能包含虚构数据。
- `outputs/`、`exports/`、`snapshots/`、`private/`、`data/`、`real_students/`、`real_classes/` 和 `.env` 已被忽略。
- 分享 Issue、PR、截图或测试数据前，请删除姓名、学号、成绩、备注、班级、学校和任何可识别信息。

## 发布

v0.1.2 准备事项见 [release checklist](docs/release-checklist.md)，变更见 [CHANGELOG.md](CHANGELOG.md)。

## 许可证

Apache License 2.0。详见 [LICENSE](LICENSE)。
