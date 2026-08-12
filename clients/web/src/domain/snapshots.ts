import type { SeatAssignment, Student } from "../api/types";
import type { HistorySnapshotPayload } from "../api/types";

/**
 * History snapshot restore (D7): a v1.x snapshot document (students, layout,
 * rules, assignments) becomes the workbench's current plan. Seats are
 * matched by id against the current room so geometry changes degrade
 * gracefully; students come from the snapshot roster.
 */

type SnapshotStudent = {
  student_id?: string;
  id?: string;
  name?: string;
  display_name?: string;
};

type SnapshotAssignment = {
  student_key?: string;
  student?: string;
  seat_id?: string;
};

export function snapshotStudents(
  snapshot: HistorySnapshotPayload,
): Student[] {
  const raw = snapshot.students;
  if (!Array.isArray(raw)) {
    return [];
  }
  return raw
    .map((entry) => entry as SnapshotStudent)
    .map((entry) => ({
      id: String(entry.student_id ?? entry.id ?? ""),
      name: String(entry.name ?? entry.display_name ?? ""),
    }))
    .filter((student) => student.id && student.name);
}

export function snapshotAssignments(
  snapshot: HistorySnapshotPayload,
  currentAssignments: SeatAssignment[],
  students: Student[],
): SeatAssignment[] {
  const raw = snapshot.assignments;
  if (!Array.isArray(raw)) {
    return currentAssignments;
  }
  const studentsById = new Map(students.map((student) => [student.id, student]));
  const bySeatId = new Map(
    currentAssignments.map((seat) => [seat.seatId, seat]),
  );
  const assignedSeatIds = new Set<string>();
  const next: SeatAssignment[] = currentAssignments.map((seat) => ({
    ...seat,
    student: undefined,
  }));
  for (const entry of raw as SnapshotAssignment[]) {
    const studentId = String(entry.student_key ?? entry.student ?? "");
    const seatId = String(entry.seat_id ?? "");
    const student = studentsById.get(studentId);
    if (!student || !bySeatId.has(seatId) || assignedSeatIds.has(seatId)) {
      continue;
    }
    const target = next.find((seat) => seat.seatId === seatId);
    if (target && !target.locked) {
      target.student = student;
      assignedSeatIds.add(seatId);
    }
  }
  return next;
}

/** Whether a snapshot can be restored at all (has a parseable roster). */
export function snapshotIsRestorable(
  snapshot: HistorySnapshotPayload,
): boolean {
  return (
    Array.isArray(snapshot.students) &&
    Array.isArray(snapshot.assignments) &&
    snapshotStudents(snapshot).length > 0
  );
}
