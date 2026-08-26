import type { SeatAssignment, Student } from "../api/types";

export const workflowSteps = [
  "roster",
  "room",
  "goal",
  "generate",
  "adjust",
  "export",
] as const;

export type WorkflowStep = (typeof workflowSteps)[number];

export function getUnseatedStudents(
  students: Student[],
  assignments: SeatAssignment[],
): Student[] {
  const seatedIds = new Set(
    assignments.flatMap((seat) => (seat.student ? [seat.student.id] : [])),
  );
  return students.filter((student) => !seatedIds.has(student.id));
}

export function swapStudents(
  assignments: SeatAssignment[],
  firstSeatId: string,
  secondSeatId: string,
): SeatAssignment[] {
  if (firstSeatId === secondSeatId) {
    return assignments;
  }

  const first = assignments.find((seat) => seat.seatId === firstSeatId);
  const second = assignments.find((seat) => seat.seatId === secondSeatId);
  if (!first || !second || first.locked || second.locked) {
    return assignments;
  }

  return assignments.map((seat) => {
    if (seat.seatId === firstSeatId) {
      return { ...seat, student: second.student };
    }
    if (seat.seatId === secondSeatId) {
      return { ...seat, student: first.student };
    }
    return seat;
  });
}

export function toggleSeatLock(
  assignments: SeatAssignment[],
  seatId: string,
): SeatAssignment[] {
  return assignments.map((seat) =>
    seat.seatId === seatId ? { ...seat, locked: !seat.locked } : seat,
  );
}

/**
 * Place one student on a seat, or clear the seat when `student` is null.
 *
 * The student's previous seat (if any) is emptied in the same pass so the
 * optimistic local update can never leave one student occupying two seats;
 * this mirrors what the Rust editor applies for `move_student`. Callers
 * resolve the student from the roster so unseated students can be placed
 * too.
 */
export function assignStudentToSeat(
  assignments: SeatAssignment[],
  seatId: string,
  student: Student | null,
): SeatAssignment[] {
  return assignments.map((seat) => {
    if (seat.seatId === seatId) {
      return { ...seat, student: student ?? undefined };
    }
    if (student && seat.student?.id === student.id) {
      return { ...seat, student: undefined };
    }
    return seat;
  });
}

export function seatRemainingStudents(
  assignments: SeatAssignment[],
  students: Student[],
): SeatAssignment[] {
  const remaining = [...getUnseatedStudents(students, assignments)];
  return assignments.map((seat) => {
    if (seat.student || seat.locked || remaining.length === 0) {
      return seat;
    }
    return { ...seat, student: remaining.shift() };
  });
}

/**
 * Keep a draft seating plan usable while the roster is edited in place.
 *
 * Existing students retain their seats, removed students are cleared, and
 * newly added students are placed in the first available unlocked seats. The
 * function intentionally preserves the current room geometry and lock state;
 * editing a name or score must not unexpectedly reset a teacher's layout.
 */
export function reconcileStudentAssignments(
  assignments: SeatAssignment[],
  students: Student[],
): SeatAssignment[] {
  const studentsById = new Map(students.map((student) => [student.id, student]));
  const reconciled = assignments.map((seat) => {
    if (!seat.student) {
      return seat;
    }
    const student = studentsById.get(seat.student.id);
    return student ? { ...seat, student } : { ...seat, student: undefined };
  });
  return seatRemainingStudents(reconciled, students);
}
