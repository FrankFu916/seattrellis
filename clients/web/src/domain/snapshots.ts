import type { SeatAssignment, Student } from "../api/types";
import type { HistorySnapshotPayload } from "../api/types";

/**
 * History snapshot restore (D7): a v1.x snapshot document (students, layout,
 * rules, assignments) becomes the workbench's current plan. Seats are
 * matched by id against the current room so geometry changes degrade
 * gracefully; students come from the snapshot roster.
 */

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
    .filter((entry): entry is Record<string, unknown> => entry !== null && typeof entry === "object" && !Array.isArray(entry))
    .map((entry) => ({
      id: String(entry.student_id ?? entry.id ?? entry.key ?? ""),
      name: String(entry.name ?? entry.display_name ?? ""),
      ...((entry.gender === null || typeof entry.gender === "string") ? { gender: entry.gender } : {}),
      ...((entry.height_cm === null || (typeof entry.height_cm === "number" && Number.isFinite(entry.height_cm))) ? { heightCm: entry.height_cm } : {}),
      ...((entry.score === null || (typeof entry.score === "number" && Number.isFinite(entry.score))) ? { score: entry.score } : {}),
      ...((entry.vision === null || typeof entry.vision === "string" || (typeof entry.vision === "number" && Number.isFinite(entry.vision))) ? { vision: entry.vision } : {}),
      ...(Array.isArray(entry.tags) && entry.tags.every((value) => typeof value === "string") ? { tags: entry.tags } : {}),
      ...(Array.isArray(entry.needs) && entry.needs.every((value) => typeof value === "string") ? { needs: entry.needs } : {}),
      ...((entry.notes === null || typeof entry.notes === "string") ? { notes: entry.notes } : {}),
      ...(entry.attributes !== null && typeof entry.attributes === "object" && !Array.isArray(entry.attributes) ? { attributes: entry.attributes as Record<string, unknown> } : {}),
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
  const assignedStudentIds = new Set<string>();
  const next: SeatAssignment[] = currentAssignments.map((seat) => ({
    ...seat,
    student: undefined,
  }));
  for (const entry of raw as SnapshotAssignment[]) {
    if (!entry || typeof entry !== "object") continue;
    const studentId = String(entry.student_key ?? entry.student ?? "");
    const seatId = String(entry.seat_id ?? "");
    const student = studentsById.get(studentId);
    if (!student || !bySeatId.has(seatId) || assignedSeatIds.has(seatId) || assignedStudentIds.has(studentId)) {
      continue;
    }
    const target = next.find((seat) => seat.seatId === seatId);
    if (target && !target.locked) {
      target.student = student;
      assignedSeatIds.add(seatId);
      assignedStudentIds.add(studentId);
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
