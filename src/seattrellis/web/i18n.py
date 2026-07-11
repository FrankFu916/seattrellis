"""Small translation layer for the local Web interface."""

from __future__ import annotations

from typing import Literal

Locale = Literal["zh", "en"]

LANGUAGE_OPTIONS: dict[str, Locale] = {
    "简体中文": "zh",
    "English": "en",
}

_TEXT: dict[str, tuple[str, str]] = {
    "app_title": ("🏫 SeatTrellis · 席序", "🏫 SeatTrellis"),
    "app_caption": (
        "学生名单、规则和历史座位记录只在本机处理；请勿把真实班级数据提交到公开仓库。",
        "Student lists, rules, and seating history stay on this computer. "
        "Do not commit real class data to a public repository.",
    ),
    "skip_to_content": ("跳到主要内容", "Skip to main content"),
    "quick_tab": ("快速排座", "Quick solve"),
    "project_tab": ("Project 工作区", "Project workspace"),
    "steps": ("步骤", "Steps"),
    "step_load": ("1. 加载数据", "1. Load data"),
    "step_solve": ("2. 设置与求解", "2. Configure & solve"),
    "step_results": ("3. 查看结果与导出", "3. Review & export"),
    "load_data": ("📂 加载数据", "📂 Load data"),
    "quick_start": ("**快速体验**", "**Quick start**"),
    "load_demo": ("🚀 一键加载 Demo", "🚀 Load Demo"),
    "demo_ready": (
        "Demo 已就绪，并已选择 daily 预设。请进入下一步。",
        "The Demo is ready with the daily preset selected. Continue to the next step.",
    ),
    "demo_missing": (
        "找不到 Demo 文件。请先在终端运行 `seattrellis init-demo`。",
        "Demo files were not found. Run `seattrellis init-demo` in a terminal first.",
    ),
    "demo_caption": (
        "使用虚构示例数据体验完整流程，无需准备文件。",
        "Try the full workflow with fictional data and no files to prepare.",
    ),
    "restore_settings": ("**恢复设置**", "**Restore settings**"),
    "web_config": ("Web 配置 JSON", "Web settings JSON"),
    "settings_restored": (
        "已恢复设置：{count} 个候选，preset={preset}。",
        "Settings restored: {count} candidates, preset={preset}.",
    ),
    "none": ("无", "none"),
    "inputs_still_needed": (
        "学生名单、layout 和 history 仍需单独加载。",
        "The student list, layout, and history still need to be loaded separately.",
    ),
    "sensitive_restored_rules": (
        "此配置的 rules overlay 含有学生标识，请按敏感文件保管。",
        "This settings file contains student identifiers in its rules overlay. "
        "Treat it as sensitive.",
    ),
    "manual_upload": ("**或手动上传**", "**Or upload files**"),
    "file_help": ("📎 文件格式说明", "📎 File formats"),
    "file_help_body": (
        """
**支持的文件格式：**

| 文件 | 格式 | 大小建议 |
|---|---|---|
| 学生名单 | `.csv` / `.xlsx` / `.xlsm` | 小于 1 MB |
| 教室布局 | `.json` | 小于 500 KB |
| 规则 | `.json` | 小于 100 KB |
| 历史快照 | `.json` | 每个小于 1 MB |

不支持旧版 `.xls`，请先另存为 `.xlsx` 或 CSV。文本文件应使用 UTF-8 编码。
""",
        """
**Supported formats:**

| File | Format | Suggested size |
|---|---|---|
| Student list | `.csv` / `.xlsx` / `.xlsm` | under 1 MB |
| Classroom layout | `.json` | under 500 KB |
| Rules | `.json` | under 100 KB |
| History snapshot | `.json` | under 1 MB each |

Legacy `.xls` files are not supported; save them as `.xlsx` or CSV first. Use UTF-8 for text files.
""",
    ),
    "students_file": ("学生名单 CSV / Excel", "Student list CSV / Excel"),
    "layout_file": ("教室布局 JSON", "Classroom layout JSON"),
    "preset": ("内置场景 preset", "Built-in preset"),
    "no_preset": ("不使用 preset", "No preset"),
    "preset_help": ("📋 场景 Preset 说明", "📋 Preset guide"),
    "scenario": ("场景", "Best for"),
    "requires": ("需要", "Needs"),
    "degradation": ("缺少数据时", "When data is missing"),
    "rules_file": (
        "规则 JSON（可选；选择 preset 时作为 overlay）",
        "Rules JSON (optional; used as an overlay with a preset)",
    ),
    "history_files": (
        "历史 snapshot JSON（可选，可多选）",
        "History snapshot JSON (optional, multiple files allowed)",
    ),
    "restored_rules_in_use": (
        "当前使用配置文件中的 rules overlay：{name}",
        "Using the rules overlay from the settings file: {name}",
    ),
    "clear_restored_rules": (
        "清除已恢复的 rules overlay",
        "Clear restored rules overlay",
    ),
    "solve_settings": ("⚙️ 求解设置", "⚙️ Solve settings"),
    "inputs_required": (
        "请先上传学生名单和教室布局（两者都需要），或加载 Demo。",
        "Upload both a student list and classroom layout, or load the Demo first.",
    ),
    "resolved_rules": ("最终生效的 rules", "Resolved rules"),
    "rules_overlay": ("rules overlay", "rules overlay"),
    "download_resolved_rules": (
        "下载合并后的 rules JSON",
        "Download resolved rules JSON",
    ),
    "rules_required": (
        "请选择 preset 或上传 rules JSON。",
        "Choose a preset or upload a rules JSON file.",
    ),
    "history_quality": ("History 质量检查", "History quality check"),
    "inspect_history": ("检查历史记录", "Inspect history"),
    "snapshot_count": ("Snapshot 数量", "Snapshots"),
    "average_coverage": ("平均学生覆盖率", "Average student coverage"),
    "complete_match": ("完全匹配", "Complete matches"),
    "history_consistent": (
        "历史记录与当前学生名单和 layout 一致。",
        "History matches the current student list and layout.",
    ),
    "history_missing_students": (
        "{snapshot}：缺少 {count} 名当前学生。",
        "{snapshot}: missing {count} current students.",
    ),
    "history_unknown_students": (
        "{snapshot}：包含 {count} 名不在当前名单中的学生。",
        "{snapshot}: contains {count} students not in the current list.",
    ),
    "history_unknown_seats": (
        "{snapshot}：引用了未知座位 {seats}。",
        "{snapshot}: references unknown seats: {seats}.",
    ),
    "history_disabled_seats": (
        "{snapshot}：引用了已禁用座位 {seats}。",
        "{snapshot}: references disabled seats: {seats}.",
    ),
    "history_layout_differs": (
        "{snapshot}：layout 与当前教室不同。",
        "{snapshot}: layout differs from the current classroom.",
    ),
    "candidate_count": ("候选方案数量", "Number of candidates"),
    "custom_seed": ("自定义 seed", "Use a custom seed"),
    "time_limit": ("单次求解秒数", "Seconds per solve"),
    "sensitive_current_rules": (
        "当前 rules overlay 引用了学生标识；下载的配置文件应按敏感文件保管。",
        "The current rules overlay references student identifiers. "
        "Treat the downloaded settings file as sensitive.",
    ),
    "download_web_config": ("下载当前 Web 配置", "Download Web settings"),
    "web_config_help": (
        "保存 preset、rules overlay 和求解参数，不包含学生名单、layout 或 history。"
        "rules overlay 可能引用学生标识。",
        "Saves the preset, rules overlay, and solve settings. It excludes the "
        "student list, layout, and history. A rules overlay may contain student identifiers.",
    ),
    "generate": ("生成座位表", "Generate seating plan"),
    "solve_complete_next": (
        "求解完成。请进入“查看结果与导出”。",
        "Solve complete. Continue to Review & export.",
    ),
    "results": ("📋 结果", "📋 Results"),
    "solve_first": (
        "请先在“设置与求解”中生成座位表。",
        "Generate a seating plan in Configure & solve first.",
    ),
    "candidate_result": (
        "已生成 {count} 个候选方案，推荐 {candidate_id}。",
        "Generated {count} candidates. Recommended: {candidate_id}.",
    ),
    "single_result": ("求解完成：{status}", "Solve complete: {status}"),
    "candidate_choice": ("选择候选方案", "Choose a candidate"),
    "recommended": ("⭐ 推荐", "⭐ Recommended"),
    "seat_map": ("🏫 座位图", "🏫 Seating map"),
    "seat_map_unavailable": (
        "上传教室布局 JSON 后可预览座位图。",
        "Upload a classroom layout JSON file to preview the seating map.",
    ),
    "plan_detail": ("📊 方案详情", "📊 Plan details"),
    "total_score": ("总分", "Total score"),
    "hard_constraints": ("硬约束", "Hard constraints"),
    "passed": ("✅ 通过", "✅ Passed"),
    "violations": ("❌ {count} 项违规", "❌ {count} violations"),
    "available_dimensions": ("可用维度", "Scored dimensions"),
    "candidate_id": ("方案 ID", "Candidate ID"),
    "violation_items": ("违规项：{items}", "Violations: {items}"),
    "candidate_comparison": ("📊 候选方案对比", "📊 Compare candidates"),
    "comparison_caption": (
        "各维度为 0–100 归一化分数；n/a 表示该维度不可用。⭐ 为推荐方案。",
        "Each dimension is normalized to 0–100; n/a means unavailable. "
        "⭐ marks the recommendation.",
    ),
    "assignment_table": ("📋 分配明细表", "📋 Assignment details"),
    "repair_title": ("🛠️ 锁定与局部重排", "🛠️ Lock & repair"),
    "repair_help": (
        "选择受影响学生时，仅重新安排这些学生；不选择则在锁定条件下全局重排。",
        "Select affected students for a local repair. Leave the selection empty for a global re-solve with locks.",
    ),
    "affected_students": ("受影响学生", "Affected students"),
    "locked_students": ("锁定学生当前位置", "Keep students in current seats"),
    "locked_seats": ("锁定座位", "Locked seats"),
    "reuse_saved_locks": ("沿用快照中已保存的锁定", "Reuse locks saved in the snapshot"),
    "repair_backend": ("求解后端", "Solver backend"),
    "repair_time_limit": ("重排时限（秒）", "Repair time limit (seconds)"),
    "run_repair": ("执行局部重排", "Run repair"),
    "repair_complete": ("局部重排完成。", "Repair complete."),
    "repair_changes": ("本次调整学生：{students}", "Students moved: {students}"),
    "repair_no_changes": ("本次重排未改变座位。", "No seats changed in this repair."),
    "exports": ("📥 导出", "📥 Exports"),
    "export_settings": ("导出模板与隐私设置", "Export template and privacy"),
    "export_template": ("模板", "Template"),
    "template_public": ("班级公示版", "Public notice"),
    "template_teacher": ("教师内部版", "Teacher internal"),
    "template_report": ("方案解释报告", "Explanation report"),
    "privacy_defaults": (
        "模板的安全默认项只能进一步隐藏，不能在此处放宽。",
        "Safe template defaults can be tightened here, but not loosened.",
    ),
    "hide_scores": ("隐藏成绩", "Hide scores"),
    "hide_notes": ("隐藏备注", "Hide notes"),
    "hide_special_needs": ("隐藏特殊需求", "Hide special needs"),
    "hide_height": ("隐藏身高", "Hide height"),
    "hide_vision": ("隐藏视力信息", "Hide vision information"),
    "anonymize_names": ("匿名化姓名", "Anonymize names"),
    "page_orientation": ("A4 方向", "A4 orientation"),
    "orientation_portrait": ("纵向", "Portrait"),
    "orientation_landscape": ("横向", "Landscape"),
    "page_scale": ("页面缩放", "Page scale"),
    "export_locale": ("导出语言", "Export language"),
    "export_format": ("导出格式", "Export format"),
    "export_on_demand": (
        "非 JSON 文件会在点击生成后再创建，避免页面加载时触发可选导出依赖。",
        "Non-JSON files are created only after you click prepare, so optional "
        "export dependencies are not loaded during page rendering.",
    ),
    "export_all_candidates": (
        "导出完整候选集比较报告",
        "Export full candidate comparison report",
    ),
    "export_all_candidates_help": (
        "仅适用于 candidate set 的 HTML 和 Print HTML 导出。",
        "Available only for candidate-set HTML and Print HTML exports.",
    ),
    "export_privacy_unsupported": (
        "模板与隐私设置仅适用于 Print HTML、PDF 和 DOCX。当前格式使用基础导出，"
        "不会应用匿名化或隐藏字段选项。",
        "Template and privacy settings apply only to Print HTML, PDF, and DOCX. "
        "The selected format uses the basic exporter and does not apply "
        "anonymization or hidden-field options.",
    ),
    "prepare_export": (
        "生成 {label} 导出文件",
        "Prepare {label} export",
    ),
    "export_ready": (
        "{label} 已生成，可以下载。",
        "{label} export is ready to download.",
    ),
    "artifact_missing": (
        "结果文件已不可用，请重新求解。",
        "The result file is no longer available. Run the solve again.",
    ),
    "download": ("下载 {label}", "Download {label}"),
    "export_failed": (
        "导出 {format} 失败：{error}",
        "{format} export failed: {error}",
    ),
    "export_unavailable": (
        "导出文件不可用：{error}",
        "The exported file is unavailable: {error}",
    ),
    "project_file": ("**Project 文件**", "**Project file**"),
    "project_method": ("选择方式", "Choose a source"),
    "path": ("输入路径", "Enter a path"),
    "upload": ("上传文件", "Upload a file"),
    "project_path": ("Project 文件路径", "Project file path"),
    "project_upload": (
        "上传 Project 文件 (.seattrellis.json)",
        "Upload a Project file (.seattrellis.json)",
    ),
    "uploaded": ("已上传：{name}", "Uploaded: {name}"),
    "read_project": ("读取 project-info", "Read project info"),
    "strict_warnings": ("将 warnings 视为错误", "Treat warnings as errors"),
    "validate_project": ("校验 project", "Validate project"),
    "project_solve": ("Project 求解", "Project solve"),
    "project_default_candidates": (
        "使用 project 默认候选数量",
        "Use the Project's default candidate count",
    ),
    "project_custom_seed": ("自定义 project seed", "Use a custom Project seed"),
    "project_time_limit": ("Project 单次求解秒数", "Seconds per Project solve"),
    "solve_project": ("按 project 求解", "Solve Project"),
    "solve_complete": ("求解完成。", "Solve complete."),
    "project_results": ("📋 Project 结果", "📋 Project results"),
    "privacy_title": ("🔒 隐私提示", "🔒 Privacy"),
    "privacy_body": (
        "SeatTrellis 在本机处理数据，不会上传学生信息。临时文件保存在系统临时目录，"
        "程序退出时会清理。请妥善保管含真实学生信息的导出文件。",
        "SeatTrellis processes data on this computer and does not upload student "
        "information. Temporary files are kept in the system temporary directory "
        "and cleaned up when the app exits. Store exports containing real student "
        "information securely.",
    ),
    "empty_layout": ("教室布局中没有座位。", "No seats are defined in the layout."),
    "seat_grid_label": ("教室座位图", "Classroom seating map"),
    "seat": ("座位 {seat_id}", "Seat {seat_id}"),
    "student": ("学生：{name}", "Student: {name}"),
    "disabled": ("已禁用", "disabled"),
    "tags": ("标签：{tags}", "Tags: {tags}"),
    "near_window": ("靠窗", "near window"),
    "near_door": ("靠门", "near door"),
    "near_platform": ("讲台侧", "near platform"),
    "near_ac": ("空调下", "under air conditioner"),
    "format_error_title": ("数据格式错误", "Invalid data format"),
    "format_error_detail": (
        "输入文件不符合要求。请检查 JSON 语法、字段名称、字段类型和必填字段。\n\n原始错误：{error}",
        "The input file does not match the expected format. Check the JSON syntax, "
        "field names, types, and required fields.\n\nOriginal error: {error}",
    ),
    "file_error_title": ("文件读取失败", "Could not read the file"),
    "file_error_detail": (
        "请检查文件路径、格式和 UTF-8 编码。支持 CSV、XLSX 和 JSON。\n\n原始错误：{error}",
        "Check the file path, format, and UTF-8 encoding. CSV, XLSX, and JSON are "
        "supported.\n\nOriginal error: {error}",
    ),
    "solve_error_title": ("求解失败", "Could not generate a plan"),
    "solve_error_detail": (
        "规则可能互相冲突、启用座位不足，或硬约束无法同时满足。"
        "可先运行 `seattrellis validate` 检查输入。\n\n原始错误：{error}",
        "Rules may conflict, there may be too few enabled seats, or hard constraints "
        "cannot all be satisfied. Run `seattrellis validate` to inspect the inputs."
        "\n\nOriginal error: {error}",
    ),
    "dependency_error_title": ("缺少可选依赖", "Optional dependency missing"),
    "dependency_error_detail": (
        "此功能需要额外的 Python 包。\n\n{error}\n\n请按提示安装后重试。",
        "This feature needs an additional Python package.\n\n{error}\n\n"
        "Install the suggested extra and try again.",
    ),
    "value_error_title": ("参数错误", "Invalid setting"),
    "value_error_detail": (
        "输入参数不符合要求。\n\n原始错误：{error}",
        "A setting is invalid.\n\nOriginal error: {error}",
    ),
    "unknown_error_title": ("发生错误", "Something went wrong"),
    "unknown_error_detail": (
        "发生未预期的错误：{name}\n\n{error}",
        "An unexpected {name} error occurred.\n\n{error}",
    ),
}


def normalize_locale(locale: str | None) -> Locale:
    """Return a supported locale, defaulting to Simplified Chinese."""
    return "en" if locale == "en" else "zh"


def translate(key: str, locale: str | None = "zh", **values: object) -> str:
    """Translate one interface string and interpolate named values."""
    try:
        pair = _TEXT[key]
    except KeyError as exc:
        raise KeyError(f"Unknown Web translation key: {key}") from exc
    template = pair[1] if normalize_locale(locale) == "en" else pair[0]
    return template.format(**values)


def available_translation_keys() -> frozenset[str]:
    """Expose translation coverage for tests and documentation tooling."""
    return frozenset(_TEXT)


def table_column_labels(locale: str | None = "zh") -> dict[str, str]:
    """Return localized labels for dataframes used by the Web interface."""
    zh = {
        "candidate_id": "方案 ID",
        "recommended": "推荐",
        "total": "总分",
        "total_score": "总分",
        "hard_constraints": "硬约束",
        "fair_rotation": "公平轮换",
        "neighbors": "近期邻座",
        "recent_neighbors": "近期邻座",
        "score_balance": "成绩搭配",
        "height": "身高偏好",
        "vision": "前排需求",
        "diversity": "多样性",
        "stability": "稳定性",
        "snapshot": "Snapshot",
        "assignments": "分配数",
        "coverage": "学生覆盖率",
        "missing_students": "缺少学生",
        "unknown_students": "旧学生",
        "unknown_seats": "未知座位",
        "disabled_seats": "禁用座位",
        "layout_matches": "Layout 一致",
        "dimension": "评分维度",
        "status": "状态",
        "score": "分数",
        "weight": "权重",
        "rating": "评价",
        "student_key": "学生标识",
        "student_name": "姓名",
        "seat_id": "座位",
    }
    if normalize_locale(locale) == "zh":
        return zh
    return {
        "candidate_id": "Candidate ID",
        "recommended": "Recommended",
        "total": "Total",
        "total_score": "Total",
        "hard_constraints": "Hard constraints",
        "fair_rotation": "Fair rotation",
        "neighbors": "Recent neighbors",
        "recent_neighbors": "Recent neighbors",
        "score_balance": "Score balance",
        "height": "Height preference",
        "vision": "Front-seat needs",
        "diversity": "Diversity",
        "stability": "Stability",
        "snapshot": "Snapshot",
        "assignments": "Assignments",
        "coverage": "Student coverage",
        "missing_students": "Missing students",
        "unknown_students": "Stale students",
        "unknown_seats": "Unknown seats",
        "disabled_seats": "Disabled seats",
        "layout_matches": "Layout matches",
        "dimension": "Dimension",
        "status": "Status",
        "score": "Score",
        "weight": "Weight",
        "rating": "Rating",
        "student_key": "Student key",
        "student_name": "Name",
        "seat_id": "Seat",
    }
