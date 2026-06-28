# Project 工作流详解

> ⚠️ 本文档正在完善中。当前 Project 功能详见 [快速开始](quickstart.zh.md)。

## 概述

Project 文件是轻量的本地 JSON 配置文件，用于保存排座工作流的路径和默认设置。它不嵌入学生名单或座位数据，只保存相对路径和默认配置。

## 命令

```bash
seattrellis project-init   # 创建 project 文件
seattrellis project-info   # 查看配置和路径状态
seattrellis project-validate  # 校验输入文件
seattrellis project-solve     # 求解
seattrellis project-export    # 导出
```

## Project 文件结构

```json
{
  "schema_version": 1,
  "name": "Demo Class",
  "students": "students.csv",
  "layout": "classroom.json",
  "rules": "rules.json",
  "history_dir": "history",
  "outputs_dir": "outputs",
  "default_candidates": 5,
  "default_candidate": "recommended",
  "default_export_format": "html"
}
```

所有路径相对于 project 文件所在目录解析。

## 相关文档

- [快速开始](quickstart.zh.md)
- [Web 端使用指南](web.zh.md)
