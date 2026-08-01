import type {
  AdvancedSolveSettings,
  CommonConstraint,
  CommonGroupRule,
  CommonPreferenceId,
  CustomRoomSettings,
  DetailedRuleSettings,
  GenerateClassRequest,
  GenerateRotationPlanRequest,
  RotationSettings,
  HardRulesPayload,
  HistorySnapshotPayload,
  Student,
} from "../api/types";

export type AdvancedSettingErrorKind = "rules" | "layout" | "seed" | "rotation";

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

export function buildGridLayout(settings: CustomRoomSettings): Record<string, unknown> {
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
  const min_distance: Array<{
    students: [string, string];
    distance: number;
    metric: "euclidean" | "graph";
  }> = [];
  for (const constraint of constraints) {
    if (constraint.kind === "fixed_seat" && constraint.first && constraint.seatId) {
      fixed_seats.push({ student: constraint.first, seat_id: constraint.seatId });
    } else if (constraint.kind === "must_adjacent" && constraint.first && constraint.second) {
      must_be_adjacent.push({ students: [constraint.first, constraint.second] });
    } else if (constraint.kind === "avoid_adjacent" && constraint.first && constraint.second) {
      cannot_be_adjacent.push({ students: [constraint.first, constraint.second] });
    } else if (
      constraint.kind === "min_distance" &&
      constraint.first &&
      constraint.second &&
      Number.isFinite(constraint.distance) &&
      constraint.distance > 0
    ) {
      min_distance.push({
        students: [constraint.first, constraint.second],
        distance: constraint.distance,
        metric: constraint.metric,
      });
    }
  }
  if (
    !fixed_seats.length &&
    !must_be_adjacent.length &&
    !cannot_be_adjacent.length &&
    !min_distance.length
  ) {
    return undefined;
  }
  return { fixed_seats, must_be_adjacent, cannot_be_adjacent, min_distance };
}

function validateDetailedRules(settings: DetailedRuleSettings): void {
  if (!settings.enabled) {
    return;
  }
  const nonNegativeIntegers = [
    settings.fairRotation.weight,
    settings.fairRotation.lookback,
    settings.avoidRecentNeighbors.weight,
    settings.avoidRecentNeighbors.lookback,
    settings.avoidRecentNeighbors.maxRecentCount,
    settings.cooling.weight,
    settings.cooling.coolingPeriod,
    settings.cooling.withinDistance,
    settings.scorePosition.weight,
    settings.scoreDistribution.weight,
    settings.mentorPairing.weight,
    settings.mentorPairing.historyLookback,
  ];
  if (nonNegativeIntegers.some((value) => !Number.isSafeInteger(value) || value < 0)) {
    throw new InvalidAdvancedSettingError("rules");
  }
  const supportedRelations = new Set([
    "desk_mate",
    "horizontal",
    "vertical",
    "diagonal",
    "adjacent_any",
    "within_distance",
  ]);
  if (
    !Number.isSafeInteger(settings.avoidRecentNeighbors.withinDistance) ||
    settings.avoidRecentNeighbors.withinDistance < 1 ||
    settings.avoidRecentNeighbors.relationTypes.length === 0 ||
    settings.avoidRecentNeighbors.relationTypes.some((relation) => !supportedRelations.has(relation))
  ) {
    throw new InvalidAdvancedSettingError("rules");
  }
  if (
    !Number.isSafeInteger(settings.cooling.coolingPeriod) ||
    settings.cooling.coolingPeriod < 1 ||
    !Number.isSafeInteger(settings.cooling.withinDistance) ||
    settings.cooling.withinDistance < 1 ||
    settings.cooling.relationTypes.length === 0 ||
    settings.cooling.relationTypes.some((relation) => !supportedRelations.has(relation))
  ) {
    throw new InvalidAdvancedSettingError("rules");
  }
  const { mentorPercentile, learnerPercentile } = settings.mentorPairing;
  if (
    !Number.isFinite(mentorPercentile) ||
    !Number.isFinite(learnerPercentile) ||
    mentorPercentile < 0 ||
    mentorPercentile > 1 ||
    learnerPercentile < 0 ||
    learnerPercentile > 1 ||
    learnerPercentile >= mentorPercentile
  ) {
    throw new InvalidAdvancedSettingError("rules");
  }
}

function buildRulesOverlay(
  preferences: CommonPreferenceId[],
  detailedRules?: DetailedRuleSettings,
  groups?: CommonGroupRule[],
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
  if (detailedRules?.enabled) {
    soft.fair_rotation = {
      enabled: detailedRules.fairRotation.enabled,
      weight: detailedRules.fairRotation.weight,
      lookback: detailedRules.fairRotation.lookback,
    };
    soft.avoid_recent_neighbors = {
      enabled: detailedRules.avoidRecentNeighbors.enabled,
      weight: detailedRules.avoidRecentNeighbors.weight,
      lookback: detailedRules.avoidRecentNeighbors.lookback,
      max_recent_count: detailedRules.avoidRecentNeighbors.maxRecentCount,
      within_distance: detailedRules.avoidRecentNeighbors.withinDistance,
      relation_types: detailedRules.avoidRecentNeighbors.relationTypes,
    };
    soft.cooling = {
      enabled: detailedRules.cooling.enabled,
      weight: detailedRules.cooling.weight,
      cooling_period: detailedRules.cooling.coolingPeriod,
      within_distance: detailedRules.cooling.withinDistance,
      relation_types: detailedRules.cooling.relationTypes,
    };
    soft.score_position = {
      enabled: detailedRules.scorePosition.enabled,
      weight: detailedRules.scorePosition.weight,
      direction: detailedRules.scorePosition.direction,
    };
    soft.score_distribution = {
      enabled: detailedRules.scoreDistribution.enabled,
      weight: detailedRules.scoreDistribution.weight,
      scope: detailedRules.scoreDistribution.scope,
    };
    soft.mentor_pairing = {
      enabled: detailedRules.mentorPairing.enabled,
      weight: detailedRules.mentorPairing.weight,
      mentor_percentile: detailedRules.mentorPairing.mentorPercentile,
      learner_percentile: detailedRules.mentorPairing.learnerPercentile,
      relation: detailedRules.mentorPairing.relation,
      avoid_recent_repeats: detailedRules.mentorPairing.avoidRecentRepeats,
      history_lookback: detailedRules.mentorPairing.historyLookback,
    };
  }
  const validGroups = (groups ?? [])
    .map((group) => ({
      name: group.name.trim(),
      students: [...new Set(group.students.map((student) => student.trim()).filter(Boolean))],
      separate: group.mode === "separate",
      together: group.mode === "together",
    }))
    .filter((group) => group.name && group.students.length >= 2);
  if (validGroups.length) {
    return {
      ...(Object.keys(soft).length ? { soft } : {}),
      groups: validGroups,
    };
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
  groups,
  preferences,
  detailedRules,
  historySnapshots,
}: {
  className: string;
  students: Student[];
  selectedRoomId: string;
  selectedGoalId: string;
  settings: AdvancedSolveSettings;
  roomSettings: CustomRoomSettings;
  constraints: CommonConstraint[];
  groups?: CommonGroupRule[];
  preferences: CommonPreferenceId[];
  detailedRules?: DetailedRuleSettings;
  historySnapshots?: HistorySnapshotPayload[];
}): GenerateClassRequest {
  const customRules = parseJsonObject(settings.customRulesJson, "rules");
  const customLayout = roomSettings.enabled
    ? roomSettings.layoutJson.trim()
      ? parseJsonObject(roomSettings.layoutJson, "layout")
      : buildGridLayout(roomSettings)
    : undefined;
  const hardRules = buildHardRules(constraints);
  if (detailedRules) {
    validateDetailedRules(detailedRules);
  }
  const rulesOverlay = buildRulesOverlay(preferences, detailedRules, groups);
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
      ...(historySnapshots?.length ? { history_snapshots: historySnapshots } : {}),
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

export function buildGenerateRotationPlanRequest({
  className,
  students,
  selectedRoomId,
  selectedGoalId,
  settings,
  roomSettings,
  constraints,
  groups,
  preferences,
  detailedRules,
  historySnapshots,
  rotation,
}: {
  className: string;
  students: Student[];
  selectedRoomId: string;
  selectedGoalId: string;
  settings: AdvancedSolveSettings;
  roomSettings: CustomRoomSettings;
  constraints: CommonConstraint[];
  groups?: CommonGroupRule[];
  preferences: CommonPreferenceId[];
  detailedRules?: DetailedRuleSettings;
  historySnapshots?: HistorySnapshotPayload[];
  rotation: RotationSettings;
}): GenerateRotationPlanRequest {
  const base = buildGenerateClassRequest({
    className,
    students,
    selectedRoomId,
    selectedGoalId,
    settings,
    roomSettings,
    constraints,
    groups,
    preferences,
    detailedRules,
    historySnapshots,
  });
  const periodLabels = rotation.periodLabels
    .split(/[\n,，]+/u)
    .map((label) => label.trim())
    .filter(Boolean);
  if (
    !Number.isInteger(rotation.periodCount) ||
    rotation.periodCount < 1 ||
    rotation.periodCount > 20 ||
    (periodLabels.length > 0 && periodLabels.length !== rotation.periodCount)
  ) {
    throw new InvalidAdvancedSettingError("rotation");
  }
  return {
    draft: base.draft,
    period_count: rotation.periodCount,
    ...(periodLabels.length ? { period_labels: periodLabels } : {}),
    options: base.options,
  };
}
