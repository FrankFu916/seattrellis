# 测试与验收策略

SeatTrellis v2 是纯 Rust 工程。测试分为五层：Rust 单元/集成测试、应用级 smoke、
浏览器 E2E、性能基准和发布前人工验收。普通开发可以先跑较快的子集，发布前再跑
完整清单。

## 本地自动测试（v2 主线）

```bash
# 服务器构建脚本会嵌入 clients/web/dist（React 构建产物）——
# 在运行任何 workspace 级 cargo 命令前先构建前端：
cd clients/web && npm ci && npm run build && cd ../..

cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis_cli
cargo clippy --all-targets -p seattrellis_core -p seattrellis_cli -- -D warnings

# Rust App 服务器
cargo test --locked -p seattrellis_app
cargo clippy --all-targets -p seattrellis_app -- -D warnings

# Tauri 壳（需要 1.88 工具链）
cargo build --locked -p seattrellis_desktop

# React 工作台
cd clients/web && npm test && npm run typecheck && npm run build

# 契约漂移检查（生成的 schema / OpenAPI / TS client）
cargo run -p xtask -- contract check

# 仓库卫生与"无 Python 运行时"检查（Python 只作为 dev/测试 runner）
python3 scripts/check_repository_hygiene.py
python3 scripts/check_no_python_runtime.py --tree --expect-retired
```

Rust 是唯一语义真相：规则编译、合法性、编辑状态机、migration、隐私和求解状态
只由 Rust 决定。求解状态词汇表冻结为 `Solved / ProvenInfeasible / Timeout /
Unknown / InvalidInput / Cancelled / InternalError`，CLI 退出码冻结为
0/2/3/4/5/70/130。每个 solve/edit/repair/rotation/export 产物都必须经过独立
validator 复核，禁止硬编码 `feasible=true`。

## Oracle 差分

Python oracle 已退休（v1.9.0 冻结，`v1.x-maintenance` 维护），只作为行为基准。
oracle 差分以冻结 tag 安装：

```bash
python -m venv .oracle-venv
.oracle-venv/bin/pip install "seattrellis[all] @ git+https://github.com/FrankFu916/seattrellis@v1.9.0"
.oracle-venv/bin/python scripts/rust_python_diff.py --fixtures    # 41 个 case 七状态差分
.oracle-venv/bin/python scripts/rust_python_diff.py --cli-golden  # CLI stdout golden 差分
```

任何 Python error 都不能记为 INFEASIBLE（七状态语义），mismatch 必须非零退出。
完整命令见 [development.md](development.md) 的 Oracle Differentials 一节。

## Web smoke 测试

React 工作台的前端单元测试和生产构建在 `clients/web/` 完成：

```bash
cd clients/web
npm test -- --run
npm run typecheck
npm run build
```

编辑器协议 contract 测试覆盖九类 operation、必填版本字段、严格 ID/revision
类型、旧 revision、错误 draft、重复 command ID、批次原子失败和按命令撤销/重做。
状态测试还会遍历全部字段，确认不包含成绩、备注、特殊需求、身高、视力或任意学生
扩展属性，并核对学生与座位关联一致。两份编辑器 JSON Schema 与 registry 生成结果
逐字典比较，避免已提交契约漂移。

发布前还应运行一次真实 Chromium 流程，覆盖名单导入映射、教室编辑、常用与高级
规则、未来轮换、调整、导出以及项目面板。

## 浏览器级 E2E

真实浏览器测试使用 `web-e2e-rust` CI job：Python 只作为 runner（Playwright），
不安装任何 Python 包。工作台 E2E 在三个隔离浏览器会话中真实执行：

1. Demo → 三个候选 → public 模板 → 姓名匿名化 → A4 横向英文 Print HTML，
   同时检查全班原姓名、学号、成绩、身高、视力和特殊需求没有泄漏；
2. 上传 CSV 学生名单、layout JSON 和 rules JSON，跨步骤返回后继续求解两个
   候选，并从下载的 candidate set 验证学生数量、唯一座位和 fixed-seat 规则；
3. 使用临时 Project 路径读取信息、校验、生成两个候选、切换非推荐候选，并
   下载包含所选候选 ID 的解释报告。

## 性能基准

发布前至少跑一次固定 40/50/60 人合成数据集（`scripts/benchmark_solver.py`，
参数与报告字段定义见 [benchmarks.md](benchmarks.md)）。每周 benchmark workflow
把人数、约束 profile 和 backend 分片并行执行，避免一个慢 case 丢失其他报告。

普通 CI 不按绝对秒数失败。手动和每周 benchmark workflow 归档 JSON/Markdown
报告；回退判断比较同类 runner 上的相对变化，并单独观察可行率、候选产出率和
候选多样性。性能回归门槛（基准 ×1.10 + 绝对上限）在 CI 常跑。

## 发布前人工 smoke

发布前应实际运行 CLI 和 Web。CLI 主流程：

```bash
seattrellis_cli doctor
seattrellis_cli validate --problem problem.json
seattrellis_cli solve --problem problem.json --output plan.json
seattrellis_cli candidates --problem problem.json --count 5
seattrellis_cli history-report --problem problem.json --history-dir examples/history
seattrellis_cli pair-report --problem problem.json --history-dir examples/history
seattrellis_cli project-info --project examples/project.seattrellis.json
seattrellis_cli project-validate --project examples/project.seattrellis.json
seattrellis_cli export --problem problem.json --solution plan.json --format png --output plan.png
```

Web 人工验收至少确认名单导入、快速求解、结果页、导出设置和 Project 工作区能
完成主流程。如果使用真实学校数据，应在本地完成测试，禁止把数据、截图、导出结果
或日志提交到公开仓库。

## 相关文档

- [开发指南](development.md)：构建与测试命令、架构规则、oracle 差分
- [发布检查清单](release-checklist.md)：发布前完整验收清单
