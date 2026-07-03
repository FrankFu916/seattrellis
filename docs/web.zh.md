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

左侧栏的“语言 / Language”可在简体中文和英文之间切换。切换语言只改变界面
文字，不会清空已加载的数据、当前步骤或求解结果。

## Rules 预览

进入“设置与求解”后，页面会显示 preset 与 rules overlay 合并后的完整
`RuleSet`。可以在求解前核对 hard rules、权重和 seed，也可以下载合并后的
JSON 留档。

## History 质量检查

上传 history 后可以运行质量检查。报告按 snapshot 显示：

- 当前学生覆盖率；
- 缺失学生和不在当前名单中的学生；
- 未知座位和已禁用座位；
- snapshot layout 是否与当前 layout 一致。

Demo 会自动加载 `examples/history/` 中的虚构历史记录。

## 保存和恢复设置

“下载当前 Web 配置”会保存 preset、rules overlay、候选数量、seed 和时间限制。
该文件不包含学生名单、layout、history、路径或求解结果。需要注意，fixed
seat、pair rule 和 group 等规则可能引用学生 ID；页面检测到这类字段时会显示
隐私提示。恢复配置后仍需加载本次使用的数据文件。

## 结果与导出

多候选结果可以切换预览，并在同一张表中比较总分、hard constraints 和七个评分维度。座位图、评分明细和分配表会随候选切换同步更新。

页面可下载 snapshot/candidate set、plan report、HTML、PDF、PNG、Excel 和 Word。缺少 optional extra 时会显示对应安装提示，不影响其他格式。

## 隐私与临时文件

所有求解均在本机完成。快速排座使用系统临时目录保存中间文件，并把下载所需 JSON 保存在当前 Streamlit 会话中。不要把真实学生数据、截图或导出文件提交到公开仓库。

## 键盘与小屏使用

- 按 Tab 可依次访问上传、选择、求解和下载控件，焦点位置会用蓝色轮廓标出。
- 页面开头提供“跳到主要内容”链接，键盘用户可以略过侧栏和重复导航。
- 座位图中的启用座位可以用 Tab 聚焦；辅助技术会读出座位、学生和位置标签。
- 窄屏下并排控件会自动改为纵向排列，按钮保留至少 44 像素的触控高度。
- 操作系统开启“减少动态效果”后，页面会停用非必要动画。

## 当前限制

- 尚未提供拖拽 layout 编辑。
- Streamlit 的表格在很窄的手机屏幕上仍可能需要横向滚动。
- 当前只提供简体中文和英文。

## 相关文档

- [快速开始](quickstart.zh.md)
- [Project 工作流详解](project.zh.md)
- [导出格式说明](export.zh.md)
