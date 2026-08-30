# 常见问题与故障排查（Troubleshooting）

[English](troubleshooting.md) · [简体中文](troubleshooting.md)

在使用 **席序（SeatTrellis）** 过程中遇到异常时，本指南可帮助您快速定位并解决问题。

---

## 🩺 1. 第一步：运行环境自检

遇到异常时，首先运行 `doctor` 命令诊断运行环境：

```bash
seattrellis doctor
```

该命令将自动检测：
- 当前 CLI 二进制文件完整性与版本号；
- Core 算法引擎 API 版本契约；
- 操作系统临时目录（Temp Dir）的读写权限。

---

## ❓ 2. 常见问题与解决方案

### Q1: 提示“无法找到可行解（Infeasible / Unknown）”

**排查建议**：
1. **可用座位不足**：检查教室启用的有效座位数（`enabled: true`）是否小于学生总人数；
2. **硬约束冲突**：
   - 运行 `seattrellis validate --problem problem.json` 检查是否存在逻辑冲突（如两人既“必须同桌”又“禁止相邻”）；
   - 运行 `seattrellis precheck --problem problem.json` 查看每个学生的候选座位域是否为空；
3. **固定座位冲突**：检查被固定座位的学生是否与其他学生的“禁止相邻”或“最小间距”产生矛盾；
4. **软偏好与硬约束的区分**：请注意，软偏好（如视力靠前、身高靠后）绝不会导致求解无解；导致无解的只有 **硬约束（Hard Constraints）**。

---

### Q2: Excel 花名册导入失败

**可能原因与对策**：
- **旧版 `.xls` 格式**：目前原生解析器支持标准的 `.xlsx` 与 `.xlsm` 格式。如果是老旧的 Excel 97-2003（`.xls`）文件，请先在 Excel 或 WPS 中另存为 `.xlsx` 或 `.csv`；
- **加密或受密码保护**：系统出于安全限制，不解析带有密码保护的工作簿；
- **公式单元格无缓存值**：如果 Excel 中包含动态公式，请确保在 Excel 中保存一次，使公式生成缓存计算结果；
- **多 Sheet 问题**：导入器默认读取工作簿的**第一个工作表（Sheet 1）**，请确保学生名单位于第一张表。

---

### Q3: 导出的 PDF 或 PNG 图片中缺少中文字符（显示为空白或告警）

**原因分析**：
PDF 与 PNG 导出使用纯 Rust 引擎在本地进行文字光栅化绘制，依赖操作系统本地安装的中文字体。若在极简 Linux 服务器或 Docker 容器中运行，可能缺失中文字体包。

**解决方案**：
在系统上安装思源黑体（Noto Sans CJK）：
```bash
# Debian / Ubuntu 环境：
sudo apt-get install -y fonts-noto-cjk

# CentOS / RHEL / Fedora 环境：
sudo yum install -y google-noto-sans-cjk-fonts
```
安装后重新执行导出命令即可。详情请参考 [中文字体策略](font-strategy.zh.md)。

---

### Q4: Web 工作台无法启动或提示端口占用

**原因分析**：
`seattrellis_web` 默认监听本地 `127.0.0.1:8765` 端口。如果该端口已被其他程序占用，启动会失败。

**解决方案**：
通过 `--port` 参数指定其他可用端口：
```bash
seattrellis_web --port 8766 --open-browser
```

若从源码构建运行，请确保已预先编译前端静态资源：
```bash
cd clients/web && npm ci && npm run build && cd ../..
```

---

### Q5: 历史文件迁移报错

**说明**：
系统仅支持将 v1 版本的学生花名册（`student_roster`）、教室布局（`classroom_layout`）和班级项目清单（`seattrellis_project`）迁移至 v2。若传入未知类型或未来更高版本的文件，系统将主动拦截以防止数据损坏。

---

## 📖 相关文档

- [CLI 命令行手册](cli.md)
- [排座规则手册](rules.zh.md)
- [输入数据格式规范](input-format.zh.md)
- [中文字体策略](font-strategy.zh.md)
