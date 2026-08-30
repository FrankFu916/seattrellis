# Input Formats & Data Schemas

[English](input-format.md) · [简体中文](input-format.zh.md)

**SeatTrellis v2.0.0** operates around three core inputs: the **Student Roster**, the **Classroom Layout**, and the **RuleSet**.

Whether managed through a unified project workspace (`*.project.json`) or embedded into a standalone `problem.json` for CLI execution, all inputs undergo strict local schema validation.

---

## 👥 1. Student Rosters (CSV / Excel)

SeatTrellis provides native Rust parsers for standard `.csv` and Excel `.xlsx` / `.xlsm` workbooks without external Python or Office dependencies.

### Field Definitions & Header Mapping

| Field Name | Key | Type | Requirement | Description & Examples |
| :--- | :--- | :--- | :--- | :--- |
| **Student ID** | `student_id` | String | Recommended | Stable unique identifier (e.g., `STU001`). If omitted, `name` is used as internal ID. |
| **Full Name** | `name` | String | Core | Display name of the student (e.g., `Alice Smith`). |
| **Gender / Group** | `gender` | String | Optional | Grouping or gender marker (e.g., `M` / `F`). |
| **Height (cm)** | `height_cm` / `height` | Positive Float | Optional | Physical height in cm, used for sightline-based ordering (e.g., `168.5`). |
| **Academic Score** | `score` | Numeric | Optional | Academic grade or weighted score for peer-mentorship mixing (e.g., `92.0`). |
| **Vision Needs** | `vision` / `vision_score` | String/Numeric | Optional | Visual accommodation indicators (e.g., `poor`, `0.6`, `needs_front`). |
| **Custom Tags** | `tags` | String | Optional | Comma/semicolon/pipe-separated tags (e.g., `monitor, team_lead`). |
| **Special Accommodations**| `needs` | String | Optional | Accommodation notes (e.g., `near_door, front_row`). |
| **Internal Notes** | `notes` | String | Optional | Teacher reference notes. |

### Excel (.xlsx / .xlsm) Ingestion Boundaries
- **First Worksheet Only**: Only Sheet 1 is parsed.
- **Text & Leading Zeros Preserved**: Identifiers like `001` or `042` remain text strings and are never coerced to numbers.
- **Formula Results**: Cached formula results are supported; formula cells lacking cached values trigger an explicit error.
- **File Safety Caps**: Maximum file size is 20 MiB, with a cap of 10,000 student rows and 256 columns.

---

## 🏫 2. Classroom Layout JSON (`layout.json`)

Classrooms are modeled as a flexible topology of discrete seat nodes, supporting irregular, L-shaped, and non-rectangular rooms:

```json
{
  "layout_id": "class-room-a",
  "name": "Classroom 3-B",
  "seats": [
    { "seat_id": "R1C1", "row": 1, "col": 1, "enabled": true },
    { "seat_id": "R1C2", "row": 1, "col": 2, "enabled": false, "zone": "aisle" },
    { "seat_id": "R1C3", "row": 1, "col": 3, "enabled": true, "near_window": true }
  ],
  "adjacency": {
    "include_horizontal": true,
    "include_vertical": false,
    "include_diagonal": false,
    "custom_edges": []
  }
}
```

### Seat Node Attributes

| Property | Type | Description |
| :--- | :--- | :--- |
| `seat_id` | String | Required. Unique desk code (e.g., `R1C1`, `DESK-12`). |
| `row` / `col` | Positive Integer | Required. 1-indexed row and column coordinates. |
| `enabled` | Boolean | Optional. Defaults to `true`; `false` marks aisles, columns, or disabled seats. |
| `zone` | String | Optional. Zone label (e.g., `front`, `middle`, `back`, `aisle`). |
| `near_window` / `near_door` / `near_ac` / `near_platform` | Boolean | Optional. Flags proximity to windows, doors, AC units, or the teacher's podium. |
| `group_id` | String/Integer | Optional. Group affiliation for team-based layouts. |

---

## 🎒 3. Class Project Workspace (`seattrellis.project.json`)

Organizes class resources using relative paths for portable, long-term archiving:

```json
{
  "kind": "seattrellis_project",
  "schema_version": 1,
  "name": "Grade 10 Class 3",
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

- **Relative Paths**: Resolved relative to the project file directory for zero-friction sharing.
- **Privacy by Design**: Project manifests contain file references only, keeping student data safely decoupled.

---

## 📸 4. Historical Snapshots (`*.snapshot.json`)

Historical snapshots enable multi-term fairness and desk-mate variation algorithms:

```json
{
  "schema_version": 2,
  "snapshot_id": "2026-term1-w01",
  "timestamp": "2026-09-01T08:00:00Z",
  "assignment": {
    "STU001": "R1C1",
    "STU002": "R1C3"
  },
  "metrics": {
    "solved": true,
    "score": 92.5
  }
}
```

- Loaded sequentially by timestamp or filename order (`examples/history/*.snapshot.json`).
- If a student is absent in older snapshots (e.g., newly transferred), the solver skips them gracefully with a diagnostic notice.

---

## 📖 Related Documentation

- [Rule Handbook](rules.en.md)
- [Quick Start Guide](quickstart.en.md)
- [Class Project Workflow](project.md)
