import { describe, expect, it } from "vitest";

import type { DraftAuditReport, Student } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { fixAllOperations, issuesFromAudit } from "./diagnostics";

const t = createTranslator("zh-CN");

const STUDENTS: Student[] = [
  { id: "S01", name: "丁一" },
  { id: "S02", name: "万二" },
  { id: "S03", name: "丈三" },
];

function report(witnesses: unknown[], suggestions: unknown[] = []): DraftAuditReport {
  return {
    api_version: "2",
    draft_id: "draft-1",
    feasible: true,
    score: {
      total: 50,
      breakdown: {
        fair_rotation_score: { status: "available", score: 50, weight: 10, details: {} },
        avoid_recent_neighbors_score: { status: "available", score: 50, weight: 10, details: {} },
        score_balance_score: { status: "available", score: 50, weight: 18, details: {} },
        height_preference_score: { status: "not_available", score: null, weight: 0, details: { reason: "no height" } },
        vision_preference_score: { status: "available", score: 50, weight: 20, details: {} },
        diversity_score: { status: "available", score: 50, weight: 5, details: {} },
        stability_score: { status: "available", score: 50, weight: 12, details: {} },
        rule_scores: {},
        hard_constraint_summary: {
          all_satisfied: witnesses.length === 0,
          checked_rule_count: witnesses.length,
          violation_count: witnesses.length,
          witnesses: witnesses as unknown[],
        },
      },
    },
    audit: {
      hard_constraint_summary: {
        all_satisfied: witnesses.length === 0,
        checked_rule_count: witnesses.length,
        violation_count: witnesses.length,
        witnesses: witnesses as unknown[],
      },
      missing_data: { students_missing_score: 0, students_missing_height: 1, students_missing_vision: 0, students_missing_needs: 0 },
      history: { snapshot_count: 0, has_history: false },
      suggested_actions: suggestions as never[],
    },
  };
}

describe("issuesFromAudit", () => {
  it("turns witnesses into severity-sorted error issues with fixes", () => {
    const issues = issuesFromAudit(
      report([
        {
          kind: "fixed_seats",
          seat_ids: ["R1C2"],
          args: { student: "S01", expected_seat: "R1C1", actual_seat: "R1C2" },
          suggested_fix: { student: "S01", seat_id: "R1C1" },
        },
        {
          kind: "min_distance",
          seat_ids: ["R5C2", "R5C3"],
          args: { student_a: "S02", student_b: "S03", distance: 2 },
          suggested_fix: { student: "S02", seat_id: "R4C4" },
        },
      ]),
      STUDENTS,
    );

    expect(issues).toHaveLength(2);
    expect(issues[0].severity).toBe("error");
    // Student keys become display names in the message args.
    expect(t(issues[0].titleKey, issues[0].args)).toContain("丁一");
    expect(t(issues[0].titleKey, issues[0].args)).toContain("R1C2");
    expect(issues[0].fix).toEqual({ student: "S01", seatId: "R1C1" });
    expect(issues[0].seatIds).toEqual(["R1C2"]);
  });

  it("maps missing-data suggestions to warning issues", () => {
    const issues = issuesFromAudit(
      report([], [
        {
          message_key: "audit.missing_height",
          suggested_action: "add_student_field",
          args: { field: "height_cm", count: 1 },
        },
      ]),
      STUDENTS,
    );
    expect(issues).toHaveLength(1);
    expect(issues[0].severity).toBe("warning");
    expect(issues[0].seatIds).toEqual([]);
    expect(t(issues[0].titleKey, issues[0].args)).toContain("1");
  });

  it("ignores the ready sentinel and returns nothing for a clean plan", () => {
    const issues = issuesFromAudit(
      report([], [
        { message_key: "audit.ready", suggested_action: "none", args: {} },
      ]),
      STUDENTS,
    );
    expect(issues).toEqual([]);
  });
});

describe("fixAllOperations", () => {
  it("collects one move_student op per fixable witness", () => {
    const issues = issuesFromAudit(
      report([
        {
          kind: "fixed_seats",
          seat_ids: ["R1C2"],
          args: { student: "S01", expected_seat: "R1C1", actual_seat: "R1C2" },
          suggested_fix: { student: "S01", seat_id: "R1C1" },
        },
        {
          kind: "min_distance",
          seat_ids: ["R5C2", "R5C3"],
          args: { student_a: "S02", student_b: "S03", distance: 2 },
          // no suggested_fix -> not fixable
        },
      ]),
      STUDENTS,
    );
    expect(fixAllOperations(issues)).toEqual([
      { kind: "move_student", payload: { student_key: "S01", seat_id: "R1C1" } },
    ]);
  });
});
