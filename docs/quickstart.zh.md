# 快速开始

[English](quickstart.md) / [简体中文](quickstart.zh.md)

本文档提供 SeatTrellis v2.0.0（纯 Rust 版本）的安装与命令行使用指南。如果你只想快速了解项目概况，请阅读[文档首页](index.zh.md)。

## 安装

SeatTrellis v2 是一个纯 Rust 本地工具，不需要 Python、Node.js 或其他运行时。

### 桌面应用（推荐）

从 [Releases](https://github.com/FrankFu916/seattrellis/releases) 页面下载对应平台的安装包：

- **Windows**：MSI 或 NSIS 安装包（x64）
- **macOS**：DMG 或 app 压缩包（Apple Silicon）
- **Linux**：DEB 包

安装前请用 `SHA256SUMS` 校验每个下载文件。

### 命令行工具

```bash
cargo install seattrellis
# 或使用 Releases 页面提供的预编译二进制
```

安装后运行 `seattrellis --help` 查看全部命令，`seattrellis doctor` 可以检查环境（二进制版本、core API 版本、临时目录是否可写）。

### 网页工作台

`seattrellis_web` 会启动一个只绑定本机回环地址（`127.0.0.1:8765`）的本地服务，并在浏览器中打开 React 工作台：

```bash
cargo run -p seattrellis_web -- --open-browser
# 或从 Releases 的预编译二进制运行
seattrellis_web --open-browser
```

桌面应用（Tauri 壳）内部就是启动同一个服务并在原生窗口中加载工作台。

### v1 Python 版本（遗留）

v1（Python）版本冻结在 **1.9.0**，在 `v1.x-maintenance` 分支上维护。旧版包的安装方式是：

```bash
pip install seattrellis==1.9.0
```

新用户请使用 v2 的桌面应用或 Rust CLI；v1 只作为遗留兼容包存在，不再开发新功能。

## 最快上手（CLI）

v2 CLI 围绕一份"问题文件"（`CoreSolveRequest` JSON）工作，它把学生、座位、规则和求解参数放在同一个文件里。以 `problem.json` 为例：

```json
{
  "api_version": 2,
  "student_count": 4,
  "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
  "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
  "fixed_seats": [[0, 0]],
  "seed": 42,
  "students": [
    {"key": "STU001", "display_name": "Alice"},
    {"key": "STU002", "display_name": "Bob"},
    {"key": "STU003", "display_name": "Carol"},
    {"key": "STU004", "display_name": "Dave"}
  ],
  "rules": {"seed": 42, "soft": {"randomize": {"enabled": true, "weight": 1}}}
}
```

三条核心命令：

```bash
# 只校验问题文件，不运行求解
seattrellis validate --problem problem.json

# 求解并把完整结果写入 plan.json
seattrellis solve --problem problem.json --output plan.json

# 把已保存的方案渲染为 PNG
seattrellis export --problem problem.json --solution plan.json --format png --output plan.png
```

`export` 支持 `svg`、`html`、`png`、`pdf`、`xlsx`、`docx`、`pptx` 七种格式；打印版 `print-html` 仅通过 `project-export` 使用。`solve` 的退出码是 v2 冻结表：`0` 成功、`2` 输入无效、`3` 确认不可行、`4` 超时、`5` 未知、`70` 内部错误、`130` 用户取消。确认不可行的退出码 3 在 `solve` / `candidates` / `project-rotate` / `project-solve` 上保持一致。

## 示例数据

仓库的 `examples/` 目录包含虚构数据：`students.csv`、`classroom.json`、`rules.json`、`rules_multi_candidate.json`、`rules_neighbor_avoidance.json`、`history/` 和 `project.seattrellis.json`。v2 CLI 没有 `init-demo` 命令，示例文件直接从仓库获取。

## 内置场景 Preset

`validate` 的 `--preset` 选项会按场景检查问题缺少哪些首选数据（history、score、height、vision），并给出 warning。内置场景包括 `random`、`exam`、`daily`、`fair-rotation`、`neighbor-aware`、`balanced`、`peer-mixing`、`score-high-front`、`score-high-back`、`row-score-balanced`、`group-score-balanced`、`mentor-pairing`、`height-aware`、`vision-friendly`：

```bash
seattrellis validate --problem problem.json --preset daily --history-dir examples/history
```

`--strict` 会把 warning 当作失败。预设是规则 JSON 的便利层，完整的规则说明见[规则说明](rules.zh.md)。

## 求解

```bash
# 固定 seed，结果可复现
seattrellis solve --problem problem.json --seed 42 --output outputs/latest.snapshot.json

# 带墙钟预算；预算耗尽时报 Timeout（退出码 4），有合法方案时仍为 Solved
seattrellis solve --problem problem.json --time-limit 3 --output outputs/latest.snapshot.json
```

## 验证与检查

```bash
# 校验问题文件（学生/座位/规则引用、固定座位与相邻规则冲突等）
seattrellis validate --problem problem.json

# 预检：候选座位域与不可行原因
seattrellis precheck --problem problem.json

# 审计已求解的方案：hard 规则状态 + soft 评分明细
seattrellis audit --problem problem.json --solution plan.json

# 对固定 assignment 打分（PlanScore 明细）
seattrellis score --problem problem.json --assignment '[[0,0],[1,1],[2,2],[3,3]]'
```

## 多方案生成

```bash
seattrellis candidates --problem problem.json --count 5 > outputs/candidates.json
```

`candidates` 生成最多 N 个满足全部 hard 约束的不同方案（1–20，默认 5），每个候选带独立 snapshot、总分和评分明细；推荐方案是加权总分最高的 hard-valid 候选。

## 历史分析

`history-report` 汇总每名学生的前排、后排、边侧、角落、靠窗、靠门、靠讲台、靠空调历史次数；`pair-report` 汇总两两学生的同桌、横向、纵向、斜向、任意相邻和指定距离内次数：

```bash
seattrellis history-report --problem problem.json --history-dir examples/history --output outputs/history-report.json
seattrellis pair-report --problem problem.json --history-dir examples/history --top 10
```

历史 snapshot 来自 `examples/history/` 等目录中的 `*.snapshot.json` 文件。历史缺失不会导致求解失败，只会让 `fair_rotation` 等评分维度显示为 `not_available`。

## 人工调整与局部修复

`edit` 对已保存的 snapshot 或候选集执行命令式微调：

```bash
seattrellis edit \
  --snapshot outputs/latest.snapshot.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R1C1 \
  --output outputs/edited.snapshot.json
```

支持的 operation：`swap:STU001:STU002`、`move:STU003:R2C2`、`batch-move:STU001=R1C2,STU002=R1C1`、`seat:STU003:R2C2`、`unseat:STU004`、`lock-student:STU001`、`unlock-student:STU001`、`lock-seat:R1C1`、`unlock-seat:R1C1`。默认即使违反 hard 约束也会写出草稿并显示诊断；加 `--strict` 后违反 hard 约束则命令失败且不写文件。多组操作也可写进 JSON 文件，用 `--operations-file` 读取并重放。

`repair` 在保留锁定座位的前提下对部分学生重新求解：

```bash
seattrellis repair \
  --problem problem.json \
  --snapshot outputs/edited.snapshot.json \
  --lock-student STU001 \
  --lock-seat R4C3 \
  --affected STU002 \
  --output outputs/repaired.snapshot.json
```

`--affected` 限制重排范围；`--lock-student` / `--lock-seat` 保留当前座位；默认继承 snapshot 中保存的锁定状态，可用 `--ignore-saved-locks` 忽略。

## Project 工作流

Project 文件用相对路径和默认配置把学生名单、layout、规则和历史目录组织成一个本地工作区。它适合 v1 风格的文件式工作流，也便于长期保存：

```bash
# 在已有 students.csv / layout.json / rules.json 的目录中创建 project 文件
seattrellis project-init --dir my-class

# 查看配置和路径状态
seattrellis project-info --project my-class/seattrellis.project.json

# 校验
seattrellis project-validate --project my-class/seattrellis.project.json

# 求解并把保存的方案写入输出
seattrellis project-solve --project my-class/seattrellis.project.json --candidates 3 --output outputs/project.plan.json

# 导出已保存的方案（不会重新求解）
seattrellis project-export --project my-class/seattrellis.project.json --snapshot outputs/project.plan.json --format html --output outputs/project.html

# 轮换：生成未来多个时段
seattrellis project-rotate --project my-class/seattrellis.project.json --periods 4

# 备份与恢复
seattrellis project-pack --project my-class/seattrellis.project.json --output my-class.seattrellis.zip
seattrellis project-restore --bundle my-class.seattrellis.zip --output-dir restored/

# 隐私扫描
seattrellis project-privacy --project my-class/seattrellis.project.json
```

`project-edit` / `project-repair` 复用与 `edit` / `repair` 相同的语义。详细说明见 [Project 工作流详解](project.zh.md)。

## Schema 工具

v2 产物（snapshot、candidate set、project、rotation plan 等）都带 `schema_version`。查看注册表、导出 JSON Schema、迁移旧版本文件：

```bash
seattrellis schema-list
seattrellis schema-export --kind seatingsnapshot --output seating-snapshot.v2.schema.json

# project 文件自带 kind 字段，可直接迁移到 v2：
seattrellis schema-migrate --input my-class/seattrellis.project.json --output my-class/seattrellis.v2.project.json

# 学生名单 / 教室布局的 v1 文档没有 kind 字段，需包一层信封再迁移，
# 或用 --in-place 就地重写（先自动创建隐藏事务备份，重复运行不会互相覆盖）：
# {"kind": "student_roster", "schema_version": 1,
#  "data": {"students": [{"student_id": "STU001", "name": "Alice"}]}}
seattrellis schema-migrate --input roster-v1.json --in-place
```

目前只有 roster（`student_roster`）、layout（`classroom_layout`）、project（`seattrellis_project`）三类提供 v1→v2 迁移步骤；snapshot、candidate set、ruleset 等暂无迁移步骤，传入会明确报错。`schema_version` 高于支持版本的文件会被拒绝迁移（防降级）。

## 继续阅读

- [CLI 命令参考](cli.md)
- [输入格式说明](input-format.zh.md)
- [规则说明](rules.zh.md)
- [Web 端使用指南](web.zh.md)
- [Project 工作流详解](project.zh.md)
- [导出格式说明](export.zh.md)
