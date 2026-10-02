# 原生客户端方向

决策日期：2026-10-01，实现更新：2026-10-02。继续提供现有 React 工作台与 Tauri 桌面壳。首个 [macOS SwiftUI/AppKit 预览版](https://github.com/FrankFu916/seattrellis/tree/main/clients/macos)已通过共享[进程内桥接](native-bridge.md)实现单方案流程；其他平台客户端仍是开发方向。

## 各平台技术与规范

| 平台 | 技术 | 官方设计规范 | 系统能力 |
| --- | --- | --- | --- |
| macOS | SwiftUI，必要时使用 AppKit | [Apple HIG](https://developer.apple.com/design/human-interface-guidelines/) | 文档窗口、菜单、快捷键、原生保存打开、打印、VoiceOver |
| iOS/iPadOS | SwiftUI，必要时使用 UIKit | Apple HIG | 文件选择、沙盒授权、自适应导航、动态字体、VoiceOver、iPad 键鼠支持 |
| Windows | WinUI 3 / Windows App SDK | [Fluent / Windows 设计](https://learn.microsoft.com/windows/apps/design/) | 原生对话框、命令、高对比度、UI Automation、键盘和触控笔、打印 |
| KDE Linux | Qt 6 Quick + Kirigami | [KDE HIG](https://develop.kde.org/hig/) | 桌面 portal、KDE 主题、无障碍、文件与打印对话框 |
| GNOME Linux | GTK 4 + libadwaita | [GNOME HIG](https://developer.gnome.org/hig/) | 自适应面板、portal、AT-SPI、GNOME 主题和对话框 |
| Android | Kotlin + Jetpack Compose | [Material 3](https://m3.material.io/)及 [Android 应用质量规范](https://developer.android.com/quality) | Storage Access Framework、适配大屏、TalkBack、字体缩放、系统返回行为 |
| 浏览器 | React、TypeScript、语义 HTML | Web 无障碍与响应式规范 | 可用时使用 File System Access；其余浏览器上传/下载，后续评估本地 WASM Worker |

AppKit 只能用于 macOS，iOS 应使用 SwiftUI/UIKit。Qt 可作为通用桌面方案，但统一皮肤不会自动符合 Apple、Fluent 或 GNOME 规范。KDE 与 GNOME 共用业务逻辑、文档和验收输入，呈现层按桌面环境分别实现。

## 共享边界

所有客户端调用相同的应用层：导入、生成、取消、编辑命令、修复、审计、序列化/重开和导出。编辑状态是显示投影，不是完整名单；原始字段、求解上下文、保存版本及锁定状态必须完整持久化。禁止在各端重复实现评分和规则语义。

macOS 预览版直接调用 ABI 1，不使用本地 HTTP 服务或 WebView。桥接已明确缓冲区所有权、会话、取消、错误与状态映射；UniFFI 仍可用于后续移动端绑定，桌面库不等于已认证的 iOS/Android SDK。Swift、WinUI、Qt 接入不要求将 Rust 求解器重写为 C++。

预览版已实现 JSON 名单与完整请求编辑、生成、移动/交换/锁定、撤销重做、保存重开、修复、审计和 SVG/HTML 导出；使用原生打开保存对话框、菜单快捷键、后台派发和未保存输入保护。候选集合、轮换、原生打印、界面本地化、移动端与签名仍需开发。macOS CI 编译真实客户端并运行模型测试，窗口操作及 VoiceOver 的人工验收另行进行。

## 开发顺序与验收

先做一个完整切片：完整名单→求解→键盘/触屏移动座位→锁定→保存→关闭→重开→局部修复→审计→导出。各端使用同一批合成的 40/60/80 人班级文档；之后加入候选对比和轮换。优先验证 Windows/macOS 桌面，再根据真实测试设备推进 Android/iPad 与 Linux 专用客户端。

每个客户端必须在目标系统测试中文输入法、字体缩放、高对比度与深色模式、屏幕阅读器完整操作、焦点、触控尺寸、减少动态效果、真实文件访问、取消、系统打印和离线行为。同机测量启动时间、空闲内存与交互延迟后，才能宣称替换带来性能提升。安装包、签名和更新另行验收。

本地 Linux 环境无法运行 SwiftUI/AppKit、WinUI 或 Android 界面。目标系统 CI 提供 macOS 编译与模型测试，不代表实机可用性已认证；达到功能一致性前，保留当前工作台。
