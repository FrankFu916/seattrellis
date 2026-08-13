# 中文字体策略

本文档说明 SeatTrellis v2 导出功能（HTML / PDF / PNG）中的中文字体兼容策略。

## 核心原则

- **不将字体文件提交到仓库**。字体文件体积大（单个中文字体 5–30 MB），且大多数字体有版权限制。
- **PDF 不嵌入字体**：按名字引用系统 CJK 字体，由查看器替换，保证导出文件体积紧凑。
- **使用跨平台字体发现链**：按固定质量优先级在常见平台字体目录中查找可用 CJK 字体。

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
- `Noto Sans CJK SC`（Google Noto 中文，部分发行版默认）
- `WenQuanYi Micro Hei`（文泉驿微米黑，常见社区字体）
- `WenQuanYi Zen Hei`（文泉驿正黑）
- `FandolSong` / `FandolHei`（部分 TeX 发行版附带）

CSS 回退链：`"Noto Sans CJK SC", "WenQuanYi Micro Hei", "WenQuanYi Zen Hei", sans-serif`

## PDF 导出（系统字体智能引用）

v2 的 PDF 渲染器不依赖 WeasyPrint、Pango 或任何 Python 包。它按固定的质量
优先级链在常见平台字体目录中枚举字体，把最佳可用 CJK 字体按 PostScript 名字
写入 PDF，由查看器用自己的字体替换（任何带图形界面的设备都有 CJK 字体）：

1. `PingFang SC`（macOS）或 `Noto Sans CJK SC`（Linux）——推荐档，效果最佳；
2. `Microsoft YaHei`（Windows）——可接受档；
3. `SimSun` / 其他 CJK 字体——fallback 档，导出时会提示"效果可能低于预期"；
4. 找不到任何 CJK 字体——保持纯 ASCII 的 Helvetica 路径，并提示安装字体。

发现链顺序固定，因此同一台机器上的导出结果是确定性的；只有"哪款字体存在"依赖
环境。Windows 应确保微软雅黑已安装（默认已安装）；Linux（服务器/Docker）可安装：

```bash
# Debian/Ubuntu
sudo apt-get install fonts-noto-cjk
# CentOS/RHEL
sudo yum install google-noto-sans-cjk-fonts
```

## PNG 导出

PNG 渲染器使用与 PDF 相同的系统字体发现逻辑（同一优先级链），把发现的 CJK
字体文件交给本地光栅化器绘制。发现链找不到中文字体时，PNG 中的中文可能显示为
占位符；此时按上面 Linux/服务器一节安装 CJK 字体即可。

## 用户自定义字体

如果用户有自己的字体文件（如学校购买的授权字体），可以：

1. 将 `.ttf` / `.otf` 文件放在系统字体目录（如 `~/Library/Fonts` 或
   `~/.fonts`），使其进入系统字体发现路径；
2. 导出时优先使用质量档更高的字体（如 PingFang SC / Noto Sans CJK SC）。

## 版本兼容

- 本策略文档随版本更新，不产生 breaking change；
- 默认行为保持：不要求用户安装字体，不随仓库分发字体，不嵌入字体文件；
- 所有 examples/ 使用系统默认字体即可正常渲染。
