import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "./api/client";
import { demoBootstrap } from "./api/demo";
import type {
  EditorState,
  GenerateClassResponse,
  GenerateClassSolvedResponse,
} from "./api/types";
import { App } from "./App";

vi.mock("./api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api/client")>()),
  loadBootstrap: vi.fn(),
  listRecentProjects: vi.fn(),
  generateClass: vi.fn(),
  fetchEditorState: vi.fn(),
  fetchDraftAudit: vi.fn(),
  deleteEditorDraft: vi.fn(),
  dispatchEditorCommand: vi.fn(),
  exportDraft: vi.fn(),
}));

vi.mock("./domain/desktop", async (original) => ({
  ...(await original<typeof import("./domain/desktop")>()),
  saveBlobWithDialog: vi.fn().mockResolvedValue("saved"),
}));

const editor = (id: string): EditorState => ({
  kind: "seattrellis_editor_state",
  protocol_version: "1.0",
  draft_id: id,
  candidate_id: id,
  revision: 0,
  undo_depth: 0,
  redo_depth: 0,
  students: [
    {
      student_key: "s1",
      display_name: "Alice",
      seat_id: "Window",
      locked: false,
    },
  ],
  seats: [
    {
      seat_id: "Window",
      row: 1,
      col: 1,
      enabled: true,
      student_key: "s1",
      locked: false,
    },
  ],
});
const result = (): GenerateClassSolvedResponse => ({
  status: "Solved",
  feasible: true,
  class_name: "Class",
  warnings: [],
  recommended_candidate_id: "a",
  editor: editor("a"),
  goal: {
    goal_id: "balanced",
    title: "Balanced",
    description: "",
    preset_name: null,
  },
  candidates: ["a", "b"].map((candidate_id) => ({
    candidate_id,
    recommended: candidate_id === "a",
    total_score: 80,
  })),
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
beforeEach(() => {
  vi.resetAllMocks();
  window.localStorage.clear();
  window.localStorage.setItem("seattrellis-locale", "en");
  window.localStorage.setItem("seattrellis-first-run:v1", "done");
  vi.stubGlobal(
    "fetch",
    vi.fn().mockRejectedValue(new Error("No extra requests in this test")),
  );
  vi.mocked(api.loadBootstrap).mockResolvedValue({
    ...demoBootstrap,
    source: "local",
  });
  vi.mocked(api.listRecentProjects).mockResolvedValue({
    api_version: "1",
    root: ".",
    projects: [
      {
        name: "Other class",
        path: "other",
        modified_at: "2026-09-12T00:00:00Z",
      },
    ],
  });
  vi.mocked(api.fetchEditorState).mockImplementation(async (id) => editor(id));
  vi.mocked(api.fetchDraftAudit).mockRejectedValue(
    new Error("audit unavailable"),
  );
  vi.mocked(api.deleteEditorDraft).mockResolvedValue(undefined);
  vi.mocked(api.generateClass).mockResolvedValue(result());
});
afterEach(() => vi.unstubAllGlobals());

async function generate(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Rules & goals" }));
  await user.click(screen.getByRole("button", { name: "Generate plan" }));
  await user.click(
    screen.getByRole("button", { name: "Generate seating plan" }),
  );
}

describe("workbench asynchronous ownership", () => {
  it("aborts an export when leaving its class and ignores the late file", async () => {
    vi.stubGlobal(
      "URL",
      class extends URL {
        static createObjectURL = vi.fn(() => "blob:late-export");
        static revokeObjectURL = vi.fn();
      },
    );
    const pending = deferred<Awaited<ReturnType<typeof api.exportDraft>>>();
    vi.mocked(api.exportDraft).mockReturnValueOnce(pending.promise);
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    await user.click(screen.getByRole("button", { name: "Export" }));
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await user.click(screen.getByRole("button", { name: /Other class/ }));
    expect(vi.mocked(api.exportDraft).mock.calls[0][1]?.aborted).toBe(true);
    await act(async () =>
      pending.resolve({
        blob: new Blob(["late file"]),
        filename: "old-class.pdf",
        warnings: [],
      }),
    );
    expect(URL.createObjectURL).not.toHaveBeenCalled();
    expect(
      screen.queryByRole("button", { name: "Save file" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Choose classroom" }),
    ).toBeEnabled();
  });

  it("does not treat exporting a seating image as saving the editable project", async () => {
    vi.stubGlobal(
      "URL",
      class extends URL {
        static createObjectURL = vi.fn(() => "blob:export");
        static revokeObjectURL = vi.fn();
      },
    );
    vi.mocked(api.exportDraft).mockResolvedValue({
      blob: new Blob(
        [
          '<html><body><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 70"><text>Alice</text></svg></body></html>',
        ],
        { type: "text/html" },
      ),
      filename: "seat-plan.html",
      warnings: [],
    });
    vi.mocked(api.dispatchEditorCommand).mockResolvedValue({
      ...editor("a"),
      revision: 1,
    });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    await user.click(
      screen.getByRole("button", { name: "Row 1, seat 1, Alice" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Lock selected seat" }),
    );
    await waitFor(() =>
      expect(api.dispatchEditorCommand).toHaveBeenCalledOnce(),
    );
    await user.click(screen.getByRole("button", { name: "Export" }));
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await screen.findByTitle("Seating chart in the generated file");
    await user.click(screen.getByRole("button", { name: "Save file" }));
    await screen.findByText(/Sent to local saving/);
    const beforeUnload = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(beforeUnload);
    expect(beforeUnload.defaultPrevented).toBe(true);
  });

  it("unblocks a new class immediately and deletes a late generation without showing it", async () => {
    const pending = deferred<GenerateClassResponse>();
    vi.mocked(api.generateClass).mockReturnValueOnce(pending.promise);
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    expect(api.generateClass).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: /Other class/ }));
    expect(
      screen.getByRole("button", { name: "Choose classroom" }),
    ).toBeEnabled();
    await act(async () => {
      pending.resolve(result());
    });
    await waitFor(() =>
      expect(api.deleteEditorDraft).toHaveBeenCalledWith("b"),
    );
    expect(api.fetchEditorState).not.toHaveBeenCalled();
    expect(screen.queryByText("Alice")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Choose classroom" }),
    ).toBeEnabled();
  });

  it("ignores a candidate switch that resolves after leaving its class", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    const pending = deferred<EditorState>();
    vi.mocked(api.fetchEditorState).mockReturnValueOnce(pending.promise);
    await user.click(screen.getByRole("button", { name: "Choose plan B" }));
    await user.click(screen.getByRole("button", { name: /Other class/ }));
    await act(async () => {
      pending.resolve(editor("b"));
    });
    expect(screen.queryByText("Alice")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Choose classroom" }),
    ).toBeEnabled();
  });

  it("ignores an editor command that resolves after leaving its class", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    const pending = deferred<EditorState>();
    vi.mocked(api.dispatchEditorCommand).mockReturnValueOnce(pending.promise);
    await user.click(
      screen.getByRole("button", { name: "Row 1, seat 1, Alice" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Lock selected seat" }),
    );
    expect(api.dispatchEditorCommand).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: /Other class/ }));
    await act(async () => {
      pending.resolve({ ...editor("a"), revision: 1 });
    });
    expect(screen.queryByText("Alice")).not.toBeInTheDocument();
  });
});
