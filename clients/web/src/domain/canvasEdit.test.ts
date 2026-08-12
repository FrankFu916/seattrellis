import { describe, expect, it } from "vitest";

import { createSeatAssignments, demoStudents } from "../api/demo";
import type {
  BatchMovePayload,
  EditorOperation,
} from "../api/types";
import {
  CANVAS_GEOMETRY,
  planBatchLock,
  planBatchMove,
  seatAtPoint,
  seatCenter,
  seatsInBox,
} from "./canvasEdit";

const geometry = CANVAS_GEOMETRY;

function grid(rows: number, columns: number, seated = 0) {
  return createSeatAssignments(rows, columns, demoStudents, seated);
}

function movesOf(op: EditorOperation): BatchMovePayload["moves"] {
  if (op.kind !== "batch_move") {
    throw new Error("expected batch_move");
  }
  return (op.payload as BatchMovePayload).moves;
}

describe("seatAtPoint", () => {
  it("maps a pointer inside a seat to that seat id", () => {
    const assignments = grid(2, 3);
    // Seat R2C1 starts at margin + 0 * step, front + 1 row.
    const point = seatCenter(
      { row: 1, column: 0 },
      geometry,
    );
    expect(seatAtPoint(assignments, point)).toBe("R2C1");
  });

  it("returns null for aisles and gaps", () => {
    const assignments = grid(1, 2);
    // The horizontal gap between R1C1 and R1C2.
    const gapX =
      geometry.margin + geometry.seatWidth + geometry.columnGap / 2;
    expect(
      seatAtPoint(assignments, {
        x: gapX,
        y: geometry.margin + geometry.frontHeight + geometry.seatHeight / 2,
      }),
    ).toBeNull();
  });
});

describe("seatsInBox", () => {
  it("selects the seats whose centers fall inside the rubber band", () => {
    const assignments = grid(2, 2);
    const a = seatCenter({ row: 0, column: 0 }, geometry);
    const b = seatCenter({ row: 1, column: 1 }, geometry);
    const ids = seatsInBox(
      assignments,
      { x1: a.x, y1: a.y, x2: b.x, y2: b.y },
      new Set(),
    );
    expect(ids).toEqual(["R1C1", "R1C2", "R2C1", "R2C2"]);
  });

  it("skips locked seats", () => {
    const assignments = grid(1, 3);
    const a = seatCenter({ row: 0, column: 0 }, geometry);
    const c = seatCenter({ row: 0, column: 2 }, geometry);
    const ids = seatsInBox(
      assignments,
      { x1: a.x, y1: a.y, x2: c.x, y2: c.y },
      new Set(["R1C2"]),
    );
    expect(ids).toEqual(["R1C1", "R1C3"]);
  });
});

describe("planBatchLock", () => {
  it("builds one lock operation per seat", () => {
    expect(planBatchLock(["R1C1", "R1C2"], true)).toEqual([
      { kind: "lock_seat", payload: { seat_id: "R1C1" } },
      { kind: "lock_seat", payload: { seat_id: "R1C2" } },
    ]);
    expect(planBatchLock(["R1C1"], false)).toEqual([
      { kind: "unlock_seat", payload: { seat_id: "R1C1" } },
    ]);
  });
});

describe("planBatchMove", () => {
  it("moves the selected students into the drop region row-major", () => {
    // 3x2 grid, 4 seated: R1C1..R2C2; R3C1/R3C2 are empty.
    const assignments = grid(3, 2, 4);
    const ops = planBatchMove(assignments, ["R1C1", "R1C2"], "R3C1");
    expect(ops).toEqual([
      {
        kind: "batch_move",
        payload: {
          moves: [
            { student_key: "S01", seat_id: "R3C1" },
            { student_key: "S02", seat_id: "R3C2" },
          ],
        },
      },
    ]);
  });

  it("skips locked target seats after the drop point", () => {
    // 4x3 grid, 3 seated in row 1; drop on R2C1 with R2C2 locked.
    const assignments = grid(4, 3, 3).map((seat) =>
      seat.seatId === "R2C2" ? { ...seat, locked: true } : seat,
    );
    const ops = planBatchMove(assignments, ["R1C1", "R1C2", "R1C3"], "R2C1");
    expect(ops).toHaveLength(1);
    expect(movesOf(ops[0]).map((move) => move.seat_id)).toEqual([
      "R2C1",
      "R2C3",
      "R3C1",
    ]);
  });

  it("keeps moving students in seat order regardless of selection order", () => {
    // 2x2 grid, 2 seated in row 1; R2C1/R2C2 are empty.
    const assignments = grid(2, 2, 2);
    const ops = planBatchMove(assignments, ["R1C2", "R1C1"], "R2C1");
    expect(ops).toHaveLength(1);
    expect(movesOf(ops[0]).map((move) => move.student_key)).toEqual([
      "S01",
      "S02",
    ]);
    expect(movesOf(ops[0]).map((move) => move.seat_id)).toEqual([
      "R2C1",
      "R2C2",
    ]);
  });

  it("returns no operations when nothing can move", () => {
    const assignments = grid(2, 2, 4);
    expect(planBatchMove(assignments, [], "R2C1")).toEqual([]);
    expect(
      planBatchMove(
        assignments.map((seat) => ({ ...seat, locked: true })),
        ["R1C1"],
        "R2C1",
      ),
    ).toEqual([]);
  });
});
