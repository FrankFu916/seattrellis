import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";

import * as client from "../api/client";
import { createTranslator } from "../i18n/messages";
import { RuleSetDiagnosticsPanel } from "./RuleSetDiagnosticsPanel";

describe("RuleSetDiagnosticsPanel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("shows a localized field-level error and keeps the path visible", async () => {
    vi.spyOn(client, "validateRuleDocument").mockResolvedValue({
      api_version: "1",
      diagnostics: [
        { path: "hard.fixed_seats[0].student", code: "unknown_student" },
        { path: "hard.fixed_seats[0].seat_id", code: "unknown_seat" },
      ],
    });

    render(
      <RuleSetDiagnosticsPanel
        source='{"hard":{"fixed_seats":[{"student":"S9","seat_id":"R9C9"}]}}'
        students={[{ id: "S1", name: "Alice" }]}
        seatIds={["R1C1"]}
        t={createTranslator("en")}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("rules-diagnostics")).toHaveAttribute("role", "alert");
    });
    expect(screen.getByText("hard.fixed_seats[0].student")).toBeInTheDocument();
    expect(screen.getByText("This student ID is not in the current roster.")).toBeInTheDocument();
  });

  it("confirms a valid object and explains an empty editor", async () => {
    vi.spyOn(client, "validateRuleDocument").mockResolvedValue({
      api_version: "1",
      diagnostics: [],
    });

    const { rerender } = render(
      <RuleSetDiagnosticsPanel
        source='{"hard":{"must_be_adjacent":[{"students":["S1","S2"]}]}}'
        students={[{ id: "S1", name: "Alice" }, { id: "S2", name: "Bob" }]}
        seatIds={["R1C1"]}
        t={createTranslator("en")}
      />,
    );
    await waitFor(() => {
      expect(screen.getByTestId("rules-diagnostics")).toHaveAttribute("role", "status");
    });
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

  it("explains a service outage without crashing", async () => {
    vi.spyOn(client, "validateRuleDocument").mockRejectedValue(new Error("down"));

    render(
      <RuleSetDiagnosticsPanel
        source='{"hard":{}}'
        students={[{ id: "S1", name: "Alice" }]}
        seatIds={["R1C1"]}
        t={createTranslator("en")}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText(/unavailable right now/)).toBeInTheDocument();
    });
  });
});
