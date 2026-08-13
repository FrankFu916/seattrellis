# 故障排除

## 先运行诊断

```bash
seattrellis_cli doctor
```

`doctor` 会打印二进制名、版本、core API 版本并探测临时目录是否可写（不可写时
以退出码 2 失败）。`seattrellis_cli --version` 可单独查看版本。

## 常见问题

### 规则不可行

先运行 `validate`（或 project 的 `project-validate`），检查 fixed seats、
must/cannot adjacency、minimum distance、禁用座位和座位数量。Soft rules 不会
导致 hard-rule 校验失败。注意退出码 `3` 表示确认不可行，`5` 表示未知
（启发式未找到方案），两者含义不同。

### Excel 无法读取

只支持 `.xlsx` 和 `.xlsm`；旧 `.xls` 请另存为 `.xlsx` 或 CSV。

### PDF 中文显示方块

v2 的 PDF 渲染器按名字引用系统 CJK 字体，由查看器替换。如果系统中没有 CJK
字体（常见于精简 Linux/服务器环境），请安装 `fonts-noto-cjk`（Debian/Ubuntu）
或 `google-noto-sans-cjk-fonts`（CentOS/RHEL），并参考[字体策略](font-strategy.zh.md)。

### Web 工作台无法启动

`seattrellis_app` 默认绑定 `127.0.0.1:8765`。端口被占用时换一个端口：

```bash
seattrellis_app --port 8766 --open-browser
```

工作台需要嵌入的前端资源；开发构建时先执行 `cd clients/web && npm ci && npm run build`。

### 迁移报错

v1 时代的文件由 `schema-migrate` 或项目面板迁移，迁移前自动创建 `.bak` 备份。
未知版本会给出明确的迁移提示；失败时不会破坏原文件。
