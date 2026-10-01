# 性能基准测试与门禁规范（Benchmarks）


**席序（SeatTrellis）** 建立了严格的算法性能基准门禁（Performance Regression Gates），确保在班级规模扩大（40 ~ 80 人）以及高密度复杂约束下，求解响应速度始终维持在秒级交互体验内。

---

## ⏱️ 1. 求解器性能基准门禁

项目固定样例使用合成数据（`synthetic-classroom` / `synthetic-v1`）；CLI 性能生成器另有下述独立版本，覆盖 40 人、50 人、60 人与 80 人的虚构班级：

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

每个规模执行 5 次并取中位数，以降低共享 CI 机器瞬时抖动造成的误报。基准只在多个无代码差异的干净运行显示持续硬件漂移时更新；绝对上限不会随 runner 漂移放宽。

语料版本 `planted-hard-v1-explicit-soft` 明确关闭全部软目标，保留旧基准实际测量的纯硬约束任务。原来的 `soft={}` 依赖反序列化缺陷，默认值修复后会改变任务。计时命令与历史基线一样不写响应文件，避免把存储锁和 fsync 延迟混入求解器指标；计时后同时验证固定退出码和输出的 `Solved` 状态。旧基线缺少机器/编译器信息，仍作为回归阈值，不能视为受控对照实验。

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


## 当前对照实验与计时限制

2026-10-01 的 Rust/OR-Tools CP-SAT 硬规则实验独立于已退役 oracle，冻结语料、原始计时和独立验证保存在 `benchmarks/solver-comparison/`。它不比较全部软目标，也不能证明语言层面谁更快。中位数减轻瞬时噪声，但 10% 容差不能把不同机器变成等价环境；现行性能门禁同时检查相对基线及明确的绝对上限。旧基线没有记录机器/编译器信息，新记录包括输入与二进制哈希、编译器、提交和机器信息。参见[技术决策](technology-decisions.md)。
