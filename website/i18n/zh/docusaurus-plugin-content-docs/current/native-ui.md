# 原生客户端方向

决策日期：2026-10-01。在专用客户端开发期间，继续提供现有 React 工作台与 Tauri 桌面壳。本轮实现的可携班级文档、编辑命令、局部修复、审计和取消接口是各平台的共享基础。本文明确技术路线，不表示这些原生客户端已经完成。

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

桌面原型可调用已认证的 loopback API。移动端优先使用进程内桥接：有界、版本化的 C ABI，或经验证的 UniFFI 等生成桥接。现有服务协议是参考接口，不代表移动端库 ABI 已发布。实际实现前须明确内存所有权、错误与状态映射、取消、线程限制和载荷上限。Swift、WinUI、Qt 接入不要求将 Rust 求解器重写为 C++。

## 开发顺序与验收

先做一个完整切片：完整名单→求解→键盘/触屏移动座位→锁定→保存→关闭→重开→局部修复→审计→导出。各端使用同一批合成的 40/60/80 人班级文档；之后加入候选对比和轮换。优先验证 Windows/macOS 桌面，再根据真实测试设备推进 Android/iPad 与 Linux 专用客户端。

每个客户端必须在目标系统测试中文输入法、字体缩放、高对比度与深色模式、屏幕阅读器完整操作、焦点、触控尺寸、减少动态效果、真实文件访问、取消、系统打印和离线行为。同机测量启动时间、空闲内存与交互延迟后，才能宣称替换带来性能提升。安装包、签名和更新另行验收。

当前 Linux 云环境无法编译或认证 SwiftUI、WinUI 和 Android 产品。不会用空壳工程冒充已交付的原生应用；替代端达到完整链路与功能一致性前，保留当前工作台。
