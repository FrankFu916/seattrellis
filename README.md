# 席序 SeatTrellis

[![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)

**简体中文 | [English](README.en.md)**

席序 SeatTrellis 是一个本地优先的课堂排座工具，用虚构示例数据展示可复现的座位安排流程。它可以生成单个 JSON snapshot，也可以一次生成多个带可解释评分的 candidate plans，并导出 Excel、PNG、HTML。

项目默认在本机处理数据。不要把真实学生名单、学号、成绩、班级、学校、座位偏好或历史座位快照提交到公开仓库。

![Demo seating chart](docs/assets/demo-seating.png)

## 快速开始

```bash
python -m pip install -e .
seattrellis --help
seattrellis init-demo
seattrellis validate --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history
seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history --output outputs/daily.snapshot.json
seattrellis export --snapshot outputs/daily.snapshot.json --format html
```

导出文件会写入 `outputs/`。该目录已被 `.gitignore` 忽略。

更多命令、场景和详细用法见 **[快速开始指南](docs/quickstart.zh.md)**。

## 安装层级

### 最小安装

```bash
python -m pip install -e .
seattrellis --help
```

最小安装支持 CLI help、CSV 输入、JSON layout/rules/snapshot/candidate set、内置规则 preset、本地 project workspace、deterministic fallback solver、多方案生成与评分，以及不依赖重库的 HTML 导出。

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

网页端支持选择内置 preset，也可以上传 rules JSON 作为覆盖；可上传多份历史 snapshot，生成 1–20 个候选方案，查看推荐方案、评分明细和 hard rule 检查，并下载 JSON、report、HTML、PNG 或 Excel。也可以读取本机 project 文件，复用 project-info、validate、solve 和 export 工作流。

## CLI

```bash
seattrellis --help
seattrellis presets list
seattrellis presets show daily
seattrellis validate --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history
seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history --output outputs/daily.snapshot.json
seattrellis export --snapshot outputs/daily.snapshot.json --format html
```

完整命令行用法、Project 工作流、多方案生成与评分见 **[快速开始指南](docs/quickstart.zh.md)**。

### Preset 与规则叠加

`presets list` 列出八种内置场景：`random`、`exam`、`daily`、`fair-rotation`、`neighbor-aware`、`balanced`、`height-aware`、`vision-friendly`。`solve` / `validate` 可以只使用 `--preset`，也可以同时传入 `--rules` 作为 overlay。缺少 history、score、height 或 vision 数据时会给出 warning 并自动降级相关 soft rule。

### Project 工作流

`project-init` 创建轻量的本地项目文件；`project-info`、`project-validate`、`project-solve`、`project-export` 分别复用现有校验、求解和导出逻辑。Project 文件只保存相对路径和默认配置，不嵌入学生名单。详见 [Project 工作流详解](docs/project.zh.md)。

### 历史分析

`solve` 支持 `--history` 或 `--history-dir` 加载历史 snapshot。`history-report` 输出每个学生的座位分类历史统计，`pair-report` 输出两两学生的同桌/邻座关系历史。详见 [快速开始指南](docs/quickstart.zh.md)。

导出支持 HTML（无需 extras）、Excel（需 `excel` extra）、PNG（需 `image` extra）。详见 [导出格式说明](docs/export.zh.md)。

## 多方案与评分

`--candidates N` 会生成 N 个不同方案，每个方案经过 7 维可解释评分（公平轮换、关系回避、成绩均衡、身高偏好、视力偏好、方案多样性、稳定性），选出最高分作为推荐方案。不可用的维度明确标记为 `not_available`，不虚构分数。

详细评分维度和使用说明见 **[快速开始指南 — 多方案评分维度](docs/quickstart.zh.md#多方案评分维度)**。

## 输入与规则

- 学生名单支持 CSV；安装 `excel` extra 后支持 `.xlsx` 和 `.xlsm`。旧版 `.xls` 请先另存为 `.xlsx` 或 CSV。
- 教室布局使用 JSON seat nodes，支持 `enabled=false` 的不可用座位。
- 规则文件分为 `hard` 和 `soft`。
- 内置 preset 生成同一种标准 rules JSON；它们不是新的求解器或规则格式。
- 未识别的规则字段会作为错误报告，避免拼写错误被静默忽略。
- `fair_rotation` 是基于历史座位类别次数的 soft rule；hard rules 仍然优先，无历史时不会报错。
- `avoid_recent_neighbors` 是基于历史同桌/相邻关系的 soft rule；fixed seats、必须相邻、禁止相邻、最小距离等 hard rules 仍然优先，无历史时不会报错。当前 fallback solver 和 OR-Tools solver 都把它作为启发式评分处理，不保证绝对最优。
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
- JSON 教室布局、规则、snapshot、candidate set 和本地 project workspace；
- 八种可发现、可导出、可与用户 rules 叠加的场景 preset；
- seat nodes 和 adjacency graph；
- 固定座位、必须相邻、禁止相邻、最小距离；
- 视力靠前、高个靠后、随机扰动、邻座成绩偏好、公平轮换、近期同桌/相邻回避启发式偏好；
- 历史 snapshot 统计、`history-report` 本地公平性摘要和 `pair-report` 关系历史摘要；
- 多方案生成、可解释评分、comparison report 和 recommended candidate；
- 可移植的相对路径 project 配置，以及 `project-init` / `project-info` / `project-validate` / `project-solve` / `project-export`；
- HTML 导出，安装 `excel` / `image` extras 后支持 Excel / PNG 导出；
- 输入预检与冲突诊断、CLI、本地 Streamlit UI、虚构示例数据、pytest 和 GitHub Actions。

## 隐私说明

- `examples/` 只能包含虚构数据。
- `examples/history/` 只包含虚构历史 snapshot，用于演示公平轮换和关系历史回避。
- project 文件只保存路径和默认配置，不应嵌入或替代真实学生数据文件。
- `outputs/`、`exports/`、`snapshots/`、`private/`、`data/`、`real_students/`、`real_classes/` 和 `.env` 已被忽略。
- 分享 Issue、PR、截图、测试数据或历史座位记录前，请删除姓名、学号、成绩、备注、班级、学校和任何可识别信息。不要把真实历史座位 snapshot 提交到公开仓库。
- 不要把真实 candidate reports 或 candidate-set snapshots 提交到公开仓库；请只写入已忽略的 `outputs/` 等私有路径。

当前公平轮换和关系回避基于历史次数进行启发式评分，不保证绝对公平或绝对最优。

## 发布

当前稳定版本为 v0.8.0；发布检查见 [release checklist](docs/release-checklist.md)，变更见 [CHANGELOG.md](CHANGELOG.md)。

## 许可证

Apache License 2.0。详见 [LICENSE](LICENSE)。
