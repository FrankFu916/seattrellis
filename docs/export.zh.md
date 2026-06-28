# 导出格式说明

> ⚠️ 本文档正在完善中。当前导出功能详见 [快速开始](quickstart.zh.md)。

## 支持的格式

| 格式 | 命令 | 所需 Extra | 说明 |
|------|------|------------|------|
| HTML | `--format html` | 无需 extras | 核心导出，始终可用 |
| Excel | `--format excel` | `excel` | `.xlsx` 格式 |
| PNG | `--format png` | `image` | 座位图图片 |

## 使用

```bash
# 导出普通 snapshot
seattrellis export --snapshot outputs/latest.snapshot.json --format html

# 导出 candidate set 中的推荐方案
seattrellis export --snapshot outputs/candidates.json --candidate recommended --format html --output outputs/recommended.html

# 导出指定候选方案
seattrellis export --snapshot outputs/candidates.json --candidate candidate_03 --format html
```

## 后续计划

- PDF 导出
- Word 导出
- 打印友好模板
- 班级公示版 / 教师内部版 / 解释报告版模板

## 相关文档

- [快速开始](quickstart.zh.md)
- [Web 端使用指南](web.zh.md)
