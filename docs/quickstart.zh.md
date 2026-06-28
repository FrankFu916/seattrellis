# 快速开始

本文档提供 SeatTrellis 的详细安装与命令行使用指南。如果你只想快速了解项目概况，请阅读 [README](../README.md)。

## 安装

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

## 演示数据

```bash
seattrellis init-demo
```

`init-demo` 默认不会覆盖已有示例文件；需要覆盖时使用 `--force`。最小安装会生成 CSV/JSON demo；安装 `excel` extra 后也会生成 `examples/students.xlsx`。

旧命令名 `seatplanner` 仍作为兼容别名保留，新文档统一使用 `seattrellis`。

## 内置场景 Preset

```bash
seattrellis presets list
seattrellis presets show daily
seattrellis presets export daily --output outputs/daily.rules.json
```

`presets list` 列出八种内置场景：`random`、`exam`、`daily`、`fair-rotation`、`neighbor-aware`、`balanced`、`height-aware`、`vision-friendly`。

`solve` / `validate` 可以只使用 `--preset`，也可以同时传入 `--rules`：preset 作为基础，用户 JSON 中明确提供的字段递归覆盖 preset，hard rules 仍通过原有校验和求解路径绝对优先。缺少 history、score、height 或 vision 数据时会给出 warning，并只降级相关 soft rule / score 维度。

## 排座

### 单方案求解

```bash
# 使用 preset
seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history --output outputs/daily.snapshot.json

# 使用规则文件
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --output outputs/latest.snapshot.json

# 带历史记录（公平轮换）
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --history-dir examples/history --output outputs/fair.snapshot.json

# 带关系回避
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_neighbor_avoidance.json --history-dir examples/history --output outputs/neighbor-aware.snapshot.json
```

### 多方案生成与评分

```bash
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_multi_candidate.json --history-dir examples/history --candidates 5 --output outputs/candidates.json --report outputs/plan-report.json
```

`--candidates 1` 保持旧行为并写出普通 snapshot。`--candidates N` 会用同一组输入、确定性 seed 序列和"排除已生成完整 assignment"的约束重复求解，写出 `kind: "candidate_set"` JSON。多方案生成是启发式流程，但每个候选仍必须通过全部 hard constraints；如果可行空间中没有足够多的不同方案，结果会保留已找到的方案并给出 warning。

### 可选 OR-Tools 求解器

```bash
python -m pip install -e ".[solver]"
SEATTRELLIS_USE_ORTOOLS=1 seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

只有设置 `SEATTRELLIS_USE_ORTOOLS=1` 时才会尝试导入 OR-Tools。若未安装 `solver` extra，CLI 会提示安装命令并以非零退出码结束。

## 验证

```bash
# 使用 preset
seattrellis validate --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history

# 使用规则文件
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

`validate` 只检查输入文件和明显的规则冲突，不生成座位表；`solve` 会在校验通过后再生成 snapshot。错误信息会尽量指出文件、字段、行号和 hard-rule 冲突。使用 `--strict` 时，warning 也会让命令以非零退出码结束。

## 历史分析

```bash
# 座位公平性报告
seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history

# 同桌/邻座关系报告
seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
```

`history-report` 会基于当前学生名单、当前 layout 和历史 snapshot 输出每名学生的前排、后排、边侧、角落、靠窗、靠门、靠讲台、靠空调次数；加 `--output outputs/history-report.json` 可导出 JSON 报告。

`pair-report` 会输出两两学生的历史同桌、横向、纵向、斜向、任意相邻和指定距离内次数；加 `--top 10` 可限制高频学生对展示数量，加 `--output outputs/pair-report.json` 可导出 JSON。

## 导出

```bash
# HTML 导出（无需 extras）
seattrellis export --snapshot outputs/latest.snapshot.json --format html

# 导出 recommended candidate
seattrellis export --snapshot outputs/candidates.json --candidate recommended --format html --output outputs/recommended.html
```

安装 `excel` 和 `image` extras 后：

```bash
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
```

导出文件会写入 `outputs/`。该目录已被 `.gitignore` 忽略。

## Project 工作流

```bash
# 创建 project 文件
seattrellis project-init --project examples/project.seattrellis.json --name "Demo Class" --students students.csv --layout classroom.json --rules rules_multi_candidate.json --history-dir history --outputs-dir outputs --candidates 5 --force

# 查看配置
seattrellis project-info --project examples/project.seattrellis.json

# 校验
seattrellis project-validate --project examples/project.seattrellis.json

# 求解
seattrellis project-solve --project examples/project.seattrellis.json --candidates 3 --output outputs/project.candidates.json --report outputs/project-plan-report.json

# 导出
seattrellis project-export --project examples/project.seattrellis.json --snapshot outputs/project.candidates.json --candidate recommended --format html --output outputs/project-recommended.html
```

`project-init` 创建轻量的本地项目文件；`project-info` 检查配置和路径状态；`project-validate`、`project-solve`、`project-export` 分别复用现有校验、求解和导出逻辑。project 文件只保存相对路径和默认配置，不嵌入学生名单或座位数据；其中的相对路径始终相对于 project 文件所在目录解析。

## 多方案评分维度

candidate set 中每个方案包含 snapshot、seed、solver backend、总分、hard-constraint 摘要和评分 breakdown。当前可解释维度包括：

- `fair_rotation_score`：启用公平轮换且有历史时可用；
- `avoid_recent_neighbors_score`：启用关系回避且有 pair history 时可用；
- `score_balance_score`、`height_preference_score`、`vision_preference_score`：对应规则启用且输入字段足够时可用；
- `diversity_score`：候选之间 assignment 差异；
- `stability_score`：相对最近历史 snapshot 保持原座位的比例；
- `hard_constraint_summary`：固定座位、相邻/禁止相邻、最小距离和 assignment 完整性检查。

缺少历史、规则未启用或字段不足时，相关维度明确标记为 `not_available`，不会虚构分数。总分是可用维度按规则权重计算的 0–100 加权平均；推荐方案是在满足 hard constraints 的候选中总分最高者，同分时按 `candidate_id` 稳定排序。评分用于比较和解释，不代表全局最优。

普通 snapshot 与 candidate set 是两种不同格式，旧 snapshot 仍可读取。`export` 收到 candidate set 时默认导出 recommended candidate，也可以用 `--candidate candidate_03` 指定。

## 下一步

- [输入格式说明](input-format.zh.md)
- [规则说明](rules.zh.md)
- [Web 端使用指南](web.zh.md)
- [Project 工作流详解](project.zh.md)
- [导出格式说明](export.zh.md)
