# Web 端使用指南

> ⚠️ 本文档正在完善中。当前 Web 端功能详见 [README](../README.md)。

## 启动

```bash
python -m pip install -e ".[web,excel,image]"
streamlit run src/seattrellis/web/app.py
```

## 功能概述

Web 端提供两个标签页：

- **快速排座**：上传学生名单、教室布局，选择 preset 或上传规则 JSON，上传历史 snapshot，生成座位表
- **Project workspace**：读取本地 project 文件，复用项目配置进行校验、求解和导出

## 后续计划

- 座位图可视化预览
- 候选方案交互式切换与对比
- 分步向导
- Demo 一键加载
- 更多导出格式

## 相关文档

- [快速开始](quickstart.zh.md)
- [Project 工作流详解](project.zh.md)
- [导出格式说明](export.zh.md)
