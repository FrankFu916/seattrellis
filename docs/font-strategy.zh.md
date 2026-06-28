# 中文字体策略

本文档说明 SeatTrellis 导出功能（HTML / PDF / PNG）中的中文字体兼容策略。

## 核心原则

- **不将字体文件提交到仓库**。字体文件体积大（单个中文字体 5–30 MB），且大多数字体有版权限制。
- **使用跨平台 CSS font-family 回退链**，优先使用系统自带字体。
- **允许用户自行配置字体**，但不作为默认功能。

## 各平台默认字体

### macOS

系统自带优质中文字体：
- `PingFang SC`（苹方，San Francisco 中文版，macOS 10.11+）
- `Heiti SC`（黑体-简，旧版 macOS）
- `STSong`（华文宋体）
- `STHeiti`（华文黑体）

CSS 回退链：`"PingFang SC", "Heiti SC", "STHeiti", -apple-system, sans-serif`

### Windows

系统自带中文字体：
- `Microsoft YaHei`（微软雅黑，Windows Vista+）
- `SimHei`（黑体，所有 Windows 版本）
- `SimSun`（宋体）
- `FangSong`（仿宋）

CSS 回退链：`"Microsoft YaHei", "SimHei", "SimSun", sans-serif`

### Linux

系统字体取决于发行版和用户安装：
- `Noto Sans SC`（Google Noto 中文，部分发行版默认）
- `WenQuanYi Micro Hei`（文泉驿微米黑，常见社区字体）
- `WenQuanYi Zen Hei`（文泉驿正黑）
- `FandolSong` / `FandolHei`（部分 TeX 发行版附带）

CSS 回退链：`"Noto Sans SC", "WenQuanYi Micro Hei", "WenQuanYi Zen Hei", sans-serif`

## 推荐 CSS 回退链（跨平台）

```css
font-family:
    "PingFang SC",
    "Microsoft YaHei",
    "Noto Sans SC",
    "WenQuanYi Micro Hei",
    -apple-system,
    sans-serif;
```

## PDF 导出（WeasyPrint）

WeasyPrint 依赖系统字体。如果 PDF 中文显示为方块或空白：

1. **macOS**：通常无需配置，PingFang SC 自动可用。
2. **Windows**：确保微软雅黑已安装（默认已安装）。
3. **Linux（服务器/Docker）**：
   ```bash
   # Debian/Ubuntu
   sudo apt-get install fonts-noto-cjk
   # CentOS/RHEL
   sudo yum install google-noto-sans-cjk-fonts
   ```

WeasyPrint 也可以配置自定义字体目录：
```python
from weasyprint import HTML
HTML('input.html').write_pdf('output.pdf', font_config=...)
```

## PNG 导出（Pillow）

PNG 导出使用 Pillow 的默认字体（通常为系统默认位图字体），中文支持有限。如需高质量中文 PNG：
1. 安装支持中文的 TrueType 字体
2. 在 `exporters/png.py` 中指定字体路径

此功能属于远期增强，当前版本不要求完美中文 PNG。

## 用户自定义字体

如果用户有自己的字体文件（如学校购买的授权字体），可以：

1. 将 `.ttf` / `.otf` 文件放在项目目录外（如 `~/fonts/`）
2. 通过环境变量或配置文件指定字体路径
3. v1.0 后可能提供 `SEATTRELLIS_FONT_PATH` 配置项

## 版本兼容

- 本策略文档随版本更新，不产生 breaking change
- 默认行为保持：不要求用户安装字体，不随仓库分发字体
- 所有 examples/ 使用系统默认字体即可正常渲染
