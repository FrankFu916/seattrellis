# 版本发布核对清单（Release Checklist）

[English](release-checklist.md) · [简体中文](release-checklist.md)

本核对清单用于确保每一个正式公开发布版本的质量、安全性与完整性。

---

## 📋 1. 代码库与范围核查

- [ ] 本次迭代承诺的功能与 Bug 修复均已实现并附带测试用例；
- [ ] `CHANGELOG.md` 已详尽记录用户可见的新增特性、变更及修复项；
- [ ] `Cargo.toml` 与各子 Crate 版本号已严格对齐；
- [ ] 本地 Git 工作区状态整洁（`git status --short` 无未提交修改）。

---

## 🧪 2. 自动化质量门禁核查

- [ ] `cargo test --locked --workspace` 全量测试通过（690+ 项用例）；
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` 静态检查零警告；
- [ ] `cd clients/web && npm test && npm run typecheck && npm run build` 前端测试与编译通过；
- [ ] `python3 scripts/bench_solver.py --check` 求解器性能基准门禁达标；
- [ ] 跨平台 CI 流水线（Linux / macOS / Windows）全部绿灯通过。

---

## 🔍 3. 功能与隐私验收

- [ ] 运行 `seattrellis doctor` 检查环境正常；
- [ ] 验证 `validate`、`solve`、`candidates`、`edit`、`repair`、`project-*` 常用命令功能正常；
- [ ] 导出公开公示版测试，核实学生姓名已脱敏，学号、成绩等敏感数据已彻底屏蔽；
- [ ] 确认示例数据中仅包含虚构数据，无任何真实个人信息。

---

## 🚀 4. 打包与发布动作

- [ ] 创建 Git 标签 `v<version>` 并推送到远端；
- [ ] 确认 GitHub Releases 自动构建成功，产物附带 `SHA256SUMS`；
- [ ] 执行 `cargo publish -p seattrellis` 发布至 crates.io；
- [ ] 在干净的隔离环境中执行 `cargo install seattrellis` 验证安装成功。
