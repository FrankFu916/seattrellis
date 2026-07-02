# 故障排除

## 先运行诊断

```bash
seattrellis doctor
```

## 常见问题

### 缺少可选依赖

按错误提示安装对应 extra，例如 `.[excel]`、`.[image]`、`.[web]`、`.[pdf]` 或 `.[docx]`。

### 规则不可行

先运行 `validate`，检查 fixed seats、must/cannot adjacency、minimum distance、禁用座位和座位数量。Soft rules 不会导致 hard-rule 校验失败。

### Excel 无法读取

只支持 `.xlsx` 和 `.xlsm`；旧 `.xls` 请另存为 `.xlsx` 或 CSV。

### PDF 中文显示方块

WeasyPrint 使用系统字体。安装可用中文字体并参考[字体策略](font-strategy.zh.md)。

### Web 下载失效

重新执行求解。关闭或重启 Streamlit 后，系统临时目录中的中间文件可能已清理。

