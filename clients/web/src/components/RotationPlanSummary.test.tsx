import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { RotationPlan } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { RotationPlanSummary } from "./RotationPlanSummary";

const plan: RotationPlan = {
  kind: "rotation_plan",
  name: "Class 8–3",
  base_history_count: 1,
  periods: [
    {
      period: 1,
      label: "Week 1",
      snapshot: {
        assignments: [
          { student_key: "S1", student_name: "Alice", seat_id: "R1C1" },
        ],
        solver_status: "FEASIBLE",
      },
    },
    {
      period: 2,
      label: "Week 2",
      snapshot: {
        assignments: [
          { student_key: "S1", student_name: "Alice", seat_id: "R1C2" },
        ],
        solver_status: "FEASIBLE",
      },
    },
  ],
  fairness_summary: {},
  pair_repeat_summary: {
    repeated_pair_count: 2,
    max_occurrences: 3,
  },
  warnings: [],
};

describe("RotationPlanSummary", () => {
  it("shows period labels and repeat metrics without student names", () => {
    render(
      <RotationPlanSummary plan={plan} t={createTranslator("en")} />,
    );

    expect(screen.getByTestId("rotation-plan-summary")).toBeInTheDocument();
    expect(screen.getByText("2 periods")).toBeInTheDocument();
    expect(screen.getByText("Period 1: Week 1")).toBeInTheDocument();
    expect(screen.getByText("Repeated neighbor pairs: 2")).toBeInTheDocument();
    expect(screen.queryByText("Alice")).not.toBeInTheDocument();
  });

  it("lets the teacher switch to another period", () => {
    const onPeriodSelect = vi.fn();
    render(
      <RotationPlanSummary
        plan={plan}
        t={createTranslator("en")}
        onPeriodSelect={onPeriodSelect}
      />,
    );

    fireEvent.click(screen.getByTestId("rotation-period-2"));

    expect(onPeriodSelect).toHaveBeenCalledWith(2);
  });
});
