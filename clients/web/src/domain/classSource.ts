import type { ClassSource } from "../api/types";

type ObjectValue = Record<string, unknown>;
const object = (value: unknown): value is ObjectValue => value !== null && typeof value === "object" && !Array.isArray(value);
const strings = (value: unknown): value is string[] => Array.isArray(value) && value.every((item) => typeof item === "string");
const finite = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);
const integer = (value: unknown, min: number, max: number) => finite(value) && Number.isSafeInteger(value) && value >= min && value <= max;
const optional = (value: unknown, validate: (value: unknown) => boolean) => value === undefined || value === null || validate(value);
function student(value: unknown): boolean {
  return object(value) && typeof value.id === "string" && typeof value.name === "string" &&
    optional(value.gender, (v) => typeof v === "string") && optional(value.heightCm, finite) && optional(value.score, finite) &&
    optional(value.vision, (v) => typeof v === "string" || finite(v)) && optional(value.tags, strings) && optional(value.needs, strings) &&
    optional(value.notes, (v) => typeof v === "string") && optional(value.attributes, object);
}

/** Validate the persisted UI source before it can become render state. Core
 * source validation on the server does not validate this separate UI model. */
export function readClassSource(value: unknown): ClassSource {
  function invalid(): never { throw new Error("Invalid class source"); }
  if (!object(value)) return invalid();
  for (const key of ["name", "selectedRoomId", "selectedGoalId"]) if (typeof value[key] !== "string") return invalid();
  if (!optional(value.selectedFileName, (v) => typeof v === "string") || !Array.isArray(value.students) || !value.students.every(student) ||
      !integer(value.sourceRevision, 0, Number.MAX_SAFE_INTEGER) || !optional(value.generatedSourceRevision, (v) => integer(v, 0, Number.MAX_SAFE_INTEGER)) ||
      !integer(value.activeRotationPeriod, 1, 20) || !strings(value.historyFileNames) || !Array.isArray(value.historySnapshots) || !value.historySnapshots.every(object)) return invalid();
  const advanced = value.advancedSettings;
  if (!object(advanced) || !integer(advanced.candidateCount, 1, 20) || typeof advanced.seed !== "string" ||
      !finite(advanced.timeLimitSeconds) || advanced.timeLimitSeconds < 0.1 || advanced.timeLimitSeconds > 300 || typeof advanced.customRulesJson !== "string") return invalid();
  const room = value.roomSettings;
  if (!object(room) || typeof room.enabled !== "boolean" || !integer(room.rows, 1, 30) || !integer(room.columns, 1, 30) ||
      ![room.aisleColumns, room.disabledSeats, room.layoutJson].every((v) => typeof v === "string")) return invalid();
  const rotation = value.rotationSettings;
  if (!object(rotation) || typeof rotation.enabled !== "boolean" || !integer(rotation.periodCount, 1, 20) || typeof rotation.periodLabels !== "string") return invalid();
  const detailed = value.detailedRules;
  if (!object(detailed) || typeof detailed.enabled !== "boolean") return invalid();
  const numericFields: Record<string, string[]> = {
    fairRotation: ["weight", "lookback"], avoidRecentNeighbors: ["weight", "lookback", "maxRecentCount", "withinDistance"],
    cooling: ["weight", "coolingPeriod", "withinDistance"], scorePosition: ["weight"], scoreDistribution: ["weight"],
    mentorPairing: ["weight", "mentorPercentile", "learnerPercentile", "historyLookback"],
  };
  for (const [name, fields] of Object.entries(numericFields)) {
    const rule = detailed[name];
    if (!object(rule) || typeof rule.enabled !== "boolean" || fields.some((field) => !finite(rule[field]))) return invalid();
  }
  for (const name of ["avoidRecentNeighbors", "cooling"]) if (!strings((detailed[name] as ObjectValue).relationTypes)) return invalid();
  if (!["high_front", "high_back"].includes(String((detailed.scorePosition as ObjectValue).direction)) ||
      !["row", "group"].includes(String((detailed.scoreDistribution as ObjectValue).scope)) ||
      typeof (detailed.mentorPairing as ObjectValue).relation !== "string" || typeof (detailed.mentorPairing as ObjectValue).avoidRecentRepeats !== "boolean") return invalid();
  if (!Array.isArray(value.constraints) || !value.constraints.every((v) => object(v) &&
      ["avoid_adjacent", "must_adjacent", "fixed_seat", "min_distance"].includes(String(v.kind)) &&
      [v.id, v.first, v.second, v.seatId].every((field) => typeof field === "string") && finite(v.distance) &&
      ["graph", "euclidean"].includes(String(v.metric)) && optional(v.enabled, (field) => typeof field === "boolean"))) return invalid();
  if (!Array.isArray(value.groups) || !value.groups.every((v) => object(v) && typeof v.id === "string" && typeof v.name === "string" &&
      ["together", "separate"].includes(String(v.mode)) && strings(v.students) && optional(v.enabled, (field) => typeof field === "boolean"))) return invalid();
  const preferenceIds = new Set(["vision_front", "height_back", "fair_rotation", "avoid_recent_neighbors", "score_position", "score_distribution", "mentor_pairing"]);
  if (!strings(value.preferences) || value.preferences.some((id) => !preferenceIds.has(id))) return invalid();
  if (!Array.isArray(value.scratchAssignments) || !value.scratchAssignments.every((v) => object(v) && typeof v.seatId === "string" &&
      integer(v.row, 0, Number.MAX_SAFE_INTEGER) && integer(v.column, 0, Number.MAX_SAFE_INTEGER) && typeof v.locked === "boolean" && optional(v.student, student))) return invalid();
  if (value.generationRepro !== undefined) {
    const repro = value.generationRepro;
    if (!object(repro) || typeof repro.seed !== "string" || typeof repro.solver !== "string" || !finite(repro.timeLimitSeconds) || !integer(repro.historyCount, 0, Number.MAX_SAFE_INTEGER)) return invalid();
  }
  if (value.exportSettings !== undefined) {
    const settings = value.exportSettings;
    if (!object(settings) || typeof settings.format !== "string" || typeof settings.anonymize !== "boolean" || typeof settings.showStudentIds !== "boolean" ||
      !["portrait", "landscape"].includes(String(settings.orientation)) || !["a4", "a3", "letter"].includes(String(settings.paper)) || !finite(settings.margin)) return invalid();
  }
  return value as ClassSource;
}
