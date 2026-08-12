// M5-A7 gates: the workbench's rule ids must all live in the Rust registry,
// and the registry itself must be complete and typed.

import { describe, expect, it } from "vitest";
import { RULE_SPECS } from "../api/ruleRegistry.generated";
import { ALL_RULE_IDS, findRule, requireRule, rulesByCategory } from "./ruleRegistry";

// The rule ids the workbench builds requests with (generation.ts and
// friends). Any id used here but missing from the registry is a contract
// violation.
const WORKBENCH_RULE_IDS = [
  "fixed_seats",
  "must_be_adjacent",
  "cannot_be_adjacent",
  "min_distance",
  "vision_front",
  "height_back",
  "fair_rotation",
  "avoid_recent_neighbors",
  "score_balance",
];

describe("rule registry consumption", () => {
  it("registry is non-empty and fully typed", () => {
    expect(RULE_SPECS.length).toBeGreaterThan(0);
    for (const spec of RULE_SPECS) {
      expect(spec.id).toBeTruthy();
      expect(spec.category).toMatch(/^(hard|soft)$/);
      expect(spec.label.zh).toBeTruthy();
      expect(spec.label.en).toBeTruthy();
      expect(spec.param_schema).toBeTruthy();
    }
  });

  it("every workbench rule id exists in the Rust registry", () => {
    for (const id of WORKBENCH_RULE_IDS) {
      expect(ALL_RULE_IDS.has(id), `rule ${id} must be in the Rust registry`).toBe(true);
    }
  });

  it("lookups resolve and unknown ids fail loudly", () => {
    expect(findRule("vision_front")?.category).toBe("soft");
    expect(findRule("fixed_seats")?.category).toBe("hard");
    expect(findRule("does_not_exist")).toBeUndefined();
    expect(() => requireRule("does_not_exist")).toThrow(/RuleSpec/);
  });

  it("categories partition the registry", () => {
    const hard = rulesByCategory("hard");
    const soft = rulesByCategory("soft");
    expect(hard.length + soft.length).toBe(RULE_SPECS.length);
    expect(hard.length).toBeGreaterThan(0);
    expect(soft.length).toBeGreaterThan(0);
  });
});
