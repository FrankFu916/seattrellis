import type { Student } from "../api/types";

export type RuleDiagnosticCode =
  | "invalid_json"
  | "root_object"
  | "unknown_field"
  | "object_required"
  | "array_required"
  | "pair_shape"
  | "fixed_seat_shape"
  | "distance_value"
  | "group_shape"
  | "group_members"
  | "group_mode"
  | "unknown_student"
  | "unknown_seat"
  | "value_type";

export type RuleDiagnostic = {
  path: string;
  code: RuleDiagnosticCode;
  detail?: string;
};

const TOP_LEVEL_FIELDS = new Set(["schema_version", "seed", "hard", "soft", "groups"]);
const HARD_FIELDS = new Set([
  "fixed_seats",
  "must_be_adjacent",
  "cannot_be_adjacent",
  "min_distance",
]);
const SOFT_FIELDS = new Set([
  "vision_front",
  "height_back",
  "randomize",
  "score_balance",
  "score_position",
  "score_distribution",
  "mentor_pairing",
  "fair_rotation",
  "avoid_recent_neighbors",
  "cooling",
]);
const SOFT_FIELD_MEMBERS: Record<string, Set<string>> = {
  vision_front: new Set(["enabled", "weight"]),
  height_back: new Set(["enabled", "weight"]),
  randomize: new Set(["enabled", "weight"]),
  score_balance: new Set(["enabled", "weight"]),
  score_position: new Set(["enabled", "weight", "direction"]),
  score_distribution: new Set(["enabled", "weight", "scope"]),
  mentor_pairing: new Set([
    "enabled",
    "weight",
    "mentor_percentile",
    "learner_percentile",
    "relation",
    "avoid_recent_repeats",
    "history_lookback",
  ]),
  fair_rotation: new Set(["enabled", "weight", "avoid_repeating_categories", "lookback"]),
  avoid_recent_neighbors: new Set([
    "enabled",
    "weight",
    "relation_types",
    "lookback",
    "max_recent_count",
    "within_distance",
  ]),
  cooling: new Set(["enabled", "weight", "cooling_period", "relation_types", "within_distance"]),
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function pushUnknownFields(
  diagnostics: RuleDiagnostic[],
  object: Record<string, unknown>,
  allowed: Set<string>,
  path: string,
): void {
  for (const key of Object.keys(object)) {
    if (!allowed.has(key)) {
      diagnostics.push({
        path: path ? `${path}.${key}` : key,
        code: "unknown_field",
      });
    }
  }
}

function validatePair(
  value: unknown,
  path: string,
  diagnostics: RuleDiagnostic[],
  studentIds: Set<string>,
): void {
  if (!isRecord(value)) {
    diagnostics.push({ path, code: "pair_shape" });
    return;
  }
  const students = value.students;
  if (
    !Array.isArray(students) ||
    students.length !== 2 ||
    !students.every(isNonEmptyString)
  ) {
    diagnostics.push({ path: `${path}.students`, code: "pair_shape" });
    return;
  }
  for (const [index, student] of students.entries()) {
    if (!studentIds.has(student)) {
      diagnostics.push({
        path: `${path}.students[${index}]`,
        code: "unknown_student",
      });
    }
  }
}

function validateHardRules(
  value: unknown,
  diagnostics: RuleDiagnostic[],
  studentIds: Set<string>,
  seatIds: Set<string>,
): void {
  if (!isRecord(value)) {
    diagnostics.push({ path: "hard", code: "object_required" });
    return;
  }
  pushUnknownFields(diagnostics, value, HARD_FIELDS, "hard");
  for (const field of HARD_FIELDS) {
    const rules = value[field];
    if (rules === undefined) continue;
    if (!Array.isArray(rules)) {
      diagnostics.push({ path: `hard.${field}`, code: "array_required" });
      continue;
    }
    rules.forEach((rule, index) => {
      const path = `hard.${field}[${index}]`;
      if (field === "fixed_seats") {
        if (!isRecord(rule) || !isNonEmptyString(rule.student) || !isNonEmptyString(rule.seat_id)) {
          diagnostics.push({ path, code: "fixed_seat_shape" });
          return;
        }
        pushUnknownFields(diagnostics, rule, new Set(["student", "seat_id"]), path);
        if (!studentIds.has(rule.student)) {
          diagnostics.push({ path: `${path}.student`, code: "unknown_student" });
        }
        if (!seatIds.has(rule.seat_id)) {
          diagnostics.push({ path: `${path}.seat_id`, code: "unknown_seat" });
        }
        return;
      }
      if (isRecord(rule)) {
        pushUnknownFields(
          diagnostics,
          rule,
          field === "min_distance"
            ? new Set(["students", "distance", "metric"])
            : new Set(["students"]),
          path,
        );
      }
      validatePair(rule, path, diagnostics, studentIds);
      if (field === "min_distance" && isRecord(rule)) {
        const distance = rule.distance;
        if (typeof distance !== "number" || !Number.isFinite(distance) || distance <= 0) {
          diagnostics.push({ path: `${path}.distance`, code: "distance_value" });
        }
        if (
          rule.metric !== undefined &&
          rule.metric !== "graph" &&
          rule.metric !== "euclidean"
        ) {
          diagnostics.push({ path: `${path}.metric`, code: "distance_value" });
        }
      }
    });
  }
}

function validateSoftRules(value: unknown, diagnostics: RuleDiagnostic[]): void {
  if (!isRecord(value)) {
    diagnostics.push({ path: "soft", code: "object_required" });
    return;
  }
  pushUnknownFields(diagnostics, value, SOFT_FIELDS, "soft");
  for (const key of SOFT_FIELDS) {
    if (value[key] !== undefined && !isRecord(value[key])) {
      diagnostics.push({ path: `soft.${key}`, code: "object_required" });
    } else if (isRecord(value[key])) {
      pushUnknownFields(diagnostics, value[key], SOFT_FIELD_MEMBERS[key], `soft.${key}`);
    }
  }
}

function validateGroups(
  value: unknown,
  diagnostics: RuleDiagnostic[],
  studentIds: Set<string>,
): void {
  if (!Array.isArray(value)) {
    diagnostics.push({ path: "groups", code: "array_required" });
    return;
  }
  const names = new Set<string>();
  value.forEach((group, index) => {
    const path = `groups[${index}]`;
    if (!isRecord(group)) {
      diagnostics.push({ path, code: "group_shape" });
      return;
    }
    pushUnknownFields(diagnostics, group, new Set(["name", "students", "separate", "together"]), path);
    if (isNonEmptyString(group.name)) {
      if (names.has(group.name.trim())) {
        diagnostics.push({ path: `${path}.name`, code: "group_shape" });
      }
      names.add(group.name.trim());
    } else {
      diagnostics.push({ path: `${path}.name`, code: "group_shape" });
    }
    if (!Array.isArray(group.students) || !group.students.every(isNonEmptyString)) {
      diagnostics.push({ path: `${path}.students`, code: "group_members" });
    } else {
      if (group.students.length < 2) {
        diagnostics.push({ path: `${path}.students`, code: "group_members" });
      }
      group.students.forEach((student, studentIndex) => {
        if (!studentIds.has(student)) {
          diagnostics.push({
            path: `${path}.students[${studentIndex}]`,
            code: "unknown_student",
          });
        }
      });
    }
    if (group.separate !== undefined && typeof group.separate !== "boolean") {
      diagnostics.push({ path: `${path}.separate`, code: "group_mode" });
    }
    if (group.together !== undefined && typeof group.together !== "boolean") {
      diagnostics.push({ path: `${path}.together`, code: "group_mode" });
    }
    if (group.separate === true && group.together === true) {
      diagnostics.push({ path, code: "group_mode" });
    }
  });
}

export function diagnoseRuleSetJson(
  source: string,
  students: Student[],
  seatIds: string[],
): RuleDiagnostic[] {
  const text = source.trim();
  if (!text) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return [{ path: "$", code: "invalid_json" }];
  }
  if (!isRecord(parsed)) {
    return [{ path: "$", code: "root_object" }];
  }
  const diagnostics: RuleDiagnostic[] = [];
  const studentIds = new Set(students.map((student) => student.id));
  const knownSeatIds = new Set(seatIds);
  pushUnknownFields(diagnostics, parsed, TOP_LEVEL_FIELDS, "");
  for (const field of ["schema_version", "seed"]) {
    if (parsed[field] !== undefined && !Number.isSafeInteger(parsed[field])) {
      diagnostics.push({ path: field, code: "value_type" });
    }
  }
  if (parsed.hard !== undefined) {
    validateHardRules(parsed.hard, diagnostics, studentIds, knownSeatIds);
  }
  if (parsed.soft !== undefined) {
    validateSoftRules(parsed.soft, diagnostics);
  }
  if (parsed.groups !== undefined) {
    validateGroups(parsed.groups, diagnostics, studentIds);
  }
  return diagnostics;
}
