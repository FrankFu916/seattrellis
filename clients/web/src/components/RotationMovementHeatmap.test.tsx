import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import type { RotationPlan, SeatAssignment } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { RotationMovementHeatmap } from "./RotationMovementHeatmap";

const layoutSeats: SeatAssignment[] = [
  { seatId: "North", row: 0, column: 0, locked: false },
  { seatId: "Window", row: 0, column: 2, locked: false },
  { seatId: "Rear", row: 1, column: 0, locked: false },
];

const plan: RotationPlan = {
  kind: "rotation_plan",
  name: "Class 8–3",
  base_history_count: 0,
  fairness_summary: {},
  pair_repeat_summary: {},
  warnings: [],
  periods: [
    {
      period: 1,
      label: "Week 1",
      snapshot: {
        solver_status: "Solved",
        assignments: [
          { student_key: "S1", student_name: "Alice", seat_id: "North" },
          { student_key: "S2", student_name: "Bob", seat_id: "Window" },
        ],
      },
    },
    {
      period: 2,
      label: "Week 2",
      snapshot: {
        solver_status: "Solved",
        assignments: [
          { student_key: "S1", student_name: "Alice", seat_id: "Window" },
          { student_key: "S2", student_name: "Bob", seat_id: "North" },
        ],
      },
    },
    {
      period: 3,
      label: "Week 3",
      snapshot: {
        solver_status: "Solved",
        assignments: [
          { student_key: "S1", student_name: "Alice", seat_id: "Window" },
          { student_key: "S2", student_name: "Bob", seat_id: "Rear" },
        ],
      },
    },
  ],
};

describe("RotationMovementHeatmap", () => {
  it("shows full-cycle churn for arbitrary seat IDs", () => {
    render(
      <RotationMovementHeatmap
        plan={plan}
        layoutSeats={layoutSeats}
        activePeriod={1}
        locale="en"
        t={createTranslator("en")}
      />,
    );

    expect(screen.getByText("Seat movement heatmap")).toBeInTheDocument();
    expect(screen.getByText("Generated snapshots")).toBeInTheDocument();
    expect(screen.getByText("Move events").previousElementSibling).toHaveTextContent("3");
    expect(screen.getByTestId("movement-seat-North")).toHaveAttribute(
      "data-level",
      "very-high",
    );
    expect(screen.getByTestId("movement-seat-Window")).toHaveStyle({
      gridColumn: "3",
      gridRow: "1",
    });
    expect(
      screen.getByLabelText(
        "North: occupant changed in 2 of 2 adjacent transitions",
      ),
    ).toBeInTheDocument();
  });

  it("switches adjacent transitions and exposes text movement details", async () => {
    const user = userEvent.setup();
    render(
      <RotationMovementHeatmap
        plan={plan}
        layoutSeats={layoutSeats}
        activePeriod={3}
        locale="en"
        t={createTranslator("en")}
      />,
    );

    await user.click(screen.getByRole("tab", { name: "Adjacent periods" }));

    expect(screen.getByRole("combobox")).toHaveValue("3");
    expect(
      screen.getByLabelText(
        "Rear: Bob, previous seat North, 1 grid step",
      ),
    ).toHaveAttribute("data-level", "low");
    expect(screen.getByText("North → Rear").closest("li")).toHaveTextContent(
      "North → Rear",
    );

    await user.selectOptions(screen.getByRole("combobox"), "2");

    expect(
      screen.getByLabelText(
        "North: Bob, previous seat Window, 2 grid steps",
      ),
    ).toHaveAttribute("data-level", "medium");
    expect(
      screen.getByLabelText(
        "Window: Alice, previous seat North, 2 grid steps",
      ),
    ).toBeInTheDocument();
  });

  it("implements roving keyboard tabs and keeps their panel mounted", async () => {
    const user = userEvent.setup();
    render(
      <RotationMovementHeatmap
        plan={plan}
        layoutSeats={layoutSeats}
        activePeriod={1}
        locale="en"
        t={createTranslator("en")}
      />,
    );

    const overview = screen.getByRole("tab", { name: "Full-cycle heat" });
    const transition = screen.getByRole("tab", { name: "Adjacent periods" });
    const panel = screen.getByRole("tabpanel");

    expect(overview).toHaveAttribute("tabindex", "0");
    expect(transition).toHaveAttribute("tabindex", "-1");
    expect(overview).toHaveAttribute("aria-controls", panel.id);

    overview.focus();
    await user.keyboard("{ArrowRight}");

    expect(transition).toHaveFocus();
    expect(transition).toHaveAttribute("aria-selected", "true");
    expect(transition).toHaveAttribute("tabindex", "0");

    await user.keyboard("{Home}");
    expect(overview).toHaveFocus();
    expect(overview).toHaveAttribute("aria-selected", "true");
  });

  it("keeps an accessible panel when current layout coordinates are unavailable", async () => {
    const user = userEvent.setup();
    render(
      <RotationMovementHeatmap
        plan={plan}
        layoutSeats={[]}
        activePeriod={1}
        locale="en"
        t={createTranslator("en")}
      />,
    );

    expect(screen.getByRole("tabpanel")).toBeInTheDocument();
    expect(
      screen.getByText(
        "The current layout has no seat coordinates to plot; the metrics above are still valid.",
      ),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Adjacent periods" }));
    expect(screen.getByRole("tabpanel")).toHaveAccessibleName("Adjacent periods");
  });

  it("shows an explicit empty state for a one-period plan", () => {
    render(
      <RotationMovementHeatmap
        plan={{ ...plan, periods: plan.periods.slice(0, 1) }}
        layoutSeats={layoutSeats}
        activePeriod={1}
        locale="en"
        t={createTranslator("en")}
      />,
    );

    expect(
      screen.getByText("At least two periods are needed to analyze seat movement."),
    ).toBeInTheDocument();
  });
});
