# SeatTrellis v2 — rc.1 代码签名决策（PD-D19，2026-08-13）

> decision_id：`PD-D19-CODE-SIGNING-RC1`
> 状态：冻结；适用于 v2.0.0-rc.1（及签名证书到位前的后续 RC）
> 依据：计划 §10（RC 门禁）、§发布红线；社区调研见本文档来源清单。

## 决定

rc.1 **不做付费代码签名**（暂缺 Apple Developer $99/年 与 Windows
证书预算），采用社区成熟实践的"未签名 + 完整性保护 + 文档化绕过"
方案；**签名不作为 rc.1 发布阻断项**，但如实写入 release notes。

## 逐平台方案（调研证据）

| 平台 | 方案 | 后果与文档化 | 先例 |
|---|---|---|---|
| macOS | **ad-hoc 签名**（Tauri `signingIdentity: "-"`，arm64 必需） | Gatekeeper 显示"未识别开发者"；绕过：右键→打开，或 系统设置→隐私与安全性→仍要打开；不可公证（免费账户无法公证） | OBS CI 构建（obs-studio #10080）；Tauri v2 文档 |
| Windows | **不签名**（自签证书与无签名行为相同，微软明确无益） | SmartScreen "Windows 已保护你的电脑"→ 更多信息→仍要运行；Windows 11 Smart App Control 可能整体拦截（提供便携版规避） | qBittorrent 多年未签名安装包（#17243/#18028/#24654） |
| Linux | 无需签名 | 发行版仓库自带密钥体系；.deb 直接分发 + SHA256SUMS | Tauri v2 文档 |

## 完整性保护（全部执行）

- 每个 release 资产附 **SHA256SUMS**（Homebrew cask 强制惯例）。
- **SLSA3 provenance**（slsa-github-generator）与 **sigstore keyless**
  （GitHub Actions OIDC）：证明供应链完整性（不影响 Gatekeeper/
  SmartScreen，但提供可验证的构建出处）。
- 若检测到与文档描述不符的警告（如杀软误报）→ 不要绕过，向项目报告。

## Release notes 措辞（英文，rc.1 起）

> **Code signing status.** SeatTrellis rc.1 is distributed unsigned (we
> don't yet have paid Apple/Windows signing certificates). Every release
> asset is integrity-protected via SHA-256 checksums and SLSA provenance —
> verify before installing.
> - **macOS:** first launch shows "unidentified developer." Control-click
>   the app → Open → Open, or System Settings → Privacy & Security → Open
>   Anyway. The .dmg SHA-256 is in SHA256SUMS.
> - **Windows:** SmartScreen may show "Windows protected your PC" — More
>   info → Run anyway after verifying the SHA-256 (Get-FileHash). Windows
>   11 Smart App Control may block the installer; use the portable build.
> - **Linux:** install via the .deb; verify against SHA256SUMS.
> If you see anything other than the described warnings, do not bypass it —
> report it to us instead.

## 真签名路径（预算到位后）

1. **Windows 优先**：Azure Artifact Signing（原 Trusted Signing）
   ~$10/月，无硬件令牌，CI 集成（Tauri `signCommand` 原生支持）；需付费
   Azure 订阅 + 身份验证。
2. **macOS**：Apple Developer $99/年（个人）是 Developer ID + 公证的
   唯一路径；免费账户无法公证。
3. 合计首年约 **$219**（$99 + 12×$10）；证书到手后按版本翻转签名。

## 来源

Tauri v2 signing docs（macOS/Windows/Linux）；Apple Gatekeeper 指南与
未识别开发者绕过；Microsoft SmartScreen reputation 文档；Azure Artifact
Signing FAQ；qBittorrent #17243/#18028/#24654；OBS #10080；sigstore
docs；slsa-github-generator；Homebrew Cask Cookbook。

## 变更影响

- rc.1 release notes 使用上述英文措辞（§用户要求：release 信息英文）。
- rc.1 门禁中"三平台签名"项从**阻断**改为**如实披露**（PD-D19 记录），
  完整性由 SHA256SUMS + SLSA 承担。
