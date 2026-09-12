import type { SeatAssignment } from "../api/types";

export type CandidateMovement = {
  studentId: string;
  studentName: string;
  fromSeatId: string | null;
  toSeatId: string | null;
  changed: boolean;
};

/** Match by stable student identity, including newly seated/unseated students. */
export function compareCandidateStudents(
  before: SeatAssignment[],
  after: SeatAssignment[],
): CandidateMovement[] {
  const previous = new Map(
    before.flatMap((seat) =>
      seat.student ? [[seat.student.id, seat] as const] : [],
    ),
  );
  const current = new Map(
    after.flatMap((seat) =>
      seat.student ? [[seat.student.id, seat] as const] : [],
    ),
  );
  return [...new Set([...previous.keys(), ...current.keys()])].map(
    (studentId) => {
      const from = previous.get(studentId);
      const to = current.get(studentId);
      return {
        studentId,
        studentName: to?.student?.name ?? from?.student?.name ?? studentId,
        fromSeatId: from?.seatId ?? null,
        toSeatId: to?.seatId ?? null,
        changed: from?.seatId !== to?.seatId,
      };
    },
  );
}

/** Spreadsheet-style labels remain readable for candidate sets larger than 26. */
export function candidateLabel(index: number): string {
  let value = index + 1;
  let label = "";
  while (value > 0) {
    value -= 1;
    label = String.fromCharCode(65 + (value % 26)) + label;
    value = Math.floor(value / 26);
  }
  return label;
}
