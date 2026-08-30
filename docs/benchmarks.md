# 性能基准测试与门禁规范（Benchmarks）

[English](benchmarks.md) · [简体中文](benchmarks.md)

**席序（SeatTrellis）** 建立了严格的算法性能基准门禁（Performance Regression Gates），确保在班级规模扩大（40 ~ 80 人）以及高密度复杂约束下，求解响应速度始终维持在秒级交互体验内。

---

## ⏱️ 1. 求解器性能基准门禁

基准测试使用标准合成测试集（`synthetic-classroom` / `synthetic-v1`），覆盖 40 人、50 人、60 人与 80 人规模的真实教学场景：

```bash
# 1. 构建 Release 高性能二进制
cargo build --release --locked -p seattrellis

# 2. 运行自动化基准测试门禁检查
python3 scripts/bench_solver.py --check
```

### 性能门禁阈值表（Release 模式）

| 班级人数规模 | 最大允许耗时（绝对上限） | CI 回归容忍度 |
| :---: | :---: | :---: |
| **40 人班级** | ≤ 1.5 秒 | ≤ 1.10 × baseline |
| **50 人班级** | ≤ 2.5 秒 | ≤ 1.10 × baseline |
| **60 人班级** | ≤ 3.5 秒 | ≤ 1.10 × baseline |
| **80 人大班** | ≤ 6.0 秒 | ≤ 1.10 × baseline |

---

## 🔄 2. 长周期质量门禁（Long-Run Gates）

在 CI/CD 流水线中，系统持续运行候选集生成与多期轮换质量测试：

```bash
# 候选方案生成与长时间压力测试
cargo test --release --locked -p seattrellis_core \
  --test candidates_gate --test long_run_gate -- --ignored

# 多期轮换一致性与内存泄漏测试
cargo test --release --locked -p seattrellis-application \
  --test rotation_gate -- --ignored
```

- **验证维度**：涵盖 1 期、3 期、5 期、10 期及 20 期的连续轮换稳定性、内存占用基线及候选方案多样性（Diversity Score）。

---

## 📖 相关文档

- [系统架构解析](architecture.md)
- [开发与测试指南](development.md)
- [v1.4 历史性能基准归档](benchmark-baseline-v1.4.md)
