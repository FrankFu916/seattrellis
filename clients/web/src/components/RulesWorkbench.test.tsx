import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { demoStudents } from "../api/demo";
import { createTranslator } from "../i18n/messages";
import type { BilingualText, SentenceTemplate } from "../api/types";
import { RulesWorkbench } from "./RulesWorkbench";

const t = createTranslator("en");

function text(zh: string, en: string): BilingualText {
  return { zh, en };
}

const TEMPLATES: SentenceTemplate[] = [
  {
    id: "student_distance",
    rule_id: "min_distance",
    category: "hard",
    label: text("学生距离", "Student distance"),
    sentence: text(
      "让 {student_a} 与 {student_b} 的距离 ≥ {distance}（座）",
      "Keep {student_a} and {student_b} at least {distance} seats apart",
    ),
    slots: [
      {
        key: "student_a",
        kind: "student",
        label: text("学生 A", "Student A"),
        placeholder: text("选择学生…", "Choose student…"),
        param_path: "students/0",
        required: true,
        options: null,
        min: null,
        max: null,
        step: null,
        default: null,
      },
      {
        key: "student_b",
        kind: "student",
        label: text("学生 B", "Student B"),
        placeholder: text("选择学生…", "Choose student…"),
        param_path: "students/1",
        required: true,
        options: null,
        min: null,
        max: null,
        step: null,
        default: null,
      },
      {
        key: "distance",
        kind: "number",
        label: text("最小距离", "Minimum distance"),
        placeholder: null,
        param_path: "distance",
        required: true,
        options: null,
        min: 0.1,
        max: null,
        step: 0.1,
        default: 2,
      },
    ],
    defaults: { students: ["", ""], distance: 2, metric: "graph" },
  },
];

vi.mock("../api/client", () => ({
  fetchRuleTemplates: vi.fn(async () => ({ api_version: "1", templates: TEMPLATES })),
  compileRuleSentence: vi.fn(async (templateId: string, slots: Record<string, string>) => ({
    api_version: "1",
    category: "hard",
    rule_id: "min_distance",
    entry: {
      students: [slots.student_a, slots.student_b],
      distance: Number(slots.distance),
      metric: "graph",
    },
  })),
}));

const goals = [
  {
    id: "daily-rotation",
    name: { "zh-CN": "日常轮换", en: "Daily rotation" },
    description: { "zh-CN": "轮换前后排", en: "Rotate front and back" },
  },
];

function renderWorkbench(overrides: Partial<Parameters<typeof RulesWorkbench>[0]> = {}) {
  const handlers = {
    onGoalChange: vi.fn(),
    onPreferenceToggle: vi.fn(),
    onConstraintAdd: vi.fn(),
    onConstraintBatchAdd: vi.fn(),
    onConstraintChange: vi.fn(),
    onConstraintRemove: vi.fn(),
    onGroupAdd: vi.fn(),
    onGroupBatchAdd: vi.fn(),
    onGroupChange: vi.fn(),
    onGroupRemove: vi.fn(),
    onDetailedRulesChange: vi.fn(),
  };
  render(
    <RulesWorkbench
      locale="en"
      t={t}
      students={demoStudents}
      seatIds={["R1C1", "R1C2", "R1C3"]}
      goals={goals}
      selectedGoalId="daily-rotation"
      preferences={[]}
      constraints={[]}
      groups={[]}
      detailedRules={{
        enabled: false,
        fairRotation: { enabled: true, weight: 10, lookback: 4 },
        avoidRecentNeighbors: { enabled: true, weight: 10, lookback: 4, maxRecentCount: 1, withinDistance: 2, relationTypes: ["desk_mate"] },
        cooling: { enabled: false, weight: 12, coolingPeriod: 3, withinDistance: 2, relationTypes: ["desk_mate"] },
        scorePosition: { enabled: true, weight: 18, direction: "high_front" },
        scoreDistribution: { enabled: true, weight: 18, scope: "row" },
        mentorPairing: { enabled: true, weight: 18, mentorPercentile: 0.75, learnerPercentile: 0.25, relation: "desk_mate", avoidRecentRepeats: true, historyLookback: 4 },
      }}
      customRulesJson=""
      {...handlers}
      {...overrides}
    />,
  );
  return handlers;
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("RulesWorkbench", () => {
  it("renders the sentence builder from the Rust templates", async () => {
    renderWorkbench();
    expect(
      await screen.findByRole("button", { name: "Student distance" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/at least/)).toBeInTheDocument();
  });

  it("compiles a filled sentence into a constraint card via the API", async () => {
    const user = userEvent.setup();
    const handlers = renderWorkbench();

    // Fill the two student slots, re-querying after each re-render.
    let slotButtons = await screen.findAllByRole("button", {
      name: /Choose student/,
    });
    expect(slotButtons).toHaveLength(2);
    await user.click(slotButtons[0]);
    await user.selectOptions(await screen.findByLabelText("Student A"), "S01");
    await user.click(screen.getByRole("button", { name: /Done/ }));

    slotButtons = await screen.findAllByRole("button", {
      name: /Choose student/,
    });
    await user.click(slotButtons[0]);
    await user.selectOptions(await screen.findByLabelText("Student B"), "S02");
    await user.click(screen.getByRole("button", { name: /Done/ }));

    // Fill the number slot.
    await user.click(screen.getByRole("button", { name: /Choose…/ }));
    await user.type(await screen.findByLabelText("Minimum distance"), "2");

    await user.click(screen.getByRole("button", { name: /Add to rules/ }));
    await waitFor(() => {
      expect(handlers.onConstraintBatchAdd).toHaveBeenCalledWith([
        expect.objectContaining({
          kind: "min_distance",
          first: "S01",
          second: "S02",
        }),
      ]);
    });
  });

  it("keeps the add button disabled until the sentence is complete", async () => {
    renderWorkbench();
    await screen.findByRole("button", { name: "Student distance" });
    expect(screen.getByRole("button", { name: /Add to rules/ })).toBeDisabled();
  });

  it("toggles a constraint card off and removes it", async () => {
    const user = userEvent.setup();
    const handlers = renderWorkbench({
      constraints: [
        {
          id: "c1",
          kind: "fixed_seat",
          first: "S01",
          second: "",
          seatId: "R1C1",
          distance: 2,
          metric: "graph",
          enabled: true,
        },
      ],
    });

    const checkbox = screen.getByRole("checkbox", { name: /Fix a student to a seat/ });
    await user.click(checkbox);
    expect(handlers.onConstraintChange).toHaveBeenCalledWith("c1", {
      enabled: false,
    });

    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(handlers.onConstraintRemove).toHaveBeenCalledWith("c1");
  });

  it("shows the read-only rule JSON", async () => {
    const user = userEvent.setup();
    renderWorkbench({
      constraints: [
        {
          id: "c1",
          kind: "min_distance",
          first: "S01",
          second: "S02",
          seatId: "",
          distance: 2,
          metric: "graph",
          enabled: true,
        },
      ],
    });

    await user.click(screen.getByRole("button", { name: /View rule code/ }));
    const view = screen.getByTestId("rules-json-view");
    expect(view).toHaveTextContent('"hard_rules"');
    expect(view).toHaveTextContent('"min_distance"');
    expect(view.querySelector("pre")).not.toBeNull();
  });

  it("expands the advanced editor for complex combinations", async () => {
    const user = userEvent.setup();
    renderWorkbench();
    await user.click(screen.getByTestId("advanced-toggle"));
    expect(screen.getByTestId("rules-advanced")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Add a rule/ })).toBeInTheDocument();
  });
});
