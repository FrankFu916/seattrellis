# 版本发布与完整性保障（Publishing）

[English](publishing.md) · [简体中文](publishing.md)

本文档规范了 **席序（SeatTrellis）** 在 GitHub Releases 与 crates.io 上的发布流程与产物完整性校验标准。

---

## 📦 1. 发布渠道与产物

1. **GitHub Releases**：
   - 提供 macOS（`.dmg` / `.app.tar.gz`）、Windows（`.msi` / `.exe`）与 Linux（`.deb`）桌面安装包；
   - 提供适用于多平台的独立 CLI 二进制（`seattrellis`）及 Web 服务二进制（`seattrellis_web`）；
   - 随版本附带 `SHA256SUMS` 与 `DESKTOP-SHA256SUMS` 散列校验清单。
2. **crates.io 官方源**：
   - 发布核心 Rust CLI 工具源码包（`cargo publish -p seattrellis`）。

---

## 🔒 2. 产物校验与安全性说明

- **未签名提示说明**：桌面安装包目前默认未购买商业代码签名证书。用户首次在 macOS 打开时可在“访达”中右键选择“打开”，Windows 出现 SmartScreen 时选择“仍要运行”；
- **哈希自检**：建议用户在安装前对比官方发布的 SHA256 哈希值确保文件未被篡改。

---

## 🔄 3. 发布回滚与补丁策略

- **标签不可变性**：已发布的 Git Tag 与 crates.io 版本严禁覆盖或重命名；
- **缺陷修复**：如发现重大缺陷，通过发布自增补丁版本（如 `2.0.1`）进行修复，并在必要时对缺陷版本执行 crates.io yank 废弃操作。

---

## 📖 相关文档

- [版本发布核对清单](release-checklist.md)
- [版本命名与兼容性规范](versioning.md)
