import type { EditorOperation, SeatAssignment } from "../api/types";

/**
 * Pure geometry + batch-operation helpers for the interactive canvas (D2).
 * All edits are expressed as Rust editing-protocol operations; the React
 * side only computes *which* operations a gesture implies (G-2).
 */

export const CANVAS_GEOMETRY = {
  seatWidth: 116,
  seatHeight: 70,
  columnGap: 18,
  rowGap: 18,
  margin: 28,
  frontHeight: 64,
} as const;

export type CanvasGeometry = typeof CANVAS_GEOMETRY;

export function seatPosition(
  assignment: Pick<SeatAssignment, "row" | "column">,
  geometry: CanvasGeometry = CANVAS_GEOMETRY,
): { x: number; y: number } {
  return {
    x: geometry.margin + assignment.column * (geometry.seatWidth + geometry.columnGap),
    y:
      geometry.margin +
      geometry.frontHeight +
      assignment.row * (geometry.seatHeight + geometry.rowGap),
  };
}

export function seatCenter(
  assignment: Pick<SeatAssignment, "row" | "column">,
  geometry: CanvasGeometry = CANVAS_GEOMETRY,
): { x: number; y: number } {
  const position = seatPosition(assignment, geometry);
  return {
    x: position.x + geometry.seatWidth / 2,
    y: position.y + geometry.seatHeight / 2,
  };
}

/** Seat ids whose center falls inside the box (viewBox coordinates). */
export function seatsInBox(
  assignments: SeatAssignment[],
  box: { x1: number; y1: number; x2: number; y2: number },
  lockedSeatIds: ReadonlySet<string>,
  geometry: CanvasGeometry = CANVAS_GEOMETRY,
): string[] {
  const x1 = Math.min(box.x1, box.x2);
  const x2 = Math.max(box.x1, box.x2);
  const y1 = Math.min(box.y1, box.y2);
  const y2 = Math.max(box.y1, box.y2);
  return assignments
    .filter((seat) => !lockedSeatIds.has(seat.seatId))
    .filter((seat) => {
      const center = seatCenter(seat, geometry);
      return center.x >= x1 && center.x <= x2 && center.y >= y1 && center.y <= y2;
    })
    .map((seat) => seat.seatId);
}

/**
 * Map a pointer position (viewBox coordinates) to the seat underneath.
 * Returns the seat id, or null when the pointer is on an empty area
 * (including aisles).
 */
export function seatAtPoint(
  assignments: SeatAssignment[],
  point: { x: number; y: number },
  geometry: CanvasGeometry = CANVAS_GEOMETRY,
): string | null {
  for (const seat of assignments) {
    const position = seatPosition(seat, geometry);
    if (
      point.x >= position.x &&
      point.x <= position.x + geometry.seatWidth &&
      point.y >= position.y &&
      point.y <= position.y + geometry.seatHeight
    ) {
      return seat.seatId;
    }
  }
  return null;
}

/** Lock or unlock a set of seats as one atomic editing command. */
export function planBatchLock(
  seatIds: string[],
  lock: boolean,
): EditorOperation[] {
  return seatIds.map((seatId) => ({
    kind: lock ? "lock_seat" : "unlock_seat",
    payload: { seat_id: seatId },
  }));
}

/**
 * Batch move: place the selected students into the region starting at the
 * drop seat, row-major, skipping locked or already-occupied (non-selected)
 * seats. Emits one atomic `batch_move` operation (Rust editing protocol),
 * or an empty array when nothing can move.
 */
export function planBatchMove(
  assignments: SeatAssignment[],
  selectedIds: string[],
  dropSeatId: string,
  geometry: CanvasGeometry = CANVAS_GEOMETRY,
): EditorOperation[] {
  const selected = new Set(selectedIds);
  const moving = assignments
    .filter((seat) => selected.has(seat.seatId) && seat.student)
    .sort((a, b) => {
      const pa = seatPosition(a, geometry);
      const pb = seatPosition(b, geometry);
      return pa.y - pb.y || pa.x - pb.x;
    });
  if (moving.length === 0) {
    return [];
  }
  const occupiedByOthers = new Set(
    assignments
      .filter((seat) => seat.student && !selected.has(seat.seatId))
      .map((seat) => seat.seatId),
  );

  const sorted = [...assignments].sort((a, b) => {
    const pa = seatPosition(a, geometry);
    const pb = seatPosition(b, geometry);
    return pa.y - pb.y || pa.x - pb.x;
  });
  const dropIndex = sorted.findIndex((seat) => seat.seatId === dropSeatId);
  if (dropIndex === -1) {
    return [];
  }

  const targets: string[] = [];
  for (
    let index = dropIndex;
    index < sorted.length && targets.length < moving.length;
    index += 1
  ) {
    const seat = sorted[index];
    if (seat.locked || occupiedByOthers.has(seat.seatId)) {
      continue;
    }
    targets.push(seat.seatId);
  }
  if (targets.length < moving.length) {
    return [];
  }

  return [
    {
      kind: "batch_move",
      payload: {
        moves: moving.map((seat, index) => ({
          student_key: seat.student!.id,
          seat_id: targets[index],
        })),
      },
    },
  ];
}
