# 测试与验收策略

SeatTrellis 的测试分为五层：单元测试、应用级 smoke、浏览器 E2E、性能基准和
发布前人工验收。普通开发可以先跑较快的子集，发布前再跑完整清单。

## 本地自动测试

```bash
python -m pytest
python -m compileall -q src/seattrellis scripts/benchmark_solver.py scripts/smoke_cli.py
cargo test --manifest-path native/Cargo.toml
python scripts/check_repository_hygiene.py
mkdocs build --strict
```

其中 Rust 测试只覆盖可选 native core，不代表默认 Python backend 被替换。
文档构建完成后，`site/schemas/` 应包含 registry 中的全部公开 Schema。

## Web smoke 测试

当前 Web 自动测试使用 Streamlit 的 app testing API：

```bash
python -m pytest tests/test_web_workflow.py
```

测试覆盖：

- 页面可加载；
- Demo 数据可一键载入；
- rules 与 history 预览可操作；
- 快速求解可生成结果；
- 导出隐私设置可显示并切换；
- 中文 / 英文切换后主步骤状态保持稳定。

关键控件使用稳定 widget key，而不是依赖中文或英文 label。后续调整文案、视觉主题
或拆分页面时，应优先保留这些 key，或同步更新 `seattrellis.web.keys` 和对应测试。
人工调整测试会在 Streamlit AppTest 中实际执行交换、移动到空座、移出、重新入座、
撤销和重做，并检查人数不变量与生成草稿的 `metadata.manual_edit`，不只是验证控件
是否存在。锁定测试还会执行“锁定 → 撤销 → 重做 → repair → 解锁”，验证学生位置
不变且 `metadata.lock_state` 在编辑和求解之间一致。
批量移动同时覆盖领域层原子失败、CLI JSON/内联解析，以及 Web 多选配对后的单次
撤销/重做，确保批次不会产生中间态。
座位图测试会真实点击已占座位和空座完成移动，点击两名学生完成交换，并切换图上
座位锁；同时验证锁定学生的座位按钮不可操作。

编辑器协议 contract 测试覆盖九类 operation、必填版本字段、严格 ID/revision
类型、旧 revision、错误 draft、重复 command ID、批次原子失败和按命令撤销/重做。
状态测试还会遍历全部字段，确认不包含成绩、备注、特殊需求、身高、视力或任意学生
扩展属性，并核对学生与座位关联一致。两份编辑器 JSON Schema 与 registry 生成结果
逐字典比较，避免已提交契约漂移。

## 浏览器级 E2E

真实浏览器测试与 AppTest 分开安装和执行：

```bash
python -m pip install -e ".[web,e2e]"
python -m playwright install chromium
python -m pytest e2e --browser=chromium
```

Linux 开发机若尚未安装 Chromium 的系统依赖，使用
`python -m playwright install --with-deps chromium`。CI 通过
`e2e/constraints.txt` 固定已验证的 Streamlit 与 Playwright 组合；项目 extras
仍保留兼容范围，便于本地验证更新版本。

当前黄金路径会启动独立的 Streamlit 进程，并真实执行：

1. 切换英文界面并加载 Demo；
2. 使用 fallback backend 生成三个候选方案；
3. 下载并解析 candidate set JSON；
4. 启用 public 模板、姓名匿名化、A4 横向和英文内容；
5. 生成并下载 Print HTML；
6. 检查匿名姓名存在，原姓名、成绩、身高、视力和特殊需求没有泄漏；
7. 确认页面没有 Streamlit exception，服务在下载后仍能通过 health check。

测试使用应用自有的 keyed region 与可访问名称定位控件，不依赖控件顺序或中文
文案。CI 在 Ubuntu、Python 3.12 和 Chromium 上单独执行这组测试；失败时上传
浏览器 trace、截图和 Streamlit 服务日志。

后续浏览器覆盖按以下顺序扩充：

1. 上传 CSV/JSON → 求解 → 下载 candidate set；
2. Project 文件 → validate → solve → export；
3. 拖拽或点击调整 → undo/redo → constraint diagnosis；
4. 主要错误路径和键盘操作。

## 性能基准

发布前至少跑一次固定 40/50/60 人合成数据集：

```bash
python scripts/benchmark_solver.py \
  --sizes 40,50,60 \
  --backends fallback,ortools \
  --candidates 1 \
  --time-limit 10 \
  --output outputs/benchmark-solver.json \
  --markdown-output outputs/benchmark-solver.md
```

普通 CI 不建议用绝对秒数直接失败。更合理的做法是归档 JSON 报告，并在 nightly
或发布前流程比较相对回退比例。

## 发布前人工 smoke

除 pytest 外，发布前应实际运行 CLI 和 Web。CLI 主流程可以直接运行：

```bash
python scripts/smoke_cli.py \
  --optional auto \
  --time-limit 3 \
  --json-report outputs/cli-smoke.json
```

该脚本会在临时目录中执行 `init-demo`、preset、validate、solve、history、pair、
project、candidate export 和 print-html 隐私参数流程；如果本地安装了 Excel、
PNG 或 DOCX 依赖，也会自动覆盖这些导出路径。PDF 因依赖系统原生库，默认不跑；
需要发布前单独验证时可加 `--include-pdf`。

手动验收时仍建议抽查以下命令，确认终端输出和用户体验符合预期：

- `seattrellis init-demo --force`
- `seattrellis validate ...`
- `seattrellis solve ... --backend fallback`
- `seattrellis solve ... --backend ortools`
- `seattrellis export ... --format print-html`
- `seattrellis project-validate ...`
- `seattrellis project-solve ...`
- `streamlit run src/seattrellis/web/app.py`

Web 人工验收至少确认 quick solve、结果页、导出设置、Project 工作区能完成主流程。
如果使用真实学校数据，应在本地完成测试，禁止把数据、截图、导出结果或日志提交到
公开仓库。
