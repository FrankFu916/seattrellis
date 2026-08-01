import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import type { DetailedRuleSettings } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { DetailedRulesPanel } from "./DetailedRulesPanel";

const initialSettings: DetailedRuleSettings = {
  enabled: false,
  fairRotation: { enabled: true, weight: 10, lookback: 4 },
  avoidRecentNeighbors: {
    enabled: true,
    weight: 10,
    lookback: 4,
    maxRecentCount: 1,
    withinDistance: 2,
    relationTypes: ["desk_mate", "adjacent_any"],
  },
  cooling: {
    enabled: false,
    weight: 12,
    coolingPeriod: 3,
    withinDistance: 2,
    relationTypes: ["desk_mate", "adjacent_any"],
  },
  scorePosition: { enabled: true, weight: 18, direction: "high_front" },
  scoreDistribution: { enabled: true, weight: 18, scope: "row" },
  mentorPairing: {
    enabled: true,
    weight: 18,
    mentorPercentile: 0.75,
    learnerPercentile: 0.25,
    relation: "desk_mate",
    avoidRecentRepeats: true,
    historyLookback: 4,
  },
};

function Harness() {
  const [settings, setSettings] = useState(initialSettings);
  return (
    <DetailedRulesPanel
      settings={settings}
      t={createTranslator("en")}
      onChange={(changes) => setSettings((current) => ({ ...current, ...changes }))}
    />
  );
}

describe("DetailedRulesPanel", () => {
  it("reveals the detailed controls when enabled and updates rule values", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    expect(screen.getByTestId("detailed-rules-settings")).not.toHaveAttribute("open");
    await user.click(screen.getByText("Detailed seating rules"));
    await user.click(screen.getByTestId("detailed-rules-toggle"));

    expect(screen.getByText("Historical position rotation")).toBeInTheDocument();
    const direction = screen.getByRole("combobox", { name: "Direction" });
    await user.selectOptions(direction, "high_back");
    expect(direction).toHaveValue("high_back");

    const recentNeighbors = screen.getByRole("group", {
      name: "Avoid recent neighbors",
    });
    const relation = within(recentNeighbors).getByRole("checkbox", {
      name: "Desk mate",
    });
    await user.click(relation);
    expect(relation).not.toBeChecked();
    expect(
      within(recentNeighbors).getByRole("checkbox", { name: "Any adjacent seat" }),
    ).toBeChecked();

    const coolingToggle = screen.getAllByRole("checkbox", {
      name: "Enable this rule",
    })[2];
    expect(coolingToggle).not.toBeChecked();
    await user.click(coolingToggle);
    expect(coolingToggle).toBeChecked();
  });
});
