import { describe, expect, it } from "vitest";

import type { DraftAuditReport } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { diffSeatIds, reasonCardFor, DIMENSION_META } from "./auditTerms";

const t = createTranslator("zh-CN");

function report(overrides: Partial<DraftAuditReport> = {}): DraftAuditReport {
  return {
    api_version: "2",
    draft_id: "draft-1",
    feasible: true,
    score: {
      total: 84.4,
      breakdown: {
        fair_rotation_score: {
          status: "available",
          score: 82.4,
          weight: 10,
          details: {},
        },
        avoid_recent_neighbors_score: {
          status: "available",
          score: 91.0,
          weight: 10,
          details: {},
        },
        score_balance_score: { status: "available", score: 76.5, weight: 18, details: {} },
        height_preference_score: { status: "not_available", score: null, weight: 0, details: { reason: "no height data" } },
        vision_preference_score: { status: "available", score: 96.8, weight: 20, details: {} },
        diversity_score: { status: "available", score: 70.1, weight: 5, details: {} },
        stability_score: { status: "available", score: 85.6, weight: 12, details: {} },
        rule_scores: {},
        hard_constraint_summary: {
          all_satisfied: true,
          checked_rule_count: 1,
          violation_count: 0,
          witnesses: [],
        },
      },
    },
    audit: {
      hard_constraint_summary: {
        all_satisfied: true,
        checked_rule_count: 1,
        violation_count: 0,
        witnesses: [],
      },
      missing_data: { students_missing_score: 0, students_missing_height: 3, students_missing_vision: 0, students_missing_needs: 0 },
      history: { snapshot_count: 0, has_history: false },
      suggested_actions: [],
    },
    ...overrides,
  };
}

describe("DIMENSION_META", () => {
  it("covers the seven scored dimensions with teacher-facing terms", () => {
    expect(DIMENSION_META).toHaveLength(7);
    const terms = DIMENSION_META.map((meta) => t(meta.term));
    // G-1: no program identifiers leak into teacher copy.
    expect(terms.join(" ")).not.toMatch(/fair_rotation|recent_neighbors|score_balance/);
    expect(terms).toContain("轮换公平性");
    expect(terms).toContain("成绩搭配");
  });
});

describe("reasonCardFor", () => {
  it("ranks available dimensions by score and marks hard requirements", () => {
    const card = reasonCardFor(report(), t);
    expect(card.hardSatisfied).toBe(true);
    expect(card.checkedRuleCount).toBe(1);
    expect(card.ranking[0].key).toBe("vision_preference_score");
    expect(card.ranking[0].score).toBe(96.8);
    // Unavailable dimensions never rank.
    expect(card.ranking.some((item) => item.key === "height_preference_score")).toBe(false);
    expect(card.reasons[0]).toContain("视力需求靠前");
    expect(card.reasons[0]).toContain("96.8");
  });

  it("reports violations when the hard summary fails", () => {
    const base = report();
    const bad = {
      ...base,
      audit: {
        ...base.audit,
        hard_constraint_summary: {
          all_satisfied: false,
          checked_rule_count: 3,
          violation_count: 2,
          witnesses: [],
        },
      },
    };
    const card = reasonCardFor(bad, t);
    expect(card.hardSatisfied).toBe(false);
    expect(card.violationCount).toBe(2);
  });
});

describe("diffSeatIds", () => {
  const seat = (seatId: string, student?: string) => ({
    seatId,
    student: student ? { id: student } : undefined,
  });

  it("marks seats whose occupant differs between two plans", () => {
    const a = [seat("R1C1", "S01"), seat("R1C2", "S02"), seat("R1C3")];
    const b = [seat("R1C1", "S02"), seat("R1C2", "S01"), seat("R1C3")];
    expect([...diffSeatIds(a, b)].sort()).toEqual(["R1C1", "R1C2"]);
  });

  it("ignores seats that match or are empty on both sides", () => {
    const a = [seat("R1C1", "S01"), seat("R1C2")];
    const b = [seat("R1C1", "S01"), seat("R1C2")];
    expect(diffSeatIds(a, b).size).toBe(0);
  });

  it("marks a seat filled on one side only", () => {
    const a = [seat("R1C1", "S01")];
    const b = [seat("R1C1")];
    expect([...diffSeatIds(a, b)]).toEqual(["R1C1"]);
  });
});
