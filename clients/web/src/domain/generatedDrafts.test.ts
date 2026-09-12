import { beforeEach, describe, expect, it, vi } from "vitest";
import { deleteEditorDraft, fetchEditorState } from "../api/client";
import type {
  EditorState,
  GenerateClassSolvedResponse,
  GenerateRotationPlanSolvedResponse,
} from "../api/types";
import { consumeGeneratedDrafts } from "./generatedDrafts";

vi.mock("../api/client", () => ({
  deleteEditorDraft: vi.fn(),
  fetchEditorState: vi.fn(),
}));
const editor = (id: string): EditorState => ({
  kind: "seattrellis_editor_state",
  protocol_version: "1.0",
  draft_id: id,
  candidate_id: id,
  revision: 0,
  undo_depth: 0,
  redo_depth: 0,
  students: [],
  seats: [],
});
const generated = (): GenerateClassSolvedResponse => ({
  status: "Solved",
  feasible: true,
  class_name: "Class",
  goal: {
    goal_id: "balanced",
    title: "Balanced",
    description: "",
    preset_name: null,
  },
  recommended_candidate_id: "a",
  editor: editor("a"),
  warnings: [],
  candidates: ["a", "b", "c"].map((candidate_id) => ({
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
  vi.mocked(fetchEditorState).mockImplementation(async (id) => editor(id));
  vi.mocked(deleteEditorDraft).mockResolvedValue(undefined);
});

describe("generated draft ownership", () => {
  it("fetches each candidate once in parallel and commits a complete generation", async () => {
    const pending = [
      deferred<EditorState>(),
      deferred<EditorState>(),
      deferred<EditorState>(),
    ];
    vi.mocked(fetchEditorState).mockImplementation(
      (id) => pending[["a", "b", "c"].indexOf(id)].promise,
    );
    const commit = vi.fn();
    const task = consumeGeneratedDrafts(generated(), () => true, commit);
    expect(fetchEditorState).toHaveBeenCalledTimes(3);
    pending[0].resolve(editor("a"));
    pending[1].resolve(editor("b"));
    await Promise.resolve();
    expect(commit).not.toHaveBeenCalled();
    pending[2].resolve(editor("c"));
    await task;
    expect(commit).toHaveBeenCalledOnce();
    expect(commit.mock.calls[0][0].candidates).toHaveLength(3);
    expect(deleteEditorDraft).not.toHaveBeenCalled();
  });

  it("disposes a response that arrived after a context reset without fetching", async () => {
    const commit = vi.fn();
    await consumeGeneratedDrafts(generated(), () => false, commit);
    expect(fetchEditorState).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(vi.mocked(deleteEditorDraft).mock.calls).toEqual([
      ["a"],
      ["b"],
      ["c"],
    ]);
  });

  it("cleans every newly created draft when superseded during hydration", async () => {
    let current = true;
    const pending = deferred<EditorState>();
    vi.mocked(fetchEditorState).mockImplementation((id) =>
      id === "b" ? pending.promise : Promise.resolve(editor(id)),
    );
    const commit = vi.fn();
    const task = consumeGeneratedDrafts(generated(), () => current, commit);
    current = false;
    pending.resolve(editor("b"));
    await task;
    expect(commit).not.toHaveBeenCalled();
    expect(deleteEditorDraft).toHaveBeenCalledTimes(3);
  });

  it("keeps the previous UI intact and disposes all drafts if the primary cannot load", async () => {
    vi.mocked(fetchEditorState).mockRejectedValueOnce(new Error("evicted"));
    const commit = vi.fn();
    await expect(
      consumeGeneratedDrafts(generated(), () => true, commit),
    ).rejects.toThrow("evicted");
    expect(commit).not.toHaveBeenCalled();
    expect(deleteEditorDraft).toHaveBeenCalledTimes(3);
  });

  it("tolerates a missing alternative and disposes only that draft", async () => {
    vi.mocked(fetchEditorState).mockImplementation(async (id) => {
      if (id === "b") throw new Error("evicted");
      return editor(id);
    });
    const commit = vi.fn();
    await consumeGeneratedDrafts(generated(), () => true, commit);
    expect(
      commit.mock.calls[0][0].candidates.map(
        (item: { editor: EditorState }) => item.editor.draft_id,
      ),
    ).toEqual(["a", "c"]);
    expect(vi.mocked(deleteEditorDraft).mock.calls).toEqual([["b"]]);
  });

  it("disposes every rotation period once, even when a deletion fails", async () => {
    const response: GenerateRotationPlanSolvedResponse = {
      status: "Solved",
      feasible: true,
      class_name: "Class",
      warnings: [],
      failed_period: null,
      editor: editor("a"),
      period_editors: [editor("a"), editor("b")],
      rotation_plan: {
        kind: "rotation_plan",
        name: "Class",
        periods: [],
        base_history_count: 0,
        fairness_summary: {},
        pair_repeat_summary: {},
        warnings: [],
      },
    };
    vi.mocked(deleteEditorDraft).mockRejectedValueOnce(new Error("offline"));
    await consumeGeneratedDrafts(response, () => false, vi.fn());
    expect(vi.mocked(deleteEditorDraft).mock.calls).toEqual([["a"], ["b"]]);
  });
});
