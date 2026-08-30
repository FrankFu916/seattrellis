# 中文字体与高保真排版策略

[English](font-strategy.md) · [简体中文](font-strategy.zh.md)

在排版与打印座位表时，中文字符的清晰度、对齐精度与跨平台一致性至关重要。本文档阐述 **席序（SeatTrellis）v2.0.0** 在各类导出格式（HTML、print-html、SVG、PDF、PNG 及 Office）中的中文字体渲染与回退策略。

---

## 🎨 1. 核心设计原则

1. **零外部字体捆绑**：不将商业中文字体打包进安装包或代码仓库，既避免版权合规风险，又将二进制包体积控制在极限轻量。
2. **端到端独立光栅化（PDF / PNG）**：PDF 和 PNG 导出时，直接在本地由 Rust 引擎（`fontdue`）加载操作系统内置的优质中文字体进行像素级光栅化绘制。导出的 PDF 本身即包含精确渲染的文字图像，**无论在任何老旧电脑或打印机上打开，绝不会出现字体缺失、乱码或文字跑位**。
3. **自适应 CSS 字体栈（HTML / SVG / Office）**：面向浏览器与文档应用，提供完备的系统字体回退链（Fallback Stack），确保在 macOS、Windows、Linux 下均呈现原生现代无衬线质感。

---

## 🖥️ 2. 各操作系统字体优先级链

### 🍎 macOS
- **首选字体**：苹方（`PingFang SC` / `PingFangSC-Regular`）
- **回退栈**：`"PingFang SC", "Heiti SC", "STHeiti", -apple-system, sans-serif`

### 🪟 Windows
- **首选字体**：微软雅黑（`Microsoft YaHei`），次选中易宋体（`SimSun`）
- **回退栈**：`"Microsoft YaHei", "SimHei", "SimSun", sans-serif`

### 🐧 Linux / Docker 容器环境
- **首选字体**：思源黑体（`Noto Sans CJK SC`）
- **回退栈**：`"Noto Sans CJK SC", "WenQuanYi Micro Hei", "WenQuanYi Zen Hei", sans-serif`

> 💡 **服务器与 Linux 容器字体安装**：
> 若在无图形界面的 Linux 服务器或 CI/CD 容器中运行导出任务，推荐预装开源思源黑体：
> ```bash
> # Debian / Ubuntu:
> sudo apt-get install -y fonts-noto-cjk
>
> # CentOS / RHEL / Fedora:
> sudo yum install -y google-noto-sans-cjk-fonts
> ```

---

## 📐 3. 自定义字体使用指引

如果您的学校或机构拥有企业授权专属字体（如定制楷体、兰亭黑体等），无需修改 SeatTrellis 任何代码：
1. 将 `.ttf` 或 `.otf` 字体文件安装至系统标准字体目录（如 macOS 的 `~/Library/Fonts`，或 Linux 的 `~/.fonts`）；
2. 操作系统生效后，系统在生成 HTML 或渲染位图时即可自动应用该字体。

---

## 📖 相关文档

- [多格式导出与排版打印](export.zh.md)
- [快速上手指南](quickstart.zh.md)
