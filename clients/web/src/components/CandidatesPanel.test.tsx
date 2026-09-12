import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fetchDraftAudit } from "../api/client";
import type { DraftAuditReport } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { CandidatesPanel, type CandidateMeta } from "./CandidatesPanel";

vi.mock("../api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api/client")>()),
  fetchDraftAudit: vi.fn(),
}));
const t = createTranslator("en");
const candidates: CandidateMeta[] = ["a", "b", "c"].map((draft_id, index) => ({
  draft_id,
  total_score: 80 - index,
  recommended: index === 0,
  revision: 0,
  assignments: [
    {
      seatId: "Window",
      row: 0,
      column: 0,
      locked: false,
      student: {
        id: index === 1 ? "2" : "1",
        name: index === 1 ? "Bob" : "Alice",
      },
    },
    {
      seatId: "Back",
      row: 1,
      column: 2,
      locked: false,
      student: {
        id: index === 1 ? "1" : "2",
        name: index === 1 ? "Alice" : "Bob",
      },
    },
  ],
}));
const props = {
  candidates,
  t,
  locale: "en" as const,
  repro: {
    seed: "42",
    solver: "native",
    timeLimitSeconds: 10,
    historyCount: 0,
  },
};
const report = (id: string, score: number): DraftAuditReport => {
  const hard = {
    all_satisfied: true,
    checked_rule_count: 1,
    violation_count: 0,
    witnesses: [],
  };
  const unavailable = {
    status: "not_available" as const,
    score: null,
    weight: 0,
    details: {},
  };
  return {
    api_version: "2",
    draft_id: id,
    feasible: true,
    score: {
      total: score,
      breakdown: {
        fair_rotation_score: {
          status: "available",
          score,
          weight: 10,
          details: {},
        },
        avoid_recent_neighbors_score: unavailable,
        score_balance_score: unavailable,
        height_preference_score: unavailable,
        vision_preference_score: unavailable,
        diversity_score: unavailable,
        stability_score: unavailable,
        rule_scores: {},
        hard_constraint_summary: hard,
      },
    },
    audit: {
      hard_constraint_summary: hard,
      missing_data: {
        students_missing_score: 0,
        students_missing_height: 0,
        students_missing_vision: 0,
        students_missing_needs: 0,
      },
      history: { snapshot_count: 0, has_history: false },
      suggested_actions: [],
    },
  };
};
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(fetchDraftAudit).mockImplementation(async (id) =>
    report(id, { a: 80, b: 70, c: 60 }[id] ?? 50),
  );
});

describe("candidate comparison", () => {
  it("renders arbitrary seat IDs at their coordinates and changes comparison without choosing", async () => {
    const user = userEvent.setup();
    const onChoose = vi.fn();
    const { container } = render(
      <CandidatesPanel {...props} onChoose={onChoose} activeDraftId="a" />,
    );
    expect(screen.getAllByTitle("Window")).toHaveLength(2);
    expect(screen.getAllByTitle("Back")[0]).toHaveStyle({
      gridRow: "2",
      gridColumn: "3",
    });
    expect(container.querySelectorAll(".mini-cell-diff")).toHaveLength(4);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Left comparison plan" }),
      "c",
    );
    expect(onChoose).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Choose plan B" }));
    expect(onChoose).toHaveBeenCalledWith("b");
  });

  it("keeps both selectors available for self-comparison and lets users recover", async () => {
    const user = userEvent.setup();
    render(<CandidatesPanel {...props} onChoose={vi.fn()} />);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Right comparison plan" }),
      "a",
    );
    expect(
      screen.getByText("Seats that differ between the plans (0)"),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("combobox")).toHaveLength(2);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Right comparison plan" }),
      "b",
    );
    expect(
      screen.getByText("Seats that differ between the plans (2)"),
    ).toBeInTheDocument();
  });

  it("filters student changes by ID and can include unchanged students", async () => {
    const user = userEvent.setup();
    render(<CandidatesPanel {...props} onChoose={vi.fn()} />);
    await user.click(screen.getByText("Student seat changes (2 changed)"));
    await user.type(screen.getByRole("searchbox"), "1");
    const table = screen.getByRole("table", { name: "Compare plans A and B" });
    expect(
      within(table).getByRole("row", { name: "Alice 1 Window Back" }),
    ).toBeInTheDocument();
    expect(within(table).queryByText("Bob")).not.toBeInTheDocument();
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Right comparison plan" }),
      "c",
    );
    expect(
      screen.getByText("No students match these filters."),
    ).toBeInTheDocument();
    await user.click(
      screen.getByRole("checkbox", { name: "Only students with changes" }),
    );
    expect(
      within(table).getByRole("row", { name: "Alice 1 Window Window" }),
    ).toBeInTheDocument();
  });

  it("compares scores for the selected pair, not always the recommendation", async () => {
    const user = userEvent.setup();
    render(<CandidatesPanel {...props} onChoose={vi.fn()} />);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Left comparison plan" }),
      "c",
    );
    await waitFor(() =>
      expect(screen.getByRole("cell", { name: "60" })).toBeInTheDocument(),
    );
    expect(screen.getByRole("cell", { name: "70" })).toBeInTheDocument();
    expect(screen.queryByRole("cell", { name: "80" })).not.toBeInTheDocument();
  });

  it("falls back to valid choices when a regenerated candidate set replaces the old IDs", async () => {
    const user = userEvent.setup();
    const view = render(<CandidatesPanel {...props} onChoose={vi.fn()} />);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Left comparison plan" }),
      "c",
    );
    view.rerender(
      <CandidatesPanel
        {...props}
        onChoose={vi.fn()}
        candidates={candidates
          .slice(0, 2)
          .map((candidate) => ({
            ...candidate,
            draft_id: `new-${candidate.draft_id}`,
            recommended: false,
          }))}
      />,
    );
    expect(
      screen.getByRole("combobox", { name: "Left comparison plan" }),
    ).toHaveValue("new-a");
    expect(
      screen.getByRole("combobox", { name: "Right comparison plan" }),
    ).toHaveValue("new-b");
    expect(screen.getByText(/Recommended · A/)).toBeInTheDocument();
  });

  it("refreshes audits when a draft revision changes", async () => {
    const view = render(<CandidatesPanel {...props} onChoose={vi.fn()} />);
    await screen.findByRole("cell", { name: "80" });
    vi.mocked(fetchDraftAudit).mockImplementation(async (id) => report(id, 25));
    view.rerender(
      <CandidatesPanel
        {...props}
        onChoose={vi.fn()}
        candidates={candidates.map((candidate) => ({
          ...candidate,
          revision: 1,
        }))}
      />,
    );
    expect(screen.queryByRole("cell", { name: "80" })).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getAllByRole("cell", { name: "25" })).toHaveLength(2),
    );
  });

  it("still offers previews and selection when audit data is unavailable", async () => {
    vi.mocked(fetchDraftAudit).mockRejectedValue(new Error("offline"));
    render(<CandidatesPanel {...props} onChoose={vi.fn()} />);
    await screen.findByRole("alert");
    expect(screen.getAllByRole("combobox")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Choose plan B" })).toBeEnabled();
  });
});
