# Font Rendering & Typography Strategy

[English](font-strategy.md) · [简体中文](font-strategy.zh.md)

Accurate character rendering, alignment precision, and cross-platform consistency are crucial for classroom seating charts. This document outlines how **SeatTrellis v2.0.0** handles fonts across all export formats (HTML, print-html, SVG, PDF, PNG, and Office documents).

---

## 🎨 1. Core Principles

1. **Zero Font Bundling**: CJK font files are large (5–30 MB each) and carry complex licensing restrictions. SeatTrellis does not bundle fonts into its binaries or repository, keeping installation lightweight.
2. **Local Rasterization (PDF / PNG)**: PDF and PNG exports discover and load high-quality local OS fonts via `fontdue`. The exported PDF is a self-contained, pre-rendered document that **guarantees pixel-perfect rendering on any printer or legacy viewer without font substitution errors**.
3. **Adaptive CSS Font Stacks (HTML / SVG / Office)**: Uses robust native system fallback chains across macOS, Windows, and Linux to maintain clean sans-serif aesthetics.

---

## 🖥️ 2. Platform Font Priority Chains

### 🍎 macOS
- **Primary Font**: PingFang SC (`PingFangSC-Regular`)
- **CSS Stack**: `"PingFang SC", "Heiti SC", "STHeiti", -apple-system, sans-serif`

### 🪟 Windows
- **Primary Font**: Microsoft YaHei (`Microsoft YaHei`), fallback to SimSun
- **CSS Stack**: `"Microsoft YaHei", "SimHei", "SimSun", sans-serif`

### 🐧 Linux & Docker Environments
- **Primary Font**: Noto Sans CJK SC (`NotoSansCJKsc-Regular`)
- **CSS Stack**: `"Noto Sans CJK SC", "WenQuanYi Micro Hei", "WenQuanYi Zen Hei", sans-serif`

> 💡 **Linux Server & Container Tip**:
> For headless Linux servers or CI/CD pipelines, install the open-source Noto CJK package:
> ```bash
> # Debian / Ubuntu:
> sudo apt-get install -y fonts-noto-cjk
>
> # CentOS / RHEL / Fedora:
> sudo yum install -y google-noto-sans-cjk-fonts
> ```

---

## 📐 3. Custom Fonts

To use a custom licensed institutional font (e.g., custom KaiTi or branded corporate font):
1. Install the `.ttf` or `.otf` file into your operating system's standard font directory (e.g., `~/Library/Fonts` on macOS or `~/.fonts` on Linux).
2. Once recognized by the OS, SeatTrellis will automatically leverage the font during generation.

---

## 📖 Related References

- [Export Formats Guide](export.md)
- [Quick Start Guide](quickstart.md)
