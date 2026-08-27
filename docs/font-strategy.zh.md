# 中文字体策略

[English](font-strategy.md) / [简体中文](font-strategy.zh.md)

本文档说明 SeatTrellis v2.0.0 导出功能（HTML / print-html / SVG / PDF / PNG）中的中文字体兼容策略。

## 核心原则

- **不将字体文件提交到仓库**。字体文件体积大（单个中文字体 5–30 MB），且大多数字体有版权限制。
- **HTML、print HTML、SVG 和 Office 文件不打包 SeatTrellis 字体**，由对应的查看器或应用处理字体回退。
- **PDF 和 PNG 在导出时使用本机系统字体进行光栅化**：它们保存导出时已绘制文字的图像，不依赖查看器选择字体。
- **使用固定优先级的字体发现链**：只有本机安装的字体会影响光栅结果。

## 各平台默认字体

### macOS

优先发现 `PingFangSC-Regular`（通常来自 PingFang SC）。

CSS 回退链：`"PingFang SC", "Heiti SC", "STHeiti", -apple-system, sans-serif`

### Windows

随后发现 `MicrosoftYaHei`，再回退到 `SimSun`。这两档主要对应 Windows 字体目录。

CSS 回退链：`"Microsoft YaHei", "SimHei", "SimSun", sans-serif`

### Linux

Linux 常见的首选字体是 `NotoSansCJKsc-Regular`。用户目录中的 PingFang 或 Noto
字体会在标准目录之后按支持的路径检查。

CSS 回退链：`"Noto Sans CJK SC", "WenQuanYi Micro Hei", "WenQuanYi Zen Hei", sans-serif`

## PDF 与 PNG 导出（本地光栅化）

v2.0.0 的 PDF 和 PNG 渲染器不依赖 WeasyPrint、Pango 或任何 Python 包。两者按
`PingFangSC-Regular` → `NotoSansCJKsc-Regular` → `MicrosoftYaHei` → `SimSun`
的固定顺序发现字体文件，用 `fontdue` 在导出进程中绘制文字。PDF 是单页压缩图像，
PNG 以 2 倍密度绘制；两者都不依赖查看器选择字体。

发现顺序固定，因此同一台机器上的结果是确定性的；只有“哪款字体存在”依赖环境。
如果发现或解析不到可用系统字体，文件仍会生成，但 PDF/PNG 文字会被省略并给出：

`no usable system font found; PNG/PDF text was omitted`

Linux（服务器/Docker）可安装：

```bash
# Debian/Ubuntu
sudo apt-get install fonts-noto-cjk
# CentOS/RHEL
sudo yum install google-noto-sans-cjk-fonts
```

## HTML、print HTML 与 SVG

这些格式保留文字，由 CSS 或 SVG 的系统 sans-serif 回退链负责显示。print HTML
的字体栈包含 PingFang SC、Microsoft YaHei 和 Noto Sans CJK SC；浏览器或打印应用
在显示时解析它，与 PDF/PNG 的本地光栅化路径不同。

## 用户自定义字体

如果用户有自己的字体文件（如学校购买的授权字体），可以：

1. 将 `.ttf` / `.otf` 文件放在系统字体目录（如 `~/Library/Fonts` 或
   `~/.fonts`），使其进入系统字体发现路径；
2. 确认该字体位于 SeatTrellis 支持的系统发现路径中；程序不会复制或分发字体。

## 版本兼容

- 本策略文档随版本更新，不产生 breaking change；
- 默认行为保持：不随仓库分发字体，不嵌入 PDF/PNG 字体文件；没有可用字体时明确 warning；
- 所有 examples/ 使用系统默认字体即可正常渲染。
