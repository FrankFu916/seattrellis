# 发布与可信发布

SeatTrellis v2 是纯 Rust 项目，发布渠道包括：GitHub Release（预编译 CLI/App 二进制
与桌面安装包 + SHA256SUMS）、crates.io（CLI 源码分发）。v1（Python）版本冻结在
1.9.0，仅作为遗留包保留在 PyPI 上，由 `v1.x-maintenance` 分支维护。

## 发布产物

### GitHub Release（`v<version>` tag）

在 GitHub 上为已审查的提交创建 tag `v<version>`（如 `v2.0.0-rc.2`）并发布 Release。
`release` 事件触发 `.github/workflows/rust.yml`：

1. 在三平台（Linux/Windows/macOS）构建 React 工作台（嵌入 App 服务器二进制）与
   `seattrellis_cli`、`seattrellis_app` release 二进制；
2. `publish-assets` job 汇总 6 个二进制（三平台 × cli/app），计算
   `SHA256SUMS` 并全部附加到 Release；
3. release 专用 job 还运行长跑质量门槛与"生产树和 release 二进制不含 Python
   运行时"扫描。

v1 行（`v1.*` tag）由维护分支处理，不会收到 Rust 二进制。

### 桌面安装包（Tauri）

`.github/workflows/tauri.yml` 在手动指定 release tag、或 `desktop-v*` 预览 tag
发布时运行，为 macOS（`.app`/`.dmg`）、Windows（MSI/NSIS）和 Linux（`.deb`）
构建安装包并附加到对应 Release。桌面安装包使用独立的 `DESKTOP-SHA256SUMS`，
避免与其他产物互相覆盖。

### crates.io（CLI）

```bash
cargo publish -p seattrellis_cli
```

`cargo install seattrellis_cli` 从 crates.io 安装。发布前确认版本号与 GitHub
Release tag 一致，并在干净环境中用 `cargo install` 验证一次安装。

## 一次性配置

- crates.io 账号完成邮箱验证，并拥有 `seattrellis_cli`（及关联 crate）的发布权限；
- GitHub Release 发布权限由仓库维护者持有；`tauri.yml` 需要 `contents: write`。

## 候选验证（RC）

每次候选验证使用独立的预发布版本，例如 `2.0.0-rc.2`、`2.0.0-rc.2`。候选通过后
把版本恢复为最终版本并再次运行完整测试；不要把预发布版本提交合并到正式发布分支。

发布前本地复核：

```bash
seattrellis_cli --version
seattrellis_cli doctor
seattrellis_cli validate --problem problem.json
seattrellis_cli solve --problem problem.json --output plan.json
```

## 失败发布与回滚

- 构建、归档检查或安装验证失败时不创建正式 Release，也不重复使用已上传的版本号；
  修复后递增预发布版本再验证。
- GitHub Release 资产可覆盖（`--clobber`），但 tag 一旦发布不应删除或重写；
- crates.io 的版本不可覆盖或原地替换。若正式版本有缺陷，应立即在 crates.io 上
  yank、在 GitHub Release 中注明影响范围，并发布递增的修复版本；
- 回滚通过发布新的 patch 版本完成，不删除既有 tag，不重写已发布产物。
