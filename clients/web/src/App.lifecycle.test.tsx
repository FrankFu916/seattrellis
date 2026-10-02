import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "./api/client";
import * as classFiles from "./domain/classFiles";
import { demoBootstrap } from "./api/demo";
import type {
  EditorState,
  GenerateClassResponse,
  GenerateClassSolvedResponse,
  OpenClassDocumentResponse,
} from "./api/types";
import { App } from "./App";

vi.mock("./api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api/client")>()),
  loadBootstrap: vi.fn(),
  listRecentProjects: vi.fn(),
  generateClass: vi.fn(),
  generateRotationPlan: vi.fn(),
  fetchEditorState: vi.fn(),
  fetchDraftAudit: vi.fn(),
  deleteEditorDraft: vi.fn(),
  dispatchEditorCommand: vi.fn(),
  exportDraft: vi.fn(),
  serializeClassDocument: vi.fn(),
  openClassDocument: vi.fn(),
  repairEditorDraft: vi.fn(),
}));

vi.mock("./domain/classFiles", () => ({
  chooseClassSaveTarget: vi.fn().mockResolvedValue({ kind: "desktop", path: "selected.json" }),
  writeClassFile: vi.fn().mockResolvedValue("saved"),
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
  vi.spyOn(window, "confirm").mockReturnValue(true);
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
  vi.mocked(classFiles.chooseClassSaveTarget).mockResolvedValue({ kind: "desktop", path: "selected.json" });
  vi.mocked(classFiles.writeClassFile).mockResolvedValue("saved");
  vi.mocked(api.serializeClassDocument).mockImplementation(async (source) => ({
    kind: "seattrellis_class_document", schema_version: 1, class_source: source, drafts: [],
  }));
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

function beforeUnloadIsProtected(): boolean {
  const event = new Event("beforeunload", { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

describe("full class workflow", () => {
  it("preserves full student metadata across two generations", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await user.click(screen.getByRole("button", { name: "Show seating details" }));
    await user.type(screen.getByRole("spinbutton", { name: "Student 1 score" }), "92");
    await user.type(screen.getByRole("spinbutton", { name: "Student 1 height" }), "180");
    await user.type(screen.getByRole("textbox", { name: "Student 1 vision" }), "poor");
    await user.type(screen.getByRole("textbox", { name: "Student 1 needs" }), "front");
    await user.type(screen.getByRole("textbox", { name: "Student 1 notes" }), "preserve this note");
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    await user.click(screen.getByRole("button", { name: "Student roster" }));
    await user.click(screen.getByRole("button", { name: "Show seating details" }));
    expect(screen.getByRole("spinbutton", { name: "Student 1 score" })).toHaveValue(92);
    expect(screen.getByRole("textbox", { name: "Student 1 notes" })).toHaveValue("preserve this note");
    await generate(user);
    expect(api.generateClass).toHaveBeenCalledTimes(2);
    expect(vi.mocked(api.generateClass).mock.calls[1][0].draft.students[0]).toMatchObject({
      student_id: "S01", score: 92, height_cm: 180, vision: "poor", needs: ["front"], notes: "preserve this note",
    });
  });

  it("protects source edits and retains edited candidates when leaving is declined", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await user.type(screen.getByRole("textbox", { name: "Student 1 name" }), " edited");
    expect(beforeUnloadIsProtected()).toBe(true);
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    await user.click(screen.getByRole("button", { name: "Save class" }));
    await screen.findByText(/Class file saved/);
    expect(beforeUnloadIsProtected()).toBe(false);
    vi.mocked(api.dispatchEditorCommand).mockResolvedValue({ ...editor("a"), revision: 1 });
    await user.click(screen.getByRole("button", { name: "Row 1, seat 1, Alice" }));
    await user.click(screen.getByRole("button", { name: "Lock selected seat" }));
    await waitFor(() => expect(api.dispatchEditorCommand).toHaveBeenCalledOnce());
    await user.click(screen.getByRole("button", { name: "Choose plan B" }));
    expect(beforeUnloadIsProtected()).toBe(true);
    vi.mocked(window.confirm).mockReturnValue(false);
    await user.click(screen.getByRole("button", { name: /Other class/ }));
    expect(window.confirm).toHaveBeenCalledOnce();
    expect(api.deleteEditorDraft).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Choose plan A" })).toBeInTheDocument();
  });

  it("writes a class file and reopens its source and locks before generating again", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await user.click(screen.getByRole("button", { name: "Show seating details" }));
    await user.type(screen.getByRole("spinbutton", { name: "Student 1 score" }), "92");
    await user.type(screen.getByRole("textbox", { name: "Student 1 notes" }), "keep note");
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    vi.mocked(api.dispatchEditorCommand).mockResolvedValue({ ...editor("a"), revision: 1 });
    await user.click(screen.getByRole("button", { name: "Row 1, seat 1, Alice" }));
    await user.click(screen.getByRole("button", { name: "Lock selected seat" }));
    await waitFor(() => expect(api.dispatchEditorCommand).toHaveBeenCalledOnce());
    await user.click(screen.getByRole("button", { name: "Save class" }));
    await screen.findByText(/Class file saved/);
    const source = { ...vi.mocked(api.serializeClassDocument).mock.calls[0][0], name: "Opened class" };
    expect(vi.mocked(api.serializeClassDocument).mock.calls[0][1]).toContainEqual({ draft_id: "a", revision: 1 });
    expect(classFiles.writeClassFile).toHaveBeenCalledOnce();
    const reopened = editor("restored");
    reopened.seats[0].locked = true;
    reopened.students[0].locked = true;
    vi.mocked(api.openClassDocument).mockResolvedValue({ class_source: source, editor: reopened, candidates: [], period_editors: [] });
    const document = { kind: "seattrellis_class_document", schema_version: 1, class_source: source, drafts: [] };
    const file = new File([JSON.stringify(document)], "class.seattrellis.json", { type: "application/json" });
    Object.defineProperty(file, "text", { value: async () => JSON.stringify(document) });
    await user.upload(screen.getByLabelText("Open class", { selector: "input" }), file);
    await screen.findByText("Class file opened.");
    expect(beforeUnloadIsProtected()).toBe(false);
    expect(screen.getByRole("button", { name: /Row 1, seat 1, Alice/ })).toHaveClass("seat-locked");
    expect(screen.getByRole("button", { name: /Opened class saved file/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Other class/ }));
    await user.click(screen.getByRole("button", { name: /Opened class saved file/ }));
    await waitFor(() => expect(api.openClassDocument).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.openClassDocument).mock.calls[1][0]).toEqual(document);
    expect(await screen.findByRole("button", { name: /Row 1, seat 1, Alice/ })).toHaveClass("seat-locked");
    await generate(user);
    expect(vi.mocked(api.generateClass).mock.calls[1][0].draft.students[0]).toMatchObject({ score: 92, notes: "keep note" });
  });

  it("keeps downloads dirty until an actual file is opened", async () => {
    vi.mocked(classFiles.chooseClassSaveTarget).mockResolvedValue({ kind: "download" });
    vi.mocked(classFiles.writeClassFile).mockResolvedValue("downloaded");
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await user.type(screen.getByRole("textbox", { name: "Student 1 name" }), " revised");
    await user.click(screen.getByRole("button", { name: "Save class" }));
    await screen.findByText(/cannot confirm the write/);
    expect(beforeUnloadIsProtected()).toBe(true);
  });

  it("exposes local repair and preserves the source roster", async () => {
    vi.mocked(api.repairEditorDraft).mockResolvedValue({ ...editor("a"), revision: 1, undo_depth: 1 });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    await user.click(screen.getByRole("button", { name: "Repair plan (keep locks)" }));
    await waitFor(() => expect(api.repairEditorDraft).toHaveBeenCalledOnce());
    expect(api.repairEditorDraft).toHaveBeenCalledWith("a", 0, [], expect.any(AbortSignal));
    expect(beforeUnloadIsProtected()).toBe(true);
  });

  it("does not let a cancelled repair clear the busy state of the next class's repair", async () => {
    const firstRepair = deferred<EditorState>();
    const nextRepair = deferred<EditorState>();
    const nextResult: GenerateClassSolvedResponse = {
      ...result(),
      recommended_candidate_id: "c",
      editor: editor("c"),
      candidates: ["c", "d"].map((candidate_id) => ({ candidate_id, recommended: candidate_id === "c", total_score: 80 })),
    };
    vi.mocked(api.generateClass).mockResolvedValueOnce(result()).mockResolvedValueOnce(nextResult);
    vi.mocked(api.repairEditorDraft).mockReturnValueOnce(firstRepair.promise).mockReturnValueOnce(nextRepair.promise);
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    await user.click(screen.getByRole("button", { name: "Repair plan (keep locks)" }));
    const oldSignal = vi.mocked(api.repairEditorDraft).mock.calls[0][3];
    await user.click(screen.getByRole("button", { name: /Other class/ }));
    expect(oldSignal?.aborted).toBe(true);
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    await user.click(screen.getByRole("button", { name: "Repair plan (keep locks)" }));
    expect(screen.getByRole("button", { name: "Repairing plan…" })).toBeDisabled();
    await act(async () => firstRepair.resolve({ ...editor("a"), revision: 1 }));
    expect(screen.getByRole("button", { name: "Repairing plan…" })).toBeDisabled();
    expect(api.repairEditorDraft).toHaveBeenCalledTimes(2);
    await act(async () => nextRepair.resolve({ ...editor("c"), revision: 1 }));
    expect(await screen.findByRole("button", { name: "Repair plan (keep locks)" })).toBeEnabled();
  });

  it("retains an editor change that completes while an older revision is being saved", async () => {
    const command = deferred<EditorState>();
    const write = deferred<"saved">();
    vi.mocked(api.dispatchEditorCommand).mockReturnValueOnce(command.promise);
    vi.mocked(classFiles.writeClassFile).mockReturnValueOnce(write.promise);
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    await screen.findByRole("button", { name: "Choose plan B" });
    await user.click(screen.getByRole("button", { name: "Row 1, seat 1, Alice" }));
    await user.click(screen.getByRole("button", { name: "Lock selected seat" }));
    await user.click(screen.getByRole("button", { name: "Save class" }));
    await waitFor(() => expect(classFiles.writeClassFile).toHaveBeenCalledOnce());
    expect(vi.mocked(api.serializeClassDocument).mock.calls[0][1]).toContainEqual({ draft_id: "a", revision: 0 });
    const changed = { ...editor("a"), revision: 1, undo_depth: 1 };
    changed.seats[0].locked = true;
    await act(async () => command.resolve(changed));
    await act(async () => write.resolve("saved"));
    await screen.findByText(/Class file saved/);
    expect(beforeUnloadIsProtected()).toBe(true);
    await user.click(screen.getByRole("button", { name: "Choose plan B" }));
    expect(beforeUnloadIsProtected()).toBe(true);
    await user.click(screen.getByRole("button", { name: "Save class" }));
    await waitFor(() => expect(api.serializeClassDocument).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.serializeClassDocument).mock.calls[1][1]).toContainEqual({ draft_id: "a", revision: 1 });
    await waitFor(() => expect(beforeUnloadIsProtected()).toBe(false));
  });

  it("ignores a malformed class file that finishes reading after changing class", async () => {
    const text = deferred<string>();
    const file = new File(["pending"], "old-class.json", { type: "application/json" });
    Object.defineProperty(file, "text", { value: () => text.promise });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await user.upload(screen.getByLabelText("Open class", { selector: "input" }), file);
    await user.click(screen.getByRole("button", { name: /Other class/ }));
    await act(async () => text.resolve("{malformed"));
    expect(api.openClassDocument).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("aborts generation on unmount and releases drafts from a transport that returns late", async () => {
    const pending = deferred<GenerateClassResponse>();
    vi.mocked(api.generateClass).mockReturnValueOnce(pending.promise);
    const user = userEvent.setup();
    const workbench = render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    const signal = vi.mocked(api.generateClass).mock.calls[0][1];
    workbench.unmount();
    expect(signal?.aborted).toBe(true);
    await act(async () => pending.resolve(result()));
    await waitFor(() => {
      expect(api.deleteEditorDraft).toHaveBeenCalledWith("a");
      expect(api.deleteEditorDraft).toHaveBeenCalledWith("b");
    });
  });

  it("releases every draft from an open response that arrives after unmount", async () => {
    const pending = deferred<OpenClassDocumentResponse>();
    vi.mocked(api.openClassDocument).mockReturnValueOnce(pending.promise);
    const user = userEvent.setup();
    const workbench = render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await user.click(screen.getByRole("button", { name: "Save class" }));
    await screen.findByText(/Class file saved/);
    const source = vi.mocked(api.serializeClassDocument).mock.calls[0][0];
    const document = { kind: "seattrellis_class_document", schema_version: 1, class_source: source, drafts: [] };
    const file = new File([JSON.stringify(document)], "opened.json", { type: "application/json" });
    Object.defineProperty(file, "text", { value: async () => JSON.stringify(document) });
    await user.upload(screen.getByLabelText("Open class", { selector: "input" }), file);
    await waitFor(() => expect(api.openClassDocument).toHaveBeenCalledOnce());
    workbench.unmount();
    await act(async () => pending.resolve({
      class_source: source,
      editor: editor("opened"),
      candidates: [{ candidate_id: "secondary", recommended: false, total_score: 80 }],
      period_editors: [],
    }));
    await waitFor(() => {
      expect(api.deleteEditorDraft).toHaveBeenCalledWith("opened");
      expect(api.deleteEditorDraft).toHaveBeenCalledWith("secondary");
    });
  });

  it.each([
    ["ProvenInfeasible", /proven infeasible/],
    ["Timeout", /search budget ran out/],
    ["Unknown", /could not determine feasibility/],
    ["Cancelled", /request was cancelled/],
  ] as const)("explains %s without claiming all failures are infeasible", async (status, text) => {
    vi.mocked(api.generateClass).mockResolvedValue({ ...result(), status, feasible: false, editor: null, recommended_candidate_id: null, candidates: [], message_key: "solve.result", recoverable: true, suggested_action: "retry" });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    expect(await screen.findByRole("alert")).toHaveTextContent(text);
  });

  it("cancels the current generation and deletes any late returned drafts", async () => {
    const pending = deferred<GenerateClassResponse>();
    vi.mocked(api.generateClass).mockReturnValue(pending.promise);
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await generate(user);
    const signal = vi.mocked(api.generateClass).mock.calls[0][1];
    await user.click(screen.getByRole("button", { name: "Cancel generation" }));
    expect(signal?.aborted).toBe(true);
    expect(await screen.findByRole("alert")).toHaveTextContent("request was cancelled");
    await act(async () => pending.resolve(result()));
    await waitFor(() => expect(api.deleteEditorDraft).toHaveBeenCalledWith("a"));
    expect(screen.queryByRole("button", { name: "Choose plan B" })).not.toBeInTheDocument();
  });
});


describe("rotation drafts and named class files", () => {
  it("keeps the edited first period dirty after choosing a different period", async () => {
    const periods = [1, 2].map((number) => ({ ...editor(`p${number}`), candidate_id: `period-${number}` }));
    vi.mocked(api.fetchEditorState).mockImplementation(async (id) => periods.find((period) => period.draft_id === id) ?? editor(id));
    vi.mocked(api.generateRotationPlan).mockResolvedValue({ status: "Solved", feasible: true, class_name: "Class", warnings: [], failed_period: null, editor: periods[0], period_editors: periods,
      rotation_plan: { kind: "rotation_plan", name: "Class", base_history_count: 0, fairness_summary: {}, pair_repeat_summary: {}, warnings: [], periods: [1, 2].map((period) => ({ period, label: `Week ${period}`, snapshot: { solver_status: "Solved", assignments: [{ student_key: "s1", student_name: "Alice", seat_id: "Window" }] } })) } });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await user.click(screen.getByRole("button", { name: "Rules & goals" }));
    await user.click(screen.getByRole("button", { name: "Generate plan" }));
    await user.click(screen.getByText("Advanced settings"));
    await user.click(screen.getByText("Generate future rotation"));
    await user.click(screen.getByRole("checkbox", { name: "Generate multiple future periods" }));
    await user.click(screen.getByRole("button", { name: "Generate seating plan" }));
    await screen.findByRole("button", { name: "Repair plan (keep locks)" });
    await user.click(screen.getByRole("button", { name: "Save class" }));
    await screen.findByText(/Class file saved/);
    expect(beforeUnloadIsProtected()).toBe(false);
    vi.mocked(api.dispatchEditorCommand).mockResolvedValue({ ...periods[0], revision: 1 });
    await user.click(screen.getByRole("button", { name: "Row 1, seat 1, Alice" }));
    await user.click(screen.getByRole("button", { name: "Lock selected seat" }));
    await waitFor(() => expect(api.dispatchEditorCommand).toHaveBeenCalledOnce());
    await user.click(screen.getByRole("button", { name: "History / rotation" }));
    await user.click(screen.getByRole("tab", { name: "Rotation plan" }));
    await user.click(screen.getByRole("button", { name: /Week 2.*Solved/ }));
    expect(beforeUnloadIsProtected()).toBe(true);
    await user.click(screen.getByRole("button", { name: "Save class" }));
    await waitFor(() => expect(api.serializeClassDocument).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.serializeClassDocument).mock.calls[1][1]).toEqual([{ draft_id: "p1", revision: 1 }, { draft_id: "p2", revision: 0 }]);
  });

  it("saves source data before solving and retains dirty state when Save as is cancelled", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Other class/ });
    await user.type(screen.getByRole("textbox", { name: "Student 1 name" }), " revised");
    vi.mocked(classFiles.chooseClassSaveTarget).mockResolvedValueOnce(null);
    await user.click(screen.getByRole("button", { name: "Save as class" }));
    await user.type(screen.getByRole("textbox", { name: "Class name" }), "Teachers class");
    await user.click(screen.getByRole("button", { name: "Save & open" }));
    await waitFor(() => expect(classFiles.chooseClassSaveTarget).toHaveBeenCalledOnce());
    expect(api.serializeClassDocument).not.toHaveBeenCalled();
    expect(beforeUnloadIsProtected()).toBe(true);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save & open" }));
    await screen.findByText(/Class file saved/);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(vi.mocked(api.serializeClassDocument).mock.calls[0][0].name).toBe("Teachers class");
    expect(vi.mocked(api.serializeClassDocument).mock.calls[0][1]).toEqual([]);
    expect(beforeUnloadIsProtected()).toBe(false);
  });
});
