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
```

## 模板与隐私

打印 HTML、PDF 和 Word 的内部导出 API 支持三种模板：

- `public`：班级公示版，只展示座位与姓名；
- `teacher`：教师内部版，可包含学生字段、规则和 warnings；
- `report`：解释报告版，包含候选评分和 hard constraint 摘要。

程序接口可通过 `PrintPrivacyOptions` 隐藏成绩、备注、特殊需求、身高和视力，或匿名化姓名。CLI/Web 的细粒度模板与隐私选择仍在后续计划中；当前默认使用公示版。

PDF 依赖系统中文字体与 WeasyPrint 运行库，具体见 [字体策略](font-strategy.zh.md)。异形或超大教室可能需要使用浏览器打印功能手动调整缩放。

## 相关文档

- [快速开始](quickstart.zh.md)
- [Web 端使用指南](web.zh.md)
