import type {
  CommonConstraint,
  CommonGroupRule,
  CommonPreferenceId,
  CompiledRule,
} from "../api/types";

/**
 * Display-level adapter: a compiled rule (Rust artifact, B3/D3) becomes the
 * corresponding entry in the workbench's rule state. The mapping is pure
 * form plumbing — the rule semantics live in Rust at solve time (the TS rule
 * builders are removed in M6).
 */

function newId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

export function compiledToConstraint(
  compiled: CompiledRule,
): CommonConstraint | null {
  const entry = compiled.entry;
  switch (compiled.rule_id) {
    case "min_distance": {
      const students = entry.students as string[];
      return {
        id: newId(),
        kind: "min_distance",
        first: students[0] ?? "",
        second: students[1] ?? "",
        seatId: "",
        distance: typeof entry.distance === "number" ? entry.distance : 2,
        metric: entry.metric === "euclidean" ? "euclidean" : "graph",
        enabled: true,
      };
    }
    case "fixed_seats":
      return {
        id: newId(),
        kind: "fixed_seat",
        first: String(entry.student ?? ""),
        second: "",
        seatId: String(entry.seat_id ?? ""),
        distance: 2,
        metric: "graph",
        enabled: true,
      };
    case "must_be_adjacent": {
      const students = entry.students as string[];
      return {
        id: newId(),
        kind: "must_adjacent",
        first: students[0] ?? "",
        second: students[1] ?? "",
        seatId: "",
        distance: 2,
        metric: "graph",
        enabled: true,
      };
    }
    case "cannot_be_adjacent": {
      const students = entry.students as string[];
      return {
        id: newId(),
        kind: "avoid_adjacent",
        first: students[0] ?? "",
        second: students[1] ?? "",
        seatId: "",
        distance: 2,
        metric: "graph",
        enabled: true,
      };
    }
    default:
      return null;
  }
}

export function compiledToGroup(compiled: CompiledRule): CommonGroupRule | null {
  if (compiled.rule_id !== "groups") {
    return null;
  }
  const entry = compiled.entry;
  return {
    id: newId(),
    name: String(entry.name ?? ""),
    mode: entry.separate === false ? "together" : "separate",
    students: (entry.students as string[] | undefined) ?? [],
    enabled: true,
  };
}

export function compiledToPreference(
  compiled: CompiledRule,
): CommonPreferenceId | null {
  switch (compiled.rule_id) {
    case "vision_front":
      return "vision_front";
    case "score_distribution":
      return "score_distribution";
    default:
      return null;
  }
}

/** Turn a compiled rule into the state entry its category manages. */
export type CompiledRuleTarget =
  | { kind: "constraint"; rule: CommonConstraint }
  | { kind: "group"; rule: CommonGroupRule }
  | { kind: "preference"; id: CommonPreferenceId }
  | null;

export function compiledRuleTarget(compiled: CompiledRule): CompiledRuleTarget {
  if (compiled.category === "hard") {
    const constraint = compiledToConstraint(compiled);
    if (constraint) {
      return { kind: "constraint", rule: constraint };
    }
    const group = compiledToGroup(compiled);
    if (group) {
      return { kind: "group", rule: group };
    }
    return null;
  }
  const preference = compiledToPreference(compiled);
  return preference ? { kind: "preference", id: preference } : null;
}
