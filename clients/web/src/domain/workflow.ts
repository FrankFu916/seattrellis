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

export type Diagnostic = {
  id: string;
  tone: "good" | "notice" | "warning";
  message:
    | "diagnostic.ready"
    | "diagnostic.unseated"
    | "diagnostic.locked"
    | "diagnostic.selection";
  count?: number;
};

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

export function seatRemainingStudents(
  assignments: SeatAssignment[],
  students: Student[],
): SeatAssignment[] {
  const remaining = [...getUnseatedStudents(students, assignments)];
  return assignments.map((seat) => {
    if (seat.student || remaining.length === 0) {
      return seat;
    }
    return { ...seat, student: remaining.shift() };
  });
}

export function deriveDiagnostics(
  assignments: SeatAssignment[],
  students: Student[],
  selectedSeatId: string | null,
): Diagnostic[] {
  const unseatedCount = getUnseatedStudents(students, assignments).length;
  const lockedCount = assignments.filter((seat) => seat.locked).length;
  const diagnostics: Diagnostic[] = [
    unseatedCount === 0
      ? { id: "ready", tone: "good", message: "diagnostic.ready" }
      : {
          id: "unseated",
          tone: "warning",
          message: "diagnostic.unseated",
          count: unseatedCount,
        },
  ];

  if (lockedCount > 0) {
    diagnostics.push({
      id: "locked",
      tone: "notice",
      message: "diagnostic.locked",
      count: lockedCount,
    });
  }

  if (selectedSeatId) {
    diagnostics.push({
      id: "selection",
      tone: "notice",
      message: "diagnostic.selection",
    });
  }

  return diagnostics;
}

export function getStepIndex(step: WorkflowStep): number {
  return workflowSteps.indexOf(step);
}

export function getAdjacentStep(
  step: WorkflowStep,
  direction: -1 | 1,
): WorkflowStep {
  const nextIndex = Math.min(
    workflowSteps.length - 1,
    Math.max(0, getStepIndex(step) + direction),
  );
  return workflowSteps[nextIndex];
}

