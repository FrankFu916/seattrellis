import type {
  AdvancedSolveSettings,
  CommonConstraint,
  CommonPreferenceId,
  CustomRoomSettings,
  GenerateClassRequest,
  HardRulesPayload,
  Student,
} from "../api/types";

export type AdvancedSettingErrorKind = "rules" | "layout" | "seed";

export class InvalidAdvancedSettingError extends Error {
  readonly kind: AdvancedSettingErrorKind;

  constructor(kind: AdvancedSettingErrorKind) {
    super(`Invalid advanced ${kind} setting.`);
    this.name = "InvalidAdvancedSettingError";
    this.kind = kind;
  }
}

function parseJsonObject(
  source: string,
  kind: "rules" | "layout",
): Record<string, unknown> | undefined {
  const text = source.trim();
  if (!text) {
    return undefined;
  }
  try {
    const parsed: unknown = JSON.parse(text);
    if (
      parsed === null ||
      typeof parsed !== "object" ||
      Array.isArray(parsed)
    ) {
      throw new InvalidAdvancedSettingError(kind);
    }
    return parsed as Record<string, unknown>;
  } catch (error) {
    if (error instanceof InvalidAdvancedSettingError) {
      throw error;
    }
    throw new InvalidAdvancedSettingError(kind);
  }
}

function parseIntegerList(source: string, max: number): Set<number> {
  const result = new Set<number>();
  for (const item of source.split(/[\s,，]+/u).filter(Boolean)) {
    const value = Number(item);
    if (!Number.isInteger(value) || value < 1 || value > max) {
      throw new InvalidAdvancedSettingError("layout");
    }
    result.add(value);
  }
  return result;
}

function parseDisabledSeats(source: string, rows: number, columns: number): Set<string> {
  const result = new Set<string>();
  for (const item of source.split(/[\s,，]+/u).filter(Boolean)) {
    const match = /^(\d+)[-:]([0-9]+)$/u.exec(item);
    if (!match) {
      throw new InvalidAdvancedSettingError("layout");
    }
    const row = Number(match[1]);
    const column = Number(match[2]);
    if (row < 1 || row > rows || column < 1 || column > columns) {
      throw new InvalidAdvancedSettingError("layout");
    }
    result.add(`${row}-${column}`);
  }
  return result;
}

function buildGridLayout(settings: CustomRoomSettings): Record<string, unknown> {
  if (!Number.isInteger(settings.rows) || settings.rows < 1 || settings.rows > 30) {
    throw new InvalidAdvancedSettingError("layout");
  }
  if (
    !Number.isInteger(settings.columns) ||
    settings.columns < 1 ||
    settings.columns > 30
  ) {
    throw new InvalidAdvancedSettingError("layout");
  }
  const aisleColumns = parseIntegerList(settings.aisleColumns, settings.columns);
  const disabledSeats = parseDisabledSeats(
    settings.disabledSeats,
    settings.rows,
    settings.columns,
  );
  const seats: Array<Record<string, unknown>> = [];
  for (let row = 1; row <= settings.rows; row += 1) {
    for (let column = 1; column <= settings.columns; column += 1) {
      const aisle = aisleColumns.has(column);
      const disabled = disabledSeats.has(`${row}-${column}`);
      const enabled = !aisle && !disabled;
      seats.push({
        seat_id: enabled
          ? `R${row}C${column}`
          : aisle
            ? `AISLE-R${row}C${column}`
            : `BLOCKED-R${row}C${column}`,
        row,
        col: column,
        x: column,
        y: row,
        enabled,
        zone: aisle ? "aisle" : row === 1 ? "front" : row === settings.rows ? "back" : "middle",
        near_platform: row === 1,
        tags: [],
        attributes: disabled ? { disabled_by_teacher: true } : {},
      });
    }
  }
  return {
    layout_id: "custom-grid",
    name: "Custom classroom",
    seats,
    adjacency: {
      include_horizontal: true,
      include_vertical: false,
      include_diagonal: false,
      max_row_delta: 1,
      max_col_delta: 1,
      use_xy_distance: true,
      custom_edges: [],
    },
    metadata: { source: "react-room-builder" },
  };
}

function buildHardRules(constraints: CommonConstraint[]): HardRulesPayload | undefined {
  const fixed_seats: Array<{ student: string; seat_id: string }> = [];
  const must_be_adjacent: Array<{ students: [string, string] }> = [];
  const cannot_be_adjacent: Array<{ students: [string, string] }> = [];
  for (const constraint of constraints) {
    if (constraint.kind === "fixed_seat" && constraint.first && constraint.seatId) {
      fixed_seats.push({ student: constraint.first, seat_id: constraint.seatId });
    } else if (constraint.kind === "must_adjacent" && constraint.first && constraint.second) {
      must_be_adjacent.push({ students: [constraint.first, constraint.second] });
    } else if (constraint.kind === "avoid_adjacent" && constraint.first && constraint.second) {
      cannot_be_adjacent.push({ students: [constraint.first, constraint.second] });
    }
  }
  if (!fixed_seats.length && !must_be_adjacent.length && !cannot_be_adjacent.length) {
    return undefined;
  }
  return { fixed_seats, must_be_adjacent, cannot_be_adjacent };
}

function buildRulesOverlay(
  preferences: CommonPreferenceId[],
): Record<string, unknown> | undefined {
  const soft: Record<string, unknown> = {};
  for (const preference of preferences) {
    if (preference === "vision_front") soft.vision_front = { enabled: true };
    if (preference === "height_back") soft.height_back = { enabled: true };
    if (preference === "fair_rotation") soft.fair_rotation = { enabled: true };
    if (preference === "avoid_recent_neighbors") {
      soft.avoid_recent_neighbors = { enabled: true };
    }
    if (preference === "score_position") {
      soft.score_position = { enabled: true, direction: "high_front" };
    }
    if (preference === "score_distribution") {
      soft.score_distribution = { enabled: true, scope: "row" };
    }
    if (preference === "mentor_pairing") {
      soft.mentor_pairing = { enabled: true };
    }
  }
  return Object.keys(soft).length ? { soft } : undefined;
}

export function buildGenerateClassRequest({
  className,
  students,
  selectedRoomId,
  selectedGoalId,
  settings,
  roomSettings,
  constraints,
  preferences,
}: {
  className: string;
  students: Student[];
  selectedRoomId: string;
  selectedGoalId: string;
  settings: AdvancedSolveSettings;
  roomSettings: CustomRoomSettings;
  constraints: CommonConstraint[];
  preferences: CommonPreferenceId[];
}): GenerateClassRequest {
  const customRules = parseJsonObject(settings.customRulesJson, "rules");
  const customLayout = roomSettings.enabled
    ? roomSettings.layoutJson.trim()
      ? parseJsonObject(roomSettings.layoutJson, "layout")
      : buildGridLayout(roomSettings)
    : undefined;
  const hardRules = buildHardRules(constraints);
  const rulesOverlay = buildRulesOverlay(preferences);
  const seedText = settings.seed.trim();
  const seed = seedText === "" ? undefined : Number(seedText);
  if (seed !== undefined && !Number.isSafeInteger(seed)) {
    throw new InvalidAdvancedSettingError("seed");
  }

  return {
    draft: {
      name: className,
      students: students.map((student) => ({
        student_id: student.id,
        name: student.name,
        gender: student.gender,
        height_cm: student.heightCm,
        score: student.score,
        vision: student.vision,
        tags: student.tags,
        needs: student.needs,
        notes: student.notes,
        attributes: student.attributes,
      })),
      room: customLayout
        ? { layout: customLayout }
        : { template_id: selectedRoomId },
      goal: {
        ...(customRules
          ? { goal_id: "custom", custom_rules: customRules }
          : { goal_id: selectedGoalId }),
        ...(hardRules ? { hard_rules: hardRules } : {}),
        ...(rulesOverlay ? { rules_overlay: rulesOverlay } : {}),
      },
    },
    options: {
      candidate_count: settings.candidateCount,
      seed,
      time_limit_seconds: settings.timeLimitSeconds,
      backend: settings.backend,
    },
  };
}
