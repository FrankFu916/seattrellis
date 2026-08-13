# Golden corpus 来源与冻结规则

本文是总计划 §9.1「golden fixture provenance」的 M6 前置记录。它只描述
合成测试数据与自动证据，不包含真实学生、班级或学校数据。

机器可读入口为 `fixtures/GOLDEN_PROVENANCE.json`。在最终 v1 tag 与
`v1.x-maintenance` 分支真正创建并保护之前，其中
`final_v1_reference.status` 必须保持 `pending`，`resolved_tag` 与
`resolved_commit` 必须为 `null`。不得把计划中的 tag 名称写成已经存在的证据。

## Corpus 清单

| Corpus | 角色 | 权威来源 |
|---|---|---|
| `fixtures/parity/` | 41 case 的 Python oracle inputs/goldens、迁移、历史、候选与导出语义基线 | 自带 `MANIFEST.json`；逐文件字节数与 SHA-256 |
| `fixtures/cli-goldens/` | Rust CLI 规范化 stdout/stderr/exit 字节契约；注册了 Python 命令时只比较成功/失败退出语义 | umbrella manifest 的完整文件 inventory |
| `fixtures/roster-mapping/` | 10 个合成 CSV 的 Python 映射建议与有序 issue 列表 | umbrella manifest + Python/Rust 双 guard |
| `fixtures/artifact-parity/` | compare/restore 的共享 oracle corpus | Python `tests/test_artifact_parity.py` 与 Rust `crates/seattrellis-io/tests/artifact_parity.rs` 共用 `expected.json`；动态时间先验证 RFC 3339 再规范化 |

`fixtures/parity/MANIFEST.json` 的 `source_commit` 是该次 oracle 生成记录，
不是最终 v1 tag。umbrella manifest 另行保存 corpus 的最后物质变更 commit；
两者不能替代尚未完成的最终 v1 tag/branch 冻结。

历史 CLI 与 roster-mapping 录制没有保存 Python 解释器补丁版本，因此
`recording_source.python_version` 明确为 `null`，状态分别为
`partially_recorded` 或带说明的 `commit_recorded`。逐文件 SHA-256 是这两组
既有证据的权威内容标识；不得补写无法证明的环境值。

## 完整性门禁

快速检查：

```bash
python scripts/gen_parity_fixtures.py manifest-check
```

该命令验证：

- MANIFEST case 集合与生成器定义相等；
- inputs/goldens 的路径集合、字节数与 SHA-256 双向相等；
- corpus 文件没有被 `.gitignore` 隐藏；
- CLI 与 roster-mapping inventory 未漂移；
- 最终 v1 reference 的 pending/resolved 状态自洽；
- artifact-parity 有文件后，producer、双 guard 与 recording source 不得仍为
  `pending`。

完整 oracle 重放：

```bash
python scripts/gen_parity_fixtures.py verify
```

`verify` 除上述检查外，还在临时目录重生成并对比 committed 与 fresh 两个
文件集合。两侧任何缺失或额外文件都会失败；只有明确命中墙钟截止的 snapshot
允许跳过字节比较，但路径仍必须存在于两侧。

更新 CLI/roster corpus 或新增 artifact-parity 后，经人工确认变更范围，再刷新
umbrella inventory：

```bash
python scripts/gen_parity_fixtures.py provenance
```

CLI 使用 `--cli-golden-record` 重录时会自动刷新 umbrella inventory。刷新哈希
不等于批准语义变化；相应的差分、Rust guard、ledger 证据与产品决策仍需独立
验收。

## 干净 checkout 要求

Parity 的 25 个 `history/*.snapshot.json` 是生成器产生的合成 evidence。它们曾被
仓库级 `*.snapshot.json` 规则忽略，导致 MANIFEST 记录文件而干净 checkout 缺失；
旧的单向 verify 又没有发现 fresh 侧多出的文件。现在 `.gitignore` 仅对
`fixtures/parity/inputs/*/history/*.snapshot.json` 放行，并由对称集合检查锁定。

`.DS_Store`、`Thumbs.db` 等系统元数据不属于 corpus，也不进入任何 hash
inventory。新增 fixture 前必须再次确认数据为虚构/合成；真实名单、成绩、备注、
特殊需求或学校信息一律不得进入这些目录。
