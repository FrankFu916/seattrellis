# 导出格式说明

## 支持的格式

v2 的所有导出格式都由本地 Rust 渲染器生成，不需要任何可选安装包：

| 格式 | 说明 |
|------|------|
| HTML | 核心导出，始终可用 |
| 打印 HTML | `print-html`，A4 打印友好模板（通过 `project-export` 使用） |
| SVG | 自包含矢量座位图，便于继续编辑 |
| PNG | 座位图图片 |
| PDF | 打印文件，系统字体智能引用 |
| XLSX | 表格文件 |
| DOCX | 可继续编辑的 Word 文档 |
| PPTX | 单页 16:9 可编辑幻灯片 |

## 使用

```bash
# 渲染 solve --output 保存的方案
seattrellis_cli export \
  --problem problem.json \
  --solution plan.json \
  --format png \
  --output outputs/plan.png

# 教师内部版（默认）或公示版
seattrellis_cli export \
  --problem problem.json \
  --solution plan.json \
  --format html \
  --template public \
  --output outputs/plan.html
```

`export` 的 `--solution` 必须是 `solve --output` 写出的结果 JSON；导出前会用独立
validator 复核方案，绝不会导出无效方案。

### 导出候选集（project 工作流）

`project-export` 渲染 `project-solve --output` 保存的方案，**不会重新求解**。
输入 candidate set 时默认导出 recommended candidate，也可以用 `--candidate`
指定：

```bash
seattrellis_cli project-solve \
  --project my-class/seattrellis.project.json \
  --candidates 3 \
  --output outputs/candidates.json

seattrellis_cli project-export \
  --project my-class/seattrellis.project.json \
  --snapshot outputs/candidates.json \
  --candidate candidate_02 \
  --format print-html \
  --output outputs/candidate-02.html
```

`project-export` 支持 `svg|html|print-html|png|pdf|xlsx|docx|pptx`，默认使用
project 的 `default_export_format`。

## 模板与隐私

导出 API 支持三种模板（`POST /api/v1/exports`）：`public`（班级公示版，只展示
座位与姓名）、`teacher`（教师内部版，可包含学生字段、规则和 warnings）、
`report`（解释报告版，包含候选评分和 hard constraint 摘要）。CLI 的
`--template` 接受 `public` 与 `teacher`。

CLI 导出一律经过隐私过滤层：默认隐藏成绩、备注、特殊需求、身高和视力字段；
`--template public` 额外匿名化姓名。`public` 模板的安全默认字段不能放宽——CLI
参数只能进一步隐藏信息，绝不会暴露敏感字段。

## 画布导出（SVG / PPTX）

SVG 和 PPTX 使用固定的 16:9 画布，输出与打印 HTML、PDF、Word 相同的模板和
隐私控制。SVG 是自包含矢量图（不含脚本或外部引用），PPTX 是单页 16:9 幻灯片，
形状均为原生可编辑对象。画布格式不接受 A4 页面方向、缩放或页边距参数。

## PDF 字体

PDF 渲染器不嵌入字体，也不依赖 WeasyPrint 或 Pango：它按固定的质量优先级链
（PingFang SC → Noto Sans CJK SC → Microsoft YaHei → SimSun → 其他 CJK 字体）
在常见平台字体目录中查找系统 CJK 字体，并在 PDF 中按名字引用，由查看器替换。
字体质量低于"推荐"档时会给出导出 warning。具体见[字体策略](font-strategy.zh.md)。

## 相关文档

- [快速开始](quickstart.zh.md)
- [Web 端使用指南](web.zh.md)
- [字体策略](font-strategy.zh.md)
