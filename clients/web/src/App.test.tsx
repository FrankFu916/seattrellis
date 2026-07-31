import { describe, expect, it } from "vitest";

import type { EditorState } from "./api/types";
import { editorToPlan } from "./App";

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
