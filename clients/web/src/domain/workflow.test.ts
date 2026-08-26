import { createSeatAssignments, demoStudents } from "../api/demo";
import {
  assignStudentToSeat,
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

  it("moves a seated student and empties the previous seat in one pass", () => {
    const original = createSeatAssignments(1, 3, demoStudents.slice(0, 2), 2);

    const moved = assignStudentToSeat(original, "R1C3", original[0].student!);

    expect(moved[0].student).toBeUndefined();
    expect(moved[1].student?.id).toBe("S02");
    expect(moved[2].student?.id).toBe("S01");
    const occupied = moved.filter((seat) => seat.student).length;
    expect(occupied).toBe(2);
  });

  it("seats an unseated student resolved from the roster", () => {
    const original = createSeatAssignments(1, 2, demoStudents.slice(0, 1), 1);
    const newcomer = { id: "S09", name: "胡可欣" };

    const moved = assignStudentToSeat(original, "R1C2", newcomer);

    expect(moved[1].student).toEqual(newcomer);
    expect(moved[0].student?.id).toBe("S01");
  });

  it("clears a seat when the assignment is removed", () => {
    const original = createSeatAssignments(1, 2, demoStudents.slice(0, 2), 2);

    const cleared = assignStudentToSeat(original, "R1C1", null);

    expect(cleared[0].student).toBeUndefined();
    expect(cleared[1].student?.id).toBe("S02");
  });
});
