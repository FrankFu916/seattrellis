# Project 工作流详解

## 概述

Project 文件是轻量的本地 JSON 配置文件，用于保存排座工作流的路径和默认设置。它不嵌入学生名单或座位数据，只保存相对路径和默认配置。

## 命令

```bash
seattrellis project-init   # 创建 project 文件
seattrellis project-info   # 查看配置和路径状态
seattrellis project-validate  # 校验输入文件
seattrellis project-solve     # 求解
seattrellis project-edit      # 人工微调
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

## Web 与 CLI

CLI 适合可复现脚本。Web 端既可输入 project 路径，也可上传 project JSON，
但上传模式只收到这一份 JSON，无法同时取得它引用的相对路径文件。因此上传
模式适合查看配置；校验、求解和导出应使用路径模式。

Project 文件不嵌入学生名单、历史 snapshot 或导出结果。移动项目时，需要把
这些文件和 project JSON 一起移动，并保留相对目录结构。

## 校验与输出

`project-info` 显示解析后的路径状态；`project-validate` 检查输入和规则冲突；
`project-solve` 将结果写入 `outputs_dir`；`project-edit` 可对最新产物或指定
snapshot/candidate set 执行人工微调；`project-export` 默认使用项目中的候选和导出
格式设置。命令行参数可以覆盖候选数量、seed、时间限制、候选 ID 和输出路径。

## 相关文档

- [快速开始](quickstart.zh.md)
- [Web 端使用指南](web.zh.md)
