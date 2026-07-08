# 测试与验收策略

SeatTrellis 的测试分为四层：单元测试、应用级 smoke、性能基准和发布前人工验收。
普通开发可以先跑较快的子集，发布前再跑完整清单。

## 本地自动测试

```bash
python -m pytest
python -m compileall -q src/seattrellis scripts/benchmark_solver.py scripts/smoke_cli.py
cargo test --manifest-path native/Cargo.toml
python scripts/check_repository_hygiene.py
mkdocs build --strict
```

其中 Rust 测试只覆盖可选 native core，不代表默认 Python backend 被替换。

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

浏览器级 E2E 仍是后续工作。引入 Playwright 时，建议先覆盖：

1. Demo → 求解 → 查看结果 → 生成 Print HTML；
2. 上传 CSV/JSON → 求解 → 下载 candidate set；
3. Project 文件 → validate → solve → export；
4. 导出隐私字段检查；
5. 主要错误路径和浏览器可访问名称。

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
