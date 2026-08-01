import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import { createTranslator } from "../i18n/messages";
import { RuleSetEditorPanel } from "./RuleSetEditorPanel";

const students = [
  { id: "S1", name: "Alice" },
  { id: "S2", name: "Bob" },
];

function Harness({ initial = "" }: { initial?: string }) {
  const [source, setSource] = useState(initial);
  return (
    <>
      <RuleSetEditorPanel
        source={source}
        students={students}
        seatIds={["R1C1", "R1C2"]}
        t={createTranslator("en")}
        onChange={setSource}
      />
      <pre data-testid="ruleset-source">{source}</pre>
    </>
  );
}

describe("RuleSetEditorPanel", () => {
  it("creates a structured RuleSet and adds a hard rule", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.click(screen.getByTestId("ruleset-editor-create"));
    await user.click(screen.getByText("Visual RuleSet editor"));
    await user.click(screen.getByTestId("ruleset-editor-add-fixed"));

    const source = JSON.parse(screen.getByTestId("ruleset-source").textContent ?? "{}");
    expect(source.schema_version).toBe(1);
    expect(source.hard.fixed_seats).toEqual([{ student: "S1", seat_id: "R1C1" }]);
  });

  it("edits soft cooling and group settings without losing existing fields", async () => {
    const user = userEvent.setup();
    render(
      <Harness
        initial={JSON.stringify({
          schema_version: 1,
          seed: 7,
          hard: { fixed_seats: [] },
          soft: { cooling: { enabled: false, weight: 5, cooling_period: 3, within_distance: 2, relation_types: ["desk_mate"] } },
          groups: [],
        })}
      />,
    );

    await user.click(screen.getByText("Visual RuleSet editor"));
    const cooling = screen.getByRole("group", { name: "Relationship cooling" });
    await user.click(within(cooling).getByRole("checkbox", { name: "Enable this rule" }));
    await user.click(screen.getByTestId("ruleset-editor-add-group"));

    const source = JSON.parse(screen.getByTestId("ruleset-source").textContent ?? "{}");
    expect(source.seed).toBe(7);
    expect(source.soft.cooling.enabled).toBe(true);
    expect(source.groups[0]).toMatchObject({
      name: "Group 1",
      students: ["S1", "S2"],
      together: true,
    });
  });
});
