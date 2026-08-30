# 快速上手指南

[English](quickstart.md) · [简体中文](quickstart.zh.md)

欢迎使用 **席序（SeatTrellis）v2.0.0**！本指南将带您在 5 分钟内完成安装，并体验从名单校验、智能排座到最终导出座次表的全流程。

---

## 📦 1. 安装与运行

SeatTrellis v2 采用纯 Rust 构建，无需安装 Python、Node.js 或任何附加环境，下载即用。

### 方式 A：桌面客户端（推荐教师使用）

访问 [GitHub Releases 发布页面](https://github.com/FrankFu916/seattrellis/releases) 下载适用于您系统的安装包：

- **macOS**（Apple Silicon）：下载 `.dmg` 镜像或 `.app.tar.gz` 压缩包。
- **Windows**（x64）：下载 `.msi` 安装包或 NSIS `.exe` 安装程序。
- **Linux**（amd64）：下载 `.deb` 软件包。

> 💡 **系统提示说明**：安装包默认未购买商业签名证书。macOS 首次打开若提示无法打开，请在“访达”中右键点击应用并选择“打开”；Windows 若弹出 SmartScreen 拦截提示，点击“更多信息”并选择“仍要运行”即可。

---

### 方式 B：命令行工具（推荐自动化与高级用户）

通过 Rust 官方包管理器一键安装：

```bash
cargo install seattrellis
```

安装完成后，运行 `seattrellis --help` 查看所有命令，或运行环境自检：

```bash
seattrellis doctor
```
> `doctor` 命令将自动检查当前二进制版本、Core API 版本以及系统临时目录的读写权限。

---

### 方式 C：启动本地 Web 工作台

如果您习惯在浏览器中操作，可通过命令行启动仅监听本地回环地址（`127.0.0.1:8765`）的轻量服务：

```bash
seattrellis_web --open-browser
```
服务启动后将自动在您的默认浏览器中打开交互式排座工作台。

---

## ⚡ 2. 命令行 3 分钟上手

在命令行中，SeatTrellis 围绕一个结构清晰的**问题描述文件**（`problem.json`）展开工作。该文件将学生信息、教室网格、排座规则及参数集中管理。

### 示例数据 `problem.json`

```json
{
  "api_version": 2,
  "student_count": 4,
  "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
  "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
  "fixed_seats": [[0, 0]],
  "seed": 42,
  "students": [
    {"key": "STU001", "display_name": "张三"},
    {"key": "STU002", "display_name": "李四"},
    {"key": "STU003", "display_name": "王五"},
    {"key": "STU004", "display_name": "赵六"}
  ],
  "rules": {
    "seed": 42,
    "soft": {
      "randomize": { "enabled": true, "weight": 1 }
    }
  }
}
```

### 核心三步操作

```bash
# 第一步：数据与规则预检（不运行求解计算）
seattrellis validate --problem problem.json

# 第二步：智能求解，将完整的排座快照写入 plan.json
seattrellis solve --problem problem.json --output plan.json

# 第三步：将生成的方案渲染导出为高清晰度 PNG 图片
seattrellis export --problem problem.json --solution plan.json --format png --output plan.png
```

---

## 🛠️ 3. 常用排座场景与进阶命令

### 3.1 求解与可复现性

```bash
# 固定随机种子（Seed），确保排座结果 100% 确定可复现
seattrellis solve --problem problem.json --seed 42 --output outputs/latest.snapshot.json

# 设定最长运算时间（秒）；超时时若已找到合法方案仍会输出 Solved
seattrellis solve --problem problem.json --time-limit 3 --output outputs/latest.snapshot.json
```

---

### 3.2 方案审计与细分打分

```bash
# 规则与可行域预检，快速定位冲突原因
seattrellis precheck --problem problem.json

# 全面审计已求解方案：硬约束合规检查与软偏好打分明细
seattrellis audit --problem problem.json --solution plan.json

# 对指定的座次分配进行即时打分分析
seattrellis score --problem problem.json --assignment '[[0,0],[1,1],[2,2],[3,3]]'
```

---

### 3.3 一键生成多套备选方案

有时老师希望对比不同的排座风格。使用 `candidates` 命令可以一次性生成多个均满足所有硬约束的高质量候选方案：

```bash
seattrellis candidates --problem problem.json --count 5 > outputs/candidates.json
```
系统会基于综合偏好得分自动标记“最佳推荐方案”，并输出各个候选方案的多样性与稳定性对比。

---

### 3.4 交互式手工微调与局部智能修复

排座完成后，如果个别同学需要临时调动，可以通过 `edit` 命令进行精准调整：

```bash
seattrellis edit \
  --snapshot outputs/latest.snapshot.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R1C1 \
  --output outputs/edited.snapshot.json
```

支持的操作指令：
- `swap:STU001:STU002`：互换两名同学的座位；
- `move:STU003:R2C2`：将同学移至指定空座；
- `lock-student:STU001` / `lock-seat:R1C1`：锁定同学当前位置或锁定指定座位；
- `batch-move:STU001=R1C2,STU002=R1C1`：批量位置调整。

若微调后产生了局部冲突，可使用 `repair` 命令在保持锁定座位不变的前提下，仅对冲突学生进行局部重排：

```bash
seattrellis repair \
  --problem problem.json \
  --snapshot outputs/edited.snapshot.json \
  --lock-student STU001 \
  --affected STU002 \
  --output outputs/repaired.snapshot.json
```

---

### 3.5 历史轮换与同桌关系分析

席序能够跨学期追踪历史座次，避免学生连续坐在偏僻位置或长期与同一人同桌：

```bash
# 生成每位学生的历史位置分布统计报告（前排/后排/靠窗/靠门/靠空调等）
seattrellis history-report --problem problem.json --history-dir examples/history --output outputs/history-report.json

# 分析两两学生之间的历史同桌与相邻频次
seattrellis pair-report --problem problem.json --history-dir examples/history --top 10
```

---

## 🎒 4. 班级项目（Project）工作流

对于长期管理的班级，推荐使用 Project 工作流，将名单、教室布局、历史记录统一归档：

```bash
# 1. 在包含名单和规则的目录下初始化班级项目
seattrellis project-init --dir my-class

# 2. 检查项目配置与文件引用
seattrellis project-info --project my-class/seattrellis.project.json

# 3. 求解班级座位并生成 3 个候选方案
seattrellis project-solve --project my-class/seattrellis.project.json --candidates 3 --output outputs/project.plan.json

# 4. 导出为可打印的 HTML 格式
seattrellis project-export --project my-class/seattrellis.project.json --snapshot outputs/project.plan.json --format print-html --output outputs/seat.html

# 5. 一键生成未来 4 个时段的公平轮换计划
seattrellis project-rotate --project my-class/seattrellis.project.json --periods 4

# 6. 项目全量打包备份与跨机器迁移恢复
seattrellis project-pack --project my-class/seattrellis.project.json --output my-class.seattrellis.zip
seattrellis project-restore --bundle my-class.seattrellis.zip --output-dir restored/
```

---

## 📖 深入探索

- 📐 **[排座规则手册](rules.zh.md)**：深入了解所有硬约束、软偏好加权与算法逻辑。
- 🖥️ **[Web 与桌面工作台指南](web.zh.md)**：掌握可视化界面、多期轮换与排版导出的全部操作。
- 📄 **[数据格式参考](input-format.zh.md)**：查看学生名单、教室布局与快照文件的详细字段规范。
- ⚙️ **[CLI 命令完整手册](cli.md)**：查阅 27 个子命令的完整参数与退出码规范。
