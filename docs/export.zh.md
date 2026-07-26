# 导出格式说明

## 支持的格式

| 格式 | 命令 | 所需 Extra | 说明 |
|------|------|------------|------|
| HTML | `--format html` | 无需 extras | 核心导出，始终可用 |
| Excel | `--format excel` | `excel` | `.xlsx` 格式 |
| PNG | `--format png` | `image` | 座位图图片 |
| PDF | `--format pdf` | `pdf` | WeasyPrint 生成的打印文件 |
| Word | `--format docx` | `docx` | 可继续编辑的 `.docx` |
| 打印 HTML | `--format print-html` | 无需 extras | A4 打印友好模板 |

## 使用

```bash
# 导出普通 snapshot
seattrellis export --snapshot outputs/latest.snapshot.json --format html

# 导出 candidate set 中的推荐方案
seattrellis export --snapshot outputs/candidates.json --candidate recommended --format html --output outputs/recommended.html

# 导出指定候选方案
seattrellis export --snapshot outputs/candidates.json --candidate candidate_03 --format html

# 导出完整候选集比较报告
seattrellis export \
  --snapshot outputs/candidates.json \
  --candidate-scope all \
  --format html \
  --output outputs/candidate-comparison.html
```

## 模板与隐私

打印 HTML、PDF 和 Word 的内部导出 API 支持三种模板：

- `public`：班级公示版，只展示座位与姓名；
- `teacher`：教师内部版，可包含学生字段、规则和 warnings；
- `report`：解释报告版，包含候选评分和 hard constraint 摘要。

程序接口统一使用 `ExportRequest`、`PrivacyOptions` 和 `PageOptions`。
旧的 `PrintPrivacyOptions` 名称作为兼容别名保留。`public` 默认隐藏成绩、
备注、特殊需求、身高和视力；`teacher` 默认显示教师内部字段；`report` 默认
显示评分但隐藏学生备注和健康相关字段。

打印 HTML、PDF 和 Word 已支持 A4 横向/纵向、5–30 mm 页边距和
0.5–2.0 缩放。CLI 和 Web 均可选择模板、隐私字段、方向、缩放与中英文内容，
原有命令行和 Python 调用保持兼容。

完整候选集比较报告使用 `--candidate-scope all` 导出，目前支持 `html` 和
`print-html`。报告包含推荐方案、总分、各评分维度、hard constraint 状态、
优势、代价和历史对比摘要。它只呈现方案级聚合指标，不读取或展示姓名、学号、
学生成绩、备注、特殊需求、身高或视力；模板和字段开关不会扩大这类报告的内容。

PDF 依赖系统中文字体与 WeasyPrint 运行库，具体见 [字体策略](font-strategy.zh.md)。异形或超大教室可能需要使用浏览器打印功能手动调整缩放。

## 相关文档

- [快速开始](quickstart.zh.md)
- [Web 端使用指南](web.zh.md)
