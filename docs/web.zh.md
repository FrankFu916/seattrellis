# Web 端使用指南

## 启动

```bash
python -m pip install -e ".[web,excel,image,pdf,docx]"
streamlit run src/seattrellis/web/app.py
```

## 功能概述

Web 端提供两个标签页：

- **快速排座**：按“加载数据 → 配置与求解 → 查看结果”三步完成排座。支持一键 Demo、CSV/XLSX 学生名单、layout、preset、rules overlay 和多份历史 snapshot。
- **Project workspace**：通过本机路径或上传 project JSON，复用项目配置进行校验、求解和导出。

## 结果与导出

多候选结果可以切换预览，并在同一张表中比较总分、hard constraints 和七个评分维度。座位图、评分明细和分配表会随候选切换同步更新。

页面可下载 snapshot/candidate set、plan report、HTML、PDF、PNG、Excel 和 Word。缺少 optional extra 时会显示对应安装提示，不影响其他格式。

## 隐私与临时文件

所有求解均在本机完成。快速排座使用系统临时目录保存中间文件，并把下载所需 JSON 保存在当前 Streamlit 会话中。不要把真实学生数据、截图或导出文件提交到公开仓库。

## 当前限制

- 尚未提供 rules overlay 合并结果预览。
- 尚未提供 history 覆盖率与 layout 一致性报告。
- 尚未提供配置下载恢复、多语言切换和拖拽 layout 编辑。

## 相关文档

- [快速开始](quickstart.zh.md)
- [Project 工作流详解](project.zh.md)
- [导出格式说明](export.zh.md)
