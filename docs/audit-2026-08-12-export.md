# SeatTrellis v2 — 导出模块（seattrellis-export）审计报告

> 日期：2026-08-12
> 范围：`crates/seattrellis-export/src/`（render.rs、print_html.rs、office.rs、fonts.rs、export.rs）
> 基线：修复前 `cargo +1.88.0 test -p seattrellis-export` 53 单测 + 6 集成 + 2 fuzz 全绿
> 结论：**修复后 61 单测 + 6 集成 + 2 fuzz 全绿，clippy -D warnings 干净**；共报告 12 项（1 阻断、4 重要、4 次要、3 建议），其中 5 项已最小修复并附回归测试，7 项仅记录。
> 关联：ledger §19.26/§19.27（PDF 144 DPI 光栅页、PNG 2x、print-html 独立版式、Office OOXML 中文/东亚字体增强）。

## 0. 方法

- 逐文件精读五个源文件；对照核心 crate 的校验边界（`validate_solve_request` / `validate_solve_response`，仅检查坐标 finiteness，不检查量级）与模型定义（`Seat.row/col` 为裸 i32）确认可达性。
- 用临时复现测试（已删除）逐项实证：XLSX/print-html key 泄漏、lang 属性注入、i32 溢出 panic、`.seats` 死 CSS、行中空洞错位全部复现。
- 修复仅限本 crate；未触碰 `crates/seattrellis-cli`（其他 agent 进行中：`commands.rs:1253` / `main.rs:1330` 有与其 WIP 相关的编译错误，与本审计无关）、`fixtures/`、`scripts/`、`clients/web`。

---

## 1. 阻断（Blocker）

### B-1 print-html 座位网格从未生效——CSS 规则挂在死类上
- 位置：`crates/seattrellis-export/src/print_html.rs:92`（CSS）与 `:281` 起（`html_seat_table` 生成的标记）
- 问题：CSS 声明 `grid-template-columns` 的类是 `.seats`，但整个文档**没有任何 `class="seats"` 元素**（已实证：`has class="seats": false`）。实际标记是 `<div class="grid-row">…</div>` 包裹每行座位，而 `.grid-row` **没有任何 CSS 规则**。因此所有座位 div 退化为普通块级元素：每行座位**纵向堆叠**成整页宽的长条，而不是按列排布的网格——print-html（v2 主打打印/贴墙格式，M5-A2）输出结构上不可用，与版式规范（§3.2「网格：按布局的行列数铺满可用面积」）直接冲突。
- 复现：任意 2×2 网格导出 print-html，检查 `<style>` 与 `<body>`：`grid-template-columns` 只出现在死规则里。
- 修复（已做）：把模板列声明从 `.seats` 移到实际包裹座位的 `.grid-row`。最小改动，无 API 变化。
- 回归测试：`seat_grid_css_targets_the_row_wrapper`。

## 2. 重要（Major）

### M-1 极端但有限的坐标/布局行值 → i32 减法溢出 panic（debug）/ 2^32 次迭代挂起（release）
- 位置：`render.rs:235`（`grid_cols`/`grid_rows` 的 `max_col - min_col` i32 减法）、`render.rs:105` 起各渲染器 `for row in min_row..=max_row` 循环
- 问题：`validate_solve_request` 只要求坐标有限（`engine.rs:1207`），`Seat.row/col` 是裸 i32。构造 `seat_positions: [[1e300,1e300],[-1e300,-1e300]]`（或 layout 里 `row: i32::MAX`/`i32::MIN`）可通过全部校验，`round() as i32` 饱和到 i32 极值 → `grid_cols` 减法溢出：debug 构建直接 panic（已复现：`attempt to subtract with overflow` at render.rs:212），release 构建回绕后所有渲染器对 2^32 个网格位置迭代，等效挂起；SVG/HTML 还会构造 2^32 单元格的字符串（内存耗尽）。导出 API 可达（客户端提交 request+response，仅过 `validate_solve_response`）。
- 修复（已做，三处最小改动）：
  1. `grid_rows`/`grid_cols` 改用 i64 减法（消除溢出 panic）；
  2. `SeatingGrid::build` 增加范围护栏：行列各 ≤ 10_000 且网格单元总数 ≤ 10_000，超限返回结构化错误（`grid extent … is too large to render`）——所有格式共用同一入口，顺带把 SVG/HTML 输出、四个渲染器循环都限制在可承受量级（≤10^8 次比较），并覆盖了 XLSX 16384 列（Excel 上限）问题；
  3. `render_png` 在分配像素缓冲**之前**检查 `width*height*3 > 512 MiB` 直接报错（2x 密度下 1000×100 房间约需 8.5 GiB），把 OOM abort 变成干净的错误。PDF 画布恒为页尺寸，不受影响。
- 回归测试：`build_rejects_pathological_grid_extent`、`png_rejects_oversized_raster_instead_of_allocating`。
- 备注：直接构造 `SeatingGrid`（绕过 `build`）的调用方不受护栏约束，但 crate 的 API 路径（`render_export` → `build`）已全部覆盖；`grid_rows/cols` 的 i64 化保证直接构造也不 panic。

### M-2 匿名化/公开导出泄漏 student_key（学号类标识符）
- 位置：`export.rs:436` `anonymize_grid`
- 问题：`anonymize_grid` 只把姓名换成占位符，**保留 `student_key`**。而 `xlsx_assignments_sheet`（office.rs:174）把 `student_key` 逐行写入 "Assignments" 表，print-html 在 `show_student_ids=true` 时把 key 渲染进 `.sid` 跨度。实证：`template:"public"` 的 XLSX 的 sheet2.xml 含 `20260001`（姓名列已是"学生"占位符）；公开 print-html 含真实 key。学生 key 通常是学号——公开模板本应"不含任何学生信息"，这是隐私泄漏（四格式一致性：svg/html/png/pdf 不渲染 key，只有 Office/print-html 泄漏）。
- 修复（已做）：`anonymize_grid` 将 `student_key` 置为 `None`（一行），并更新注释说明。
- 回归测试：`anonymized_exports_carry_no_student_keys`（XLSX 逐 zip 部件断言 + print-html 断言）、`teacher_exports_keep_student_keys`（teacher 模板行为不变）。

### M-3 print-html `lang` 属性未转义 → 导出 HTML 属性注入
- 位置：`print_html.rs:119`（`lang = options.locale`）
- 问题：`locale` 从导出请求 JSON 直插双引号属性 `<html lang="{lang}">`，未经过 `html_escape`。构造 `locale: 'zh" onmouseover="alert(1)'` 可逃逸属性注入事件处理器（已复现）；导出的 HTML 会被用户在浏览器中打开，属注入面。其余插值（title/meta/name/key/truncated names）均已转义，此为本文件唯一漏网点。
- 修复（已做）：`lang = html_escape(&options.locale)`。
- 回归测试：`locale_attribute_is_escaped`。

### M-4 print-html 行中空洞使后续座位左移错位
- 位置：`print_html.rs:291`（`html_seat_table` 对 `None` 单元格 `continue`）
- 问题：整列缺失会走"过道"分支（位置保留），但**行中空洞**（该列在其他行有座位、本行没有）时 `None` 位置直接跳过，不产生任何元素。CSS grid 自动放置下，本行后续座位全部左移一轨。实证：第 2 行只有 col 1、col 3 两个座位时，col 3 的 E 被渲染到第 2 轨；而 svg/html/png/pdf 均用空槽（void 单元格/骨架）保留位置——打印图表与实际教室错位，与其余三格式语义不一致。
- 修复（已做）：`None` 位置发出空填充 `<div></div>`（不可见，仅占轨）。
- 回归测试：`mid_row_hole_keeps_column_alignment`。

## 3. 次要（Minor）

### m-1 PPTX 超宽网格产生负 ext（无效 OOXML）
- 位置：`office.rs:470`（`cell_w - 10_000`）
- 问题：超过约 1098 列时 10_000 EMU 间隙大于格宽，`a:ext cx` 为负，PowerPoint 会报修复/拒绝。extent 护栏（10k 列上限）内仍可达。
- 修复（已做）：`(cell_w - 10_000).max(0)` 钳制为非负。
- 回归测试：`pptx_shape_extents_never_go_negative`（1100 列网格 + quick-xml 良构断言）。

### m-2 过道判定与 SVG 语义不一致（仅含禁用座位的列）
- 位置：`print_html.rs:165`（`occupied_cols` 只收集 `enabled` 座位）
- 问题：一列若只有禁用座位（`enabled=false`），被判为"过道"，每行渲染"过道"标签，禁用座位本身不显示；SVG/HTML 渲染器在该位置显示"unused"灰格。语义分歧、打印图上丢失禁用座信息。
- 处理：仅报告，未改（属版式设计取舍，需产品确认；修法是把 `occupied_cols` 收集条件改为"任意座位"）。

### m-3 fonts.rs 文档过时 + macOS 26 字体发现回退
- 位置：`fonts.rs:1-8`（模块注释仍称"PDF 按名引用系统字体"——实际已是导出时光栅化，PD-D12 R2）；`fonts.rs:56`（`/System/Library/Fonts/PingFang.ttc` 在新系统已迁移，本机实测回退到 `STHeiti Light.ttc`，质量仍为 Preferred，非 bug）。
- 处理：仅报告。注释建议同步为"光栅化 + 质量链发现"。

### m-4 渲染器对网格位置的线性查找 O(单元数²)
- 位置：`render.rs` `cell_at`、`print_html.rs:291` `grid.cells.iter().find(...)`
- 问题：每个网格位置都线性扫描全部单元格。extent 护栏（10k 单元）下最坏约 10^8 次比较/格式，秒级可接受；超过护栏已被拒绝。真实教室（≤数百座位）无感。
- 处理：仅报告。建议后续在 `build` 里建 `HashMap<(row,col), idx>`（改动 `SeatingGrid` 结构，属 API 变更，故未做）。

## 4. 建议（Suggestion）

### s-1 PDF 两个 stream 的 `/Length` 口径不一致（均正确，仅记录）
- `render.rs:869` 内容流 `/Length` 不含结尾换行（符合 ISO 32000-1 7.3.8.1：endstream 前的 EOL 不计入），`render.rs:877` 图像流 `/Length` 含 `>` EOD 与换行（ASCIIHexDecode 在 `>` 停止，多余空白被忽略）。两种口径各对应其解码器语义，现有 reader 与 xref 测试全部通过；建议后续统一为"数据字节精确长度"以消除误读风险。

### s-2 RunLength 回退路径缺少端到端覆盖
- `pdf_compress_image` 只在 zlib 失败时走 RunLength；单测覆盖了编码器 round-trip（`pdf_run_length_encoding_handles_literals_and_repeats`）与解码器逻辑，但没有强制触发回退分支的集成用例。建议加一个"Flate 失败 → RLE + ASCIIHex 仍可被解析器解码"的测试。

### s-3 直接构造 `PrintHtmlOptions` 时 `page_scale`/`margin_mm` 可传 NaN
- `compute_layout` 里 `options.page_scale.clamp(0.5, 2.0)` 对 NaN 返回 NaN，会生成非法 CSS（`NaNmm`）。API 路径（`render_export` → `validate_page_scale`/`validate_margin_mm`）已拒绝非有限值，仅内部直接构造有风险；建议 `render_print_html` 入口再钳制一次。

## 5. 已核查无误的区域（供回归参考）

- **PDF 结构**：对象 1–5 的 xref 偏移、trailer、`startxref`、`%%EOF` 逐字节校验测试通过；`/Filter [/ASCIIHexDecode /FlateDecode]`、`/Length`、MediaBox、内容流 `cm`/`Do` 映射与 144 DPI 光栅页自洽；空网格被 `build` 拒绝，超大网格被 extent 护栏限制，画布恒为页尺寸。
- **RLE 编码**：128 上限、字面量/重复段切换、EOD=128，round-trip 测试通过。
- **SVG/HTML/Office 转义**：`render.rs::escape_text` 与 `office.rs::xml_escape` 均覆盖 `& < > " '` 并剥离 XML 1.0 非法控制字符；`]]>` 序列因 `>` 转义而安全；print-html 全部文本插值（title/meta/name/key/截断名单）已转义。
- **XLSX**：`excel_column` 26 进制正确（A…Z、AA…）；inlineStr 与 `xml:space="preserve"` 一致；Assignments 行号连续；zip 条目齐全，quick-xml 独立解析 + 差分 harness 的 openpyxl/python-docx/python-pptx 独立 reader 均通过。
- **字体**：本机实测 fontdue 可解析 TTC（STHeiti Light.ttc），CJK 字形（"张"）光栅化正常；无字体机器降级为无文字 PNG/PDF，不 panic。

## 6. 修复清单与验证

| # | 修复 | 文件 | 回归测试 |
|---|---|---|---|
| B-1 | 网格模板列声明移至 `.grid-row` | print_html.rs | `seat_grid_css_targets_the_row_wrapper` |
| M-1 | extent 护栏 + i64 网格维度 + PNG 缓冲上限 | render.rs | `build_rejects_pathological_grid_extent`、`png_rejects_oversized_raster_instead_of_allocating` |
| M-2 | 匿名化时清除 `student_key` | export.rs | `anonymized_exports_carry_no_student_keys`、`teacher_exports_keep_student_keys` |
| M-3 | `lang` 属性转义 | print_html.rs | `locale_attribute_is_escaped` |
| M-4 | 空洞位置发填充 div 保位 | print_html.rs | `mid_row_hole_keeps_column_alignment` |
| m-1 | PPTX 形状尺寸非负钳制 | office.rs | `pptx_shape_extents_never_go_negative` |

验证结果：
- `cargo +1.88.0 test -p seattrellis-export`：61 单测 + 6 集成 + 2 fuzz，0 失败（基线 53+6+2）。
- `cargo +1.88.0 clippy --all-targets -p seattrellis-export -- -D warnings`：干净；`cargo fmt` 已应用。
- 依赖 crate（seattrellis-application、seattrellis-server）编译通过；seattrellis_cli 的编译错误位于 `commands.rs:1253`/`main.rs:1330`（HistoryReportArgs/PairReportArgs 等），属其他 agent 进行中的改动，与本审计改动无关。
- 未 git commit / push；未触碰 fixtures/、scripts/、clients/web/、crates/seattrellis-cli/。
