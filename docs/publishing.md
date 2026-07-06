# 发布与可信发布

SeatTrellis 使用 GitHub Actions OIDC Trusted Publishing，不在仓库中保存
PyPI API token。

## 一次性配置

仓库管理员需要分别在 TestPyPI 和 PyPI 创建 pending publisher：

- Owner：`FrankFu916`
- Repository：`seattrellis`
- Workflow：`publish.yml`
- TestPyPI environment：`testpypi`
- PyPI environment：`pypi`

GitHub 仓库中应存在同名 environments。建议给 `pypi` environment 添加人工
审批；`testpypi` 可允许维护者手动运行。

## TestPyPI

在 Actions 中手动运行 `Publish distributions`，选择 `testpypi`。工作流会：

1. 在干净环境构建 wheel 和 sdist；
2. 运行 `twine check`；
3. 检查归档内没有私有目录、`.env`、`.DS_Store` 或非示例 snapshot；
4. 通过 OIDC 发布到 TestPyPI；
5. 从 TestPyPI 轮询下载刚发布的 wheel，在干净 runner 中安装并运行
   `seattrellis --version` 和 `seattrellis --help`。

必要时也可以在本地全新虚拟环境中复核：

```bash
seattrellis --version
seattrellis --help
```

## PyPI

创建 GitHub Release 后，`release.published` 事件会使用 `pypi` environment 发布
同一构建产物。工作流会强制校验 `pyproject.toml`、运行时
`seattrellis.__version__` 与 `v<version>` release tag 完全一致；Release 标题仍需
在发布前人工核对。

如果 Trusted Publisher 尚未配置，发布 job 会失败且不会回退到长期 token。

## 失败发布与回滚

- 构建、归档检查、OIDC 或 TestPyPI 安装验证失败时，不创建正式 Release，也不
  重复使用已上传的版本号；修复后递增预发布版本再验证。
- PyPI 文件不可覆盖或原地替换。若正式版本有缺陷，应立即在 PyPI 标记为
  yanked、在 GitHub Release 中注明影响范围，并发布递增的修复版本。
- 回滚应用行为通过发布新的 patch 版本完成，不删除既有 tag，不重写已发布
  wheel/sdist，也不把发布流程降级为长期 API token。
- 若怀疑 OIDC 配置或工作流被篡改，先禁用 GitHub environment、移除对应
  Trusted Publisher，再完成审计和恢复。
