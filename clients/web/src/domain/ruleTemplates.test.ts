import { describe, expect, it } from "vitest";

import type { CompiledRule } from "../api/types";
import {
  compiledRuleTarget,
  compiledToConstraint,
  compiledToGroup,
  compiledToPreference,
} from "./ruleTemplates";

function compiled(rule_id: string, entry: Record<string, unknown>, category: "hard" | "soft" = "hard"): CompiledRule {
  return { api_version: "1", category, rule_id, entry };
}

describe("compiledToConstraint", () => {
  it("maps min_distance entries", () => {
    const constraint = compiledToConstraint(
      compiled("min_distance", { students: ["S01", "S02"], distance: 2.5, metric: "euclidean" }),
    );
    expect(constraint).toMatchObject({
      kind: "min_distance",
      first: "S01",
      second: "S02",
      distance: 2.5,
      metric: "euclidean",
      enabled: true,
    });
  });

  it("maps fixed seat, adjacency and distance kinds", () => {
    expect(
      compiledToConstraint(compiled("fixed_seats", { student: "S01", seat_id: "R1C1" })),
    ).toMatchObject({ kind: "fixed_seat", first: "S01", seatId: "R1C1" });
    expect(
      compiledToConstraint(compiled("must_be_adjacent", { students: ["S01", "S02"] })),
    ).toMatchObject({ kind: "must_adjacent" });
    expect(
      compiledToConstraint(compiled("cannot_be_adjacent", { students: ["S01", "S02"] })),
    ).toMatchObject({ kind: "avoid_adjacent" });
  });

  it("returns null for unknown hard rule ids", () => {
    expect(compiledToConstraint(compiled("nope", {}))).toBeNull();
  });
});

describe("compiledToGroup", () => {
  it("maps group entries with the mode transformation", () => {
    const group = compiledToGroup(
      compiled("groups", { name: "第一组", students: ["S01", "S02"], separate: true }),
    );
    expect(group).toMatchObject({ name: "第一组", students: ["S01", "S02"], mode: "separate" });
    const together = compiledToGroup(
      compiled("groups", { name: "G", students: ["S01", "S02"], separate: false }),
    );
    expect(together?.mode).toBe("together");
  });

  it("returns null for non-group ids", () => {
    expect(compiledToGroup(compiled("min_distance", {}))).toBeNull();
  });
});

describe("compiledToPreference", () => {
  it("maps the soft template rule ids to preferences", () => {
    expect(
      compiledToPreference(compiled("vision_front", { enabled: true }, "soft")),
    ).toBe("vision_front");
    expect(
      compiledToPreference(compiled("score_distribution", { enabled: true }, "soft")),
    ).toBe("score_distribution");
    expect(compiledToPreference(compiled("nope", {}, "soft"))).toBeNull();
  });
});

describe("compiledRuleTarget", () => {
  it("routes hard rules to constraints and soft rules to preferences", () => {
    expect(
      compiledRuleTarget(compiled("min_distance", { students: ["S01", "S02"] }))?.kind,
    ).toBe("constraint");
    expect(
      compiledRuleTarget(compiled("groups", { name: "G", students: ["S01", "S02"] }))?.kind,
    ).toBe("group");
    expect(
      compiledRuleTarget(compiled("vision_front", {}, "soft"))?.kind,
    ).toBe("preference");
    expect(compiledRuleTarget(compiled("nope", {}))).toBeNull();
  });
});
