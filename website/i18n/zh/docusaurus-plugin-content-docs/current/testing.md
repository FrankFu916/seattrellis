# 测试与质量保障规范（Testing & Acceptance）


**席序（SeatTrellis）** 建立了金字塔型的 5 层全方位质量保障体系：Rust 单元/集成测试、应用冒烟测试、浏览器端到端测试（E2E）、性能回归门禁与发布验收测试。

---

## 🧪 1. 五层测试架构

```mermaid
graph TD
    L5[5. 正式发布全量验收 Release Acceptance] --> L4[4. 性能回归与多期轮换门禁 Performance Gates]
    L4 --> L3[3. 浏览器端到端自动化测试 Browser E2E]
    L3 --> L2[2. 应用用例与契约冒烟测试 Application Smoke]
    L2 --> L1[1. Rust 单元/集成/属性与模糊测试 Unit & Integration]
```

---

## 💻 2. 本地测试执行指南

### 2.1 单元与集成测试
```bash
# 运行全部 690+ 项 Rust 单元与集成测试
cargo test --locked --workspace

# 运行前端 160+ 项 React/Vitest 测试
cd clients/web && npm test -- --run && cd ../..
```

### 2.2 契约与静态一致性检查
```bash
# 检查 OpenAPI 文档与前端 TypeScript 类型契约是否同步
cargo run -p xtask -- contract check

# 检查代码仓库整洁性与运行时纯度
python3 scripts/check_repository_hygiene.py
python3 scripts/check_no_python_runtime.py --tree --expect-retired
```

### 2.3 性能门禁与长周期压力测试
```bash
# 求解器 40/50/60/80 人基准耗时检查
cargo build --release --locked -p seattrellis
python3 scripts/bench_solver.py --check

# 多候选生成与多期长周期轮换压力测试
cargo test --release --locked -p seattrellis_core --test candidates_gate --test long_run_gate -- --ignored
cargo test --release --locked -p seattrellis-application --test rotation_gate -- --ignored
```

---

## 📖 相关文档

- [开发指南](development.md)
- [性能基准测试规范](benchmarks.md)
- [版本发布核对清单](release-checklist.md)


## CLI 验收来源校正

当前 CLI 覆盖由 `cli_arg_sweep.rs` 与 `lifecycle_regressions.rs` 的真实子进程测试提供，检查退出码、dry-run 零写入、保存重开和项目输出。`fixtures/cli-goldens/` 是已退役 oracle 的存档参考，没有当前自动化消费者，也没有可复现的生成流程，不能把存档数量当作现有测试覆盖。


## 当前浏览器链路

Chromium 验收覆盖名单映射、候选选择、锁定/交换/撤销、实名和匿名导出及惰性预览、两期轮换保存重开、班级文件下载后刷新页面并上传重开、完整源字段和锁恢复、局部修复与导出，以及真正另存为后的班级切换。浏览器上传下载不能替代真实系统文件对话框、Tauri 窗口和屏幕阅读器验收，后者仍需目标平台测试。
