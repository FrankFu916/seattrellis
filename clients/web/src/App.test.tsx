import { afterEach, describe, expect, it, vi } from "vitest";

import type { EditorState } from "./api/types";
import { RosterApiError } from "./api/client";
import {
  DEFAULT_EXPORT_FORMAT,
  DEFAULT_EXPORT_TEMPLATE,
  editorToPlan,
  getInitialLocale,
  isEditableTarget,
  isRevisionConflict,
} from "./App";

describe("export defaults", () => {
  it("uses the Rust catalog id for the print-ready format", () => {
    expect(DEFAULT_EXPORT_FORMAT).toBe("print-html");
  });

  it("preserves student names unless public sharing is explicitly selected", () => {
    expect(DEFAULT_EXPORT_TEMPLATE).toBe("teacher");
  });
});

function sampleEditor(): EditorState {
  return {
    kind: "seattrellis_editor_state",
    protocol_version: "1.0",
    draft_id: "draft-1",
    revision: 3,
    candidate_id: "candidate-1",
    undo_depth: 2,
    redo_depth: 0,
    students: [
      { student_key: "S1", display_name: "Alice", seat_id: "R1C1", locked: false },
      { student_key: "S2", display_name: "Bob", seat_id: "R1C2", locked: false },
    ],
    seats: [
      { seat_id: "R1C1", row: 1, col: 1, enabled: true, student_key: "S1", locked: false },
      { seat_id: "R1C2", row: 1, col: 2, enabled: true, student_key: "S2", locked: false },
      { seat_id: "AISLE-R1C3", row: 1, col: 3, enabled: false, student_key: null, locked: false },
    ],
  };
}

describe("editorToPlan", () => {
  it("renders enabled seats as assignments with zero-based coordinates", () => {
    const { assignments } = editorToPlan(sampleEditor());

    expect(assignments).toHaveLength(2);
    expect(assignments[0]).toMatchObject({
      seatId: "R1C1",
      row: 0,
      column: 0,
      student: { id: "S1", name: "Alice" },
    });
  });

  it("skips disabled aisle cells", () => {
    const { assignments } = editorToPlan(sampleEditor());

    expect(assignments.find((seat) => seat.seatId === "AISLE-R1C3")).toBeUndefined();
  });

  it("maps every editor student to the roster model", () => {
    const { students } = editorToPlan(sampleEditor());

    expect(students).toEqual([
      { id: "S1", name: "Alice" },
      { id: "S2", name: "Bob" },
    ]);
  });
});

describe("isEditableTarget (C2: native text undo must win in form controls)", () => {
  it("is true for inputs, textareas, selects and contenteditable", () => {
    for (const tag of ["INPUT", "TEXTAREA", "SELECT"]) {
      const element = document.createElement(tag);
      expect(isEditableTarget(element)).toBe(true);
    }
    const editable = document.createElement("div");
    editable.contentEditable = "true";
    expect(isEditableTarget(editable)).toBe(true);
  });

  it("is false for plain elements and non-elements", () => {
    expect(isEditableTarget(document.createElement("button"))).toBe(false);
    expect(isEditableTarget(document.createElement("svg"))).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});

function stubEnvironment(stored: string | null, language: string) {
  vi.stubGlobal("localStorage", { getItem: () => stored });
  vi.stubGlobal("navigator", { language });
}

describe("getInitialLocale (W2: first start follows the system language)", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("prefers an explicit choice recorded in localStorage", () => {
    stubEnvironment("en", "zh-CN");
    expect(getInitialLocale()).toBe("en");

    stubEnvironment("zh-CN", "en-US");
    expect(getInitialLocale()).toBe("zh-CN");
  });

  it("falls back to the system language when nothing is stored", () => {
    stubEnvironment(null, "zh-CN");
    expect(getInitialLocale()).toBe("zh-CN");

    stubEnvironment(null, "zh-TW");
    expect(getInitialLocale()).toBe("zh-CN");

    stubEnvironment(null, "en-US");
    expect(getInitialLocale()).toBe("en");

    stubEnvironment(null, "fr");
    expect(getInitialLocale()).toBe("en");
  });

  it("ignores an unrecognized stored value and uses the system language", () => {
    stubEnvironment("fr", "zh-CN");
    expect(getInitialLocale()).toBe("zh-CN");
  });
});

describe("isRevisionConflict (W8: stale editor revision)", () => {
  it("detects the stable conflict code regardless of status", () => {
    const error = new RosterApiError(400, "editor_revision_conflict", "stale");
    expect(isRevisionConflict(error)).toBe(true);
  });

  it("detects a 409 answer from the editor command endpoint", () => {
    const error = new RosterApiError(409, "request_failed", "stale revision");
    expect(isRevisionConflict(error)).toBe(true);
  });

  it("ignores unrelated API failures", () => {
    expect(
      isRevisionConflict(new RosterApiError(400, "bad_request", "nope")),
    ).toBe(false);
    expect(isRevisionConflict(new Error("boom"))).toBe(false);
  });
});
