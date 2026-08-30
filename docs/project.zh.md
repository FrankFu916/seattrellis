# 班级项目（Project）工作流指南

[English](project.md) · [简体中文](project.zh.md)

**班级项目（Project）** 是 SeatTrellis 专为长期班级管理设计的本地文件工作流。它使用一个轻量级的 JSON 清单文件（`seattrellis.project.json`），将学生名单、教室布局、排座规则、历史轮换与输出目录统一组织，实现免反复配置、一键求解与全周期轮换。

---

## 🎒 1. 班级项目目录结构

一个典型的班级项目文件夹结构如下：

```text
my-class/
├── seattrellis.project.json   # 班级项目清单配置文件
├── students.csv               # 学生花名册
├── classroom.json             # 教室网格布局
├── rules.json                 # 排座规则与偏好
├── history/                   # 历史排座快照目录
│   ├── week-01.snapshot.json
│   └── week-02.snapshot.json
└── outputs/                   # 求解方案与导出文件输出目录
```

### 清单文件示例（`seattrellis.project.json`）

```json
{
  "kind": "seattrellis_project",
  "schema_version": 1,
  "name": "高一（3）班",
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

> 🔒 **相对路径与安全性**：
> 清单中引用的所有路径均严格相对于清单文件所在目录。清单文件不包含学生真实敏感信息，仅记录文件指针，可安全共享配置模板。

---

## ⚙️ 2. 项目专属子命令大全

SeatTrellis CLI 提供了一套以 `project-*` 为前缀的高效自动化工具链：

| 子命令 | 功能说明 | 典型范例 |
| :--- | :--- | :--- |
| `project-init` | 在包含名单和布局的目录中快速初始化项目清单。 | `seattrellis project-init --dir my-class` |
| `project-list` | 扫描并列出本地指定目录下的所有班级项目。 | `seattrellis project-list --root .` |
| `project-info` | 检查项目配置完整性与各个关联文件的路径状态。 | `seattrellis project-info --project my-class/seattrellis.project.json` |
| `project-validate`| 严格校验清单及其引用的所有数据文件与规则冲突。 | `seattrellis project-validate --project my-class/seattrellis.project.json --strict` |
| `project-solve` | 按照项目配置直接运行求解，生成候选方案集。 | `seattrellis project-solve --project my-class/seattrellis.project.json --candidates 3` |
| `project-rotate`| 一键推演并生成未来多期公平轮换方案（1~20 期）。 | `seattrellis project-rotate --project my-class/seattrellis.project.json --periods 4` |
| `project-edit` | 对已生成的项目方案应用交互式调整指令。 | `seattrellis project-edit --project ... --operation swap:STU01:STU02` |
| `project-repair`| 在锁定关键座位的前提下对冲突人员执行局部重排。 | `seattrellis project-repair --project ... --lock-student STU01` |
| `project-export`| 将方案渲染为指定格式（直接渲染，不重复求解）。 | `seattrellis project-export --project ... --format print-html` |
| `project-privacy`| 扫描项目及输出文件中是否存在未脱敏的敏感字段。 | `seattrellis project-privacy --project my-class/seattrellis.project.json` |
| `project-pack` | 将班级全量数据打包为 `.seattrellis.zip` 归档。 | `seattrellis project-pack --project ... --output class-backup.zip` |
| `project-restore`| 从 `.zip` 备份包完整还原班级工作区。 | `seattrellis project-restore --bundle class-backup.zip --output-dir restored/` |

---

## 🚀 3. 典型操作工作流

### 步骤一：创建并校验项目
```bash
# 初始化
seattrellis project-init --dir my-class

# 预检配置与文件依赖
seattrellis project-validate --project my-class/seattrellis.project.json
```

### 步骤二：求解与候选方案比对
```bash
# 求解并生成 3 个候选方案及比对报告
seattrellis project-solve \
  --project my-class/seattrellis.project.json \
  --candidates 3 \
  --output outputs/candidates.json \
  --report outputs/plan-report.json
```

### 步骤三：渲染导出
```bash
# 导出为 A4 横向学生公示版
seattrellis project-export \
  --project my-class/seattrellis.project.json \
  --snapshot outputs/candidates.json \
  --format print-html \
  --template public \
  --orientation landscape \
  --output outputs/wall-sheet.html
```

### 步骤四：多学期滚动轮换与归档备份
```bash
# 生成未来 4 个轮换周期
seattrellis project-rotate --project my-class/seattrellis.project.json --periods 4

# 打包备份
seattrellis project-pack --project my-class/seattrellis.project.json --output class_term1.seattrellis.zip
```

---

## 📖 相关文档

- [快速上手指南](quickstart.zh.md)
- [多格式导出与排版打印](export.zh.md)
- [Web 与桌面工作台指南](web.zh.md)
