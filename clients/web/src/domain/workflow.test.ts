import { createSeatAssignments, demoStudents } from "../api/demo";
import {
  deriveDiagnostics,
  getUnseatedStudents,
  reconcileStudentAssignments,
  seatRemainingStudents,
  swapStudents,
  toggleSeatLock,
} from "./workflow";

describe("seating adjustments", () => {
  it("swaps students without mutating the previous plan", () => {
    const original = createSeatAssignments(2, 2, demoStudents.slice(0, 2));
    const updated = swapStudents(original, "R1C1", "R1C2");

    expect(updated === original).toBe(false);
    expect(original[0].student?.id).toBe("S01");
    expect(updated[0].student?.id).toBe("S02");
    expect(updated[1].student?.id).toBe("S01");
  });

  it("does not move a locked seat", () => {
    const original = toggleSeatLock(
      createSeatAssignments(2, 2, demoStudents.slice(0, 2)),
      "R1C1",
    );

    expect(swapStudents(original, "R1C1", "R1C2")).toBe(original);
  });

  it("fills open seats and clears the unseated warning", () => {
    const students = demoStudents.slice(0, 4);
    const original = createSeatAssignments(2, 2, students, 2);
    const filled = seatRemainingStudents(original, students);

    expect(getUnseatedStudents(students, filled)).toHaveLength(0);
    expect(deriveDiagnostics(filled, students, null)).toEqual([
      { id: "ready", tone: "good", message: "diagnostic.ready" },
    ]);
  });

  it("reconciles roster edits without resetting geometry or locked seats", () => {
    const original = createSeatAssignments(1, 3, demoStudents.slice(0, 3), 3).map(
      (seat, index) => (index === 0 ? { ...seat, locked: true } : seat),
    );
    const edited = [
      { ...demoStudents[0], name: "Alice updated" },
      { id: "S04", name: "Dora" },
    ];

    const reconciled = reconcileStudentAssignments(original, edited);

    expect(reconciled.map((seat) => seat.seatId)).toEqual(
      original.map((seat) => seat.seatId),
    );
    expect(reconciled[0]).toMatchObject({
      locked: true,
      student: { id: "S01", name: "Alice updated" },
    });
    expect(reconciled[1].student).toMatchObject({ id: "S04", name: "Dora" });
    expect(reconciled[2].student).toBeUndefined();
  });
});
