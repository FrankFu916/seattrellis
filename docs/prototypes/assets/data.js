/* ---------------------------------------------------------------------------
 * SeatTrellis v2 M4 prototype gallery — demo data.
 *
 * Students, seats and rules are extracted verbatim from the golden parity
 * corpus fixture `fixtures/parity/inputs/data-unicode/` (students.csv,
 * classroom.json, rules.json) so every mockup renders real roster data.
 *
 * `plans`, `history` and `pair_history` are demo-only reconstructions used
 * to illustrate UX variants; they are not solver outputs.
 * ------------------------------------------------------------------------- */

window.PT = (function () {
  "use strict";

  // --- students: fixtures/parity/inputs/data-unicode/students.csv -----------
  var students = [
    { id: "STU001", name: "学生丁001", gender: "F", height: 154, score: 91.6, vision: null, tags: ["leader"], needs: ["vision_front"], notes: null },
    { id: "STU002", name: "学生丂002", gender: "M", height: 173, score: 76.3, vision: null, tags: [], needs: [], notes: null },
    { id: "STU003", name: "学生七003", gender: "F", height: 131, score: 61.0, vision: null, tags: [], needs: [], notes: null },
    { id: "STU004", name: "学生丄004", gender: "M", height: 180, score: 97.5, vision: null, tags: [], needs: [], notes: null },
    { id: "STU005", name: "学生丅005", gender: "F", height: 142, score: 79.0, vision: "0.6", tags: [], needs: [], notes: null },
    { id: "STU006", name: "学生丆006", gender: "M", height: 161, score: 96.8, vision: null, tags: [], needs: [], notes: "特别关注" },
    { id: "STU007", name: "学生万007", gender: "F", height: 171, score: 60.7, vision: null, tags: [], needs: [], notes: null },
    { id: "STU008", name: "学生丈008", gender: "M", height: 175, score: 66.4, vision: null, tags: [], needs: [], notes: null },
    { id: "STU009", name: "学生三009", gender: "F", height: 151, score: 55.1, vision: null, tags: [], needs: [], notes: null },
    { id: "STU010", name: "学生上010", gender: "M", height: 150, score: 57.1, vision: "0.6", tags: ["leader"], needs: [], notes: null },
    { id: "STU011", name: "学生下011", gender: "F", height: 146, score: 61.3, vision: "poor", tags: [], needs: [], notes: null },
    { id: "STU012", name: "学生丌012", gender: "M", height: 167, score: 77.7, vision: null, tags: [], needs: [], notes: "特别关注" },
    { id: "STU013", name: "学生不013", gender: "F", height: 147, score: 75.4, vision: null, tags: [], needs: [], notes: null },
    { id: "STU014", name: "学生与014", gender: "M", height: 173, score: 96.2, vision: null, tags: [], needs: [], notes: null },
    { id: "STU015", name: "学生丏015", gender: "F", height: 164, score: 72.0, vision: "0.6", tags: [], needs: [], notes: null },
    { id: "STU016", name: "学生丐016", gender: "M", height: 138, score: 79.6, vision: null, tags: [], needs: [], notes: null },
    { id: "STU017", name: "学生丑017", gender: "F", height: 150, score: 82.8, vision: null, tags: [], needs: [], notes: null },
    { id: "STU018", name: "学生丒018", gender: "M", height: 130, score: 77.9, vision: null, tags: [], needs: [], notes: "特别关注" },
    { id: "STU019", name: "学生专019", gender: "F", height: 186, score: 71.9, vision: null, tags: [], needs: [], notes: null },
    { id: "STU020", name: "学生一020", gender: "M", height: 131, score: 90.1, vision: "0.6", tags: [], needs: [], notes: null }
  ];

  var byId = {};
  students.forEach(function (s) { byId[s.id] = s; });

  // --- seats: fixtures/parity/inputs/data-unicode/classroom.json ------------
  // 5 rows x 4 columns; rows 1-5 map front -> back. Demo shorthand keeps the
  // zone/window/door/platform semantics of the source document.
  var rows = 5, cols = 4;
  var seats = [];
  var zones = ["front", "middle", "middle", "back", "back"];
  for (var r = 1; r <= rows; r++) {
    for (var c = 1; c <= cols; c++) {
      seats.push({
        id: "R" + r + "C" + c, row: r, col: c, zone: zones[r - 1],
        near_platform: r === 1, near_door: c === 4, near_window: c === 1
      });
    }
  }
  var seatById = {};
  seats.forEach(function (s) { seatById[s.id] = s; });

  // --- rules: fixtures/parity/inputs/data-unicode/rules.json ----------------
  var rules = {
    seed: 168996,
    hard: {
      fixed_seats: [{ seat_id: "R1C1", student: "STU001" }]
    },
    soft: {
      vision_front: { enabled: true, weight: 20 }
    }
  };

  // --- demo plans (not solver output; for UX mockups only) ------------------
  // Plan A: the "current" saved plan. Plan B: an alternative candidate with
  // four seats swapped (STU003/STU009/STU013/STU014).
  var planA = {
    id: "cand-4", label: "方案 A · 推荐", seed: 168996,
    assignment: {
      R1C1: "STU001", R1C2: "STU005", R1C3: "STU010", R1C4: "STU011",
      R2C1: "STU006", R2C2: "STU015", R2C3: "STU020", R2C4: "STU012",
      R3C1: "STU013", R3C2: "STU016", R3C3: "STU017", R3C4: "STU018",
      R4C1: "STU002", R4C2: "STU003", R4C3: "STU014", R4C4: "STU009",
      R5C1: "STU004", R5C2: "STU007", R5C3: "STU008", R5C4: "STU019"
    }
  };
  var planB = {
    id: "cand-5", label: "方案 B", seed: 168997,
    assignment: {
      R1C1: "STU001", R1C2: "STU005", R1C3: "STU010", R1C4: "STU011",
      R2C1: "STU006", R2C2: "STU015", R2C3: "STU020", R2C4: "STU012",
      R3C1: "STU014", R3C2: "STU016", R3C3: "STU017", R3C4: "STU018",
      R4C1: "STU002", R4C2: "STU009", R4C3: "STU003", R4C4: "STU013",
      R5C1: "STU004", R5C2: "STU007", R5C3: "STU008", R5C4: "STU019"
    }
  };
  var planC = {
    id: "cand-6", label: "方案 C", seed: 168998,
    assignment: {
      R1C1: "STU005", R1C2: "STU001", R1C3: "STU010", R1C4: "STU011",
      R2C1: "STU006", R2C2: "STU015", R2C3: "STU020", R2C4: "STU012",
      R3C1: "STU013", R3C2: "STU016", R3C3: "STU017", R3C4: "STU018",
      R4C1: "STU002", R4C2: "STU003", R4C3: "STU014", R4C4: "STU009",
      R5C1: "STU004", R5C2: "STU007", R5C3: "STU008", R5C4: "STU019"
    }
  };

  // --- demo PlanScore breakdown (dimension names match seattrellis_core) ----
  var scoresA = {
    fair_rotation: 82.4, avoid_recent_neighbors: 91.0, score_balance: 76.5,
    height_back: 88.2, vision_front: 96.8, diversity: 70.1, stability: 85.6,
    total: 84.4
  };
  var scoresB = {
    fair_rotation: 78.1, avoid_recent_neighbors: 86.3, score_balance: 81.2,
    height_back: 85.0, vision_front: 96.8, diversity: 74.6, stability: 79.3,
    total: 82.9
  };
  var scoresC = {
    fair_rotation: 79.0, avoid_recent_neighbors: 88.0, score_balance: 77.1,
    height_back: 86.4, vision_front: 78.2, diversity: 71.0, stability: 83.0,
    total: 81.5
  };

  var hardSummary = {
    all_satisfied: true, checked_rule_count: 1, violation_count: 0,
    witnesses: []
  };
  var hardSummaryBroken = {
    all_satisfied: false, checked_rule_count: 1, violation_count: 2,
    witnesses: [
      { rule: "fixed_seats", detail: "学生丁001 (STU001) 应固定于 R1C1，实际在 R1C2" },
      { rule: "min_distance", detail: "学生万007 (STU007) 与 学生丈008 (STU008) 距离 1 < 要求 2" }
    ]
  };

  // --- demo rotation history (3 periods) ------------------------------------
  var history = [
    { period: 1, label: "第 1 期", date: "2026-02-17",
      moved: ["STU001", "STU005", "STU011", "STU015", "STU004", "STU019"] },
    { period: 2, label: "第 2 期", date: "2026-03-03",
      moved: ["STU002", "STU006", "STU010", "STU014", "STU007", "STU008"] },
    { period: 3, label: "第 3 期 · 当前", date: "2026-03-17",
      moved: ["STU003", "STU009", "STU013", "STU016", "STU012", "STU020"] }
  ];

  var pairHistory = [
    { a: "STU002", b: "STU014", periods: 2, last: "第 2 期", note: "连续 2 期同桌" },
    { a: "STU007", b: "STU008", periods: 1, last: "第 1 期", note: "近期相邻" },
    { a: "STU004", b: "STU019", periods: 1, last: "第 2 期", note: "近期相邻" }
  ];

  // --- demo diagnostics (violations for the broken plan C) ------------------
  var diagnostics = [
    {
      severity: "error", rule: "fixed_seats", title: "固定座位规则被违反",
      desc: "学生丁001（STU001）被要求固定于 R1C1，但当前方案将其安排在 R1C2。",
      fix: { action: "move_back", label: "移回固定座位", args: { student: "STU001", seat: "R1C1" } }
    },
    {
      severity: "error", rule: "min_distance", title: "学生间距不足",
      desc: "学生万007（STU007）与学生丈008（STU008）距离为 1，低于规则要求的 2。",
      fix: { action: "swap", label: "建议交换", args: { student: "STU008", seat: "R4C4" } }
    },
    {
      severity: "warning", rule: "vision_front", title: "视力需求未充分满足",
      desc: "2 名标记为「需要前排」的学生（STU010、STU020）当前位于第 2 排之后。",
      fix: { action: "auto_fix", label: "自动修复", args: {} }
    },
    {
      severity: "info", rule: "history", title: "未提供历史快照",
      desc: "fair_rotation / avoid_recent_neighbors 维度因缺少历史而无法评估。",
      fix: { action: "add_history", label: "添加历史", args: {} }
    }
  ];

  // --- misc demo data -------------------------------------------------------
  var classInfo = {
    name: "初二（3）班", teacher: "李老师", room: "教学楼 302", students: students.length
  };

  var rosterPreview = [
    { row: 2, cells: ["STU011", "学生下011", "F", "146", "61.3", "poor", "—", "—"] },
    { row: 3, cells: ["STU012", "学生丌012", "M", "167", "77.7", "", "—", "特别关注"] },
    { row: 4, cells: ["STU013", "学生不013", "F", "147", "75.4", "", "—", "—"] },
    { row: 5, cells: ["STU014", "学生与014", "M", "173", "96.2", "", "—", "—"] },
    { row: 6, cells: ["STU015", "学生丏015", "F", "164", "72.0", "0.6", "—", "—"] },
    { row: 7, cells: ["STU016", "学生丐016", "M", "138", "79.6", "", "—", "—"] }
  ];

  return {
    students: students, byId: byId, seats: seats, seatById: seatById,
    rules: rules, rows: rows, cols: cols,
    planA: planA, planB: planB, planC: planC,
    scoresA: scoresA, scoresB: scoresB, scoresC: scoresC,
    hardSummary: hardSummary, hardSummaryBroken: hardSummaryBroken,
    history: history, pairHistory: pairHistory,
    diagnostics: diagnostics, classInfo: classInfo, rosterPreview: rosterPreview
  };
})();
