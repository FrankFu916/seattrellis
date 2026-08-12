import { describe, expect, it } from "vitest";

import { createSeatAssignments, demoStudents } from "../api/demo";
import {
  snapshotAssignments,
  snapshotIsRestorable,
  snapshotStudents,
} from "./snapshots";

const SNAPSHOT = {
  students: [
    { student_id: "S01", name: "丁一" },
    { student_id: "S02", name: "万二" },
  ],
  layout: {},
  rules: {},
  assignments: [
    { student_key: "S01", seat_id: "R1C1" },
    { student_key: "S02", seat_id: "R1C2" },
  ],
};

describe("snapshotStudents", () => {
  it("extracts the snapshot roster", () => {
    expect(snapshotStudents(SNAPSHOT)).toEqual([
      { id: "S01", name: "丁一" },
      { id: "S02", name: "万二" },
    ]);
  });
});

describe("snapshotAssignments", () => {
  it("places snapshot students into the current room by seat id", () => {
    const current = createSeatAssignments(2, 2, demoStudents, 4);
    const students = snapshotStudents(SNAPSHOT);
    const next = snapshotAssignments(SNAPSHOT, current, students);
    expect(next.find((seat) => seat.seatId === "R1C1")?.student?.id).toBe("S01");
    expect(next.find((seat) => seat.seatId === "R1C2")?.student?.id).toBe("S02");
    expect(next.find((seat) => seat.seatId === "R2C1")?.student).toBeUndefined();
  });

  it("skips seats missing from the current room and locked seats", () => {
    const current = createSeatAssignments(1, 2, demoStudents, 2).map((seat) =>
      seat.seatId === "R1C2" ? { ...seat, locked: true } : seat,
    );
    const students = snapshotStudents(SNAPSHOT);
    const next = snapshotAssignments(SNAPSHOT, current, students);
    expect(next.find((seat) => seat.seatId === "R1C1")?.student?.id).toBe("S01");
    expect(next.find((seat) => seat.seatId === "R1C2")?.student).toBeUndefined();
  });
});

describe("snapshotIsRestorable", () => {
  it("requires both a roster and assignments", () => {
    expect(snapshotIsRestorable(SNAPSHOT)).toBe(true);
    expect(snapshotIsRestorable({ students: [], assignments: [] })).toBe(false);
    expect(snapshotIsRestorable({})).toBe(false);
  });
});
