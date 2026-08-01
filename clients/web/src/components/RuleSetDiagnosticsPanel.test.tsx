import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { createTranslator } from "../i18n/messages";
import { RuleSetDiagnosticsPanel } from "./RuleSetDiagnosticsPanel";

describe("RuleSetDiagnosticsPanel", () => {
  it("shows a localized field-level error and keeps the path visible", () => {
    render(
      <RuleSetDiagnosticsPanel
        source='{"hard":{"fixed_seats":[{"student":"S9","seat_id":"R9C9"}]}}'
        students={[{ id: "S1", name: "Alice" }]}
        seatIds={["R1C1"]}
        t={createTranslator("en")}
      />,
    );

    expect(screen.getByTestId("rules-diagnostics")).toHaveAttribute("role", "alert");
    expect(screen.getByText("hard.fixed_seats[0].student")).toBeInTheDocument();
    expect(screen.getByText("This student ID is not in the current roster.")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Choose a student from the current roster; re-import the roster if it changed.",
      ),
    ).toBeInTheDocument();
  });

  it("confirms a valid object and explains an empty editor", () => {
    const { rerender } = render(
      <RuleSetDiagnosticsPanel
        source='{"hard":{"must_be_adjacent":[{"students":["S1","S2"]}]}}'
        students={[{ id: "S1", name: "Alice" }, { id: "S2", name: "Bob" }]}
        seatIds={["R1C1"]}
        t={createTranslator("en")}
      />,
    );
    expect(screen.getByTestId("rules-diagnostics")).toHaveAttribute("role", "status");
    expect(
      screen.getByText("The format and field checks passed. You can generate the plan."),
    ).toBeInTheDocument();

    rerender(
      <RuleSetDiagnosticsPanel
        source=""
        students={[]}
        seatIds={[]}
        t={createTranslator("en")}
      />,
    );
    expect(
      screen.getByText(
        "No custom rules entered. The selected goal and common settings will be used.",
      ),
    ).toBeInTheDocument();
  });
});
