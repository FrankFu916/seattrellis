import type {
  RotationPeriod,
  RotationPlan,
  SeatAssignment,
} from "../api/types";

export type RotationHeatLevel =
  | "empty"
  | "stable"
  | "low"
  | "medium"
  | "high"
  | "very-high"
  | "unknown";

export type RotationSeatCoordinate = Pick<
  SeatAssignment,
  "seatId" | "row" | "column"
>;

export type StudentMovementStatus =
  | "moved"
  | "stayed"
  | "seated"
  | "unseated";

export type StudentMovement = {
  studentKey: string;
  studentName: string;
  status: StudentMovementStatus;
  fromSeatId: string | null;
  toSeatId: string | null;
  distance: number | null;
};

export type RotationTransitionMetrics = {
  comparableStudentCount: number;
  movedCount: number;
  stayedCount: number;
  seatedCount: number;
  unseatedCount: number;
  knownDistanceCount: number;
  averageDistance: number | null;
  maximumDistance: number | null;
};

export type RotationTransition = {
  from: RotationPeriod;
  to: RotationPeriod;
  movements: StudentMovement[];
  metrics: RotationTransitionMetrics;
};

export type RotationSeatChurn = RotationSeatCoordinate & {
  occupantChangeCount: number;
  transitionCount: number;
  changeRate: number;
  hasOccupant: boolean;
  level: RotationHeatLevel;
};

export type RotationTransitionSeat = RotationSeatCoordinate & {
  studentKey: string | null;
  studentName: string | null;
  fromSeatId: string | null;
  distance: number | null;
  level: RotationHeatLevel;
};

export type RotationMovementSummary = {
  transitionCount: number;
  comparableStudentCount: number;
  movementEventCount: number;
  uniqueMovedStudentCount: number;
  stayedEventCount: number;
  seatedEventCount: number;
  unseatedEventCount: number;
  knownDistanceCount: number;
  averageDistance: number | null;
  maximumDistance: number | null;
};

export type RotationMovementAnalysis = {
  periods: RotationPeriod[];
  seats: RotationSeatCoordinate[];
  transitions: RotationTransition[];
  seatChurn: RotationSeatChurn[];
  summary: RotationMovementSummary;
};

type PlanAssignment = RotationPeriod["snapshot"]["assignments"][number];

function orderedPeriods(plan: RotationPlan): RotationPeriod[] {
  return [...plan.periods].sort((left, right) => left.period - right.period);
}

function orderedSeats(
  seats: RotationSeatCoordinate[],
): RotationSeatCoordinate[] {
  const byId = new Map<string, RotationSeatCoordinate>();
  for (const seat of seats) {
    if (
      seat.seatId &&
      Number.isFinite(seat.row) &&
      Number.isFinite(seat.column) &&
      seat.row >= 0 &&
      seat.column >= 0
    ) {
      byId.set(seat.seatId, {
        seatId: seat.seatId,
        row: seat.row,
        column: seat.column,
      });
    }
  }
  return [...byId.values()].sort(
    (left, right) =>
      left.row - right.row ||
      left.column - right.column ||
      left.seatId.localeCompare(right.seatId),
  );
}

function assignmentMaps(period: RotationPeriod): {
  byStudent: Map<string, PlanAssignment>;
  bySeat: Map<string, PlanAssignment>;
} {
  const byStudent = new Map<string, PlanAssignment>();
  const bySeat = new Map<string, PlanAssignment>();
  for (const assignment of period.snapshot.assignments) {
    if (assignment.student_key) {
      byStudent.set(assignment.student_key, assignment);
    }
    if (assignment.seat_id) {
      bySeat.set(assignment.seat_id, assignment);
    }
  }
  return { byStudent, bySeat };
}

function manhattanDistance(
  fromSeatId: string,
  toSeatId: string,
  coordinateBySeat: Map<string, RotationSeatCoordinate>,
): number | null {
  if (fromSeatId === toSeatId) {
    return 0;
  }
  const from = coordinateBySeat.get(fromSeatId);
  const to = coordinateBySeat.get(toSeatId);
  if (!from || !to) {
    return null;
  }
  return Math.abs(from.row - to.row) + Math.abs(from.column - to.column);
}

export function transitionDistanceLevel(
  distance: number | null,
): RotationHeatLevel {
  if (distance === null) {
    return "unknown";
  }
  if (distance === 0) {
    return "stable";
  }
  if (distance === 1) {
    return "low";
  }
  if (distance === 2) {
    return "medium";
  }
  if (distance === 3) {
    return "high";
  }
  return "very-high";
}

export function churnRateLevel(rate: number): RotationHeatLevel {
  if (rate <= 0) {
    return "stable";
  }
  if (rate <= 0.25) {
    return "low";
  }
  if (rate <= 0.5) {
    return "medium";
  }
  if (rate <= 0.75) {
    return "high";
  }
  return "very-high";
}

function comparePeriods(
  from: RotationPeriod,
  to: RotationPeriod,
  coordinateBySeat: Map<string, RotationSeatCoordinate>,
): RotationTransition {
  const previous = assignmentMaps(from).byStudent;
  const current = assignmentMaps(to).byStudent;
  const studentKeys = new Set([...previous.keys(), ...current.keys()]);
  const movements: StudentMovement[] = [];

  for (const studentKey of studentKeys) {
    const before = previous.get(studentKey);
    const after = current.get(studentKey);
    if (before && after) {
      const status = before.seat_id === after.seat_id ? "stayed" : "moved";
      movements.push({
        studentKey,
        studentName: after.student_name || before.student_name || studentKey,
        status,
        fromSeatId: before.seat_id,
        toSeatId: after.seat_id,
        distance: manhattanDistance(
          before.seat_id,
          after.seat_id,
          coordinateBySeat,
        ),
      });
      continue;
    }
    if (after) {
      movements.push({
        studentKey,
        studentName: after.student_name || studentKey,
        status: "seated",
        fromSeatId: null,
        toSeatId: after.seat_id,
        distance: null,
      });
      continue;
    }
    if (before) {
      movements.push({
        studentKey,
        studentName: before.student_name || studentKey,
        status: "unseated",
        fromSeatId: before.seat_id,
        toSeatId: null,
        distance: null,
      });
    }
  }

  movements.sort(
    (left, right) =>
      (right.distance ?? -1) - (left.distance ?? -1) ||
      left.studentName.localeCompare(right.studentName),
  );

  const moved = movements.filter((movement) => movement.status === "moved");
  const knownDistances = moved
    .map((movement) => movement.distance)
    .filter((distance): distance is number => distance !== null);
  const distanceTotal = knownDistances.reduce((sum, distance) => sum + distance, 0);

  return {
    from,
    to,
    movements,
    metrics: {
      comparableStudentCount: movements.filter(
        (movement) => movement.status === "moved" || movement.status === "stayed",
      ).length,
      movedCount: moved.length,
      stayedCount: movements.filter((movement) => movement.status === "stayed").length,
      seatedCount: movements.filter((movement) => movement.status === "seated").length,
      unseatedCount: movements.filter((movement) => movement.status === "unseated").length,
      knownDistanceCount: knownDistances.length,
      averageDistance:
        knownDistances.length > 0 ? distanceTotal / knownDistances.length : null,
      maximumDistance:
        knownDistances.length > 0 ? Math.max(...knownDistances) : null,
    },
  };
}

export function transitionSeats(
  transition: RotationTransition,
  seats: RotationSeatCoordinate[],
): RotationTransitionSeat[] {
  const currentBySeat = assignmentMaps(transition.to).bySeat;
  const movementByStudent = new Map(
    transition.movements.map((movement) => [movement.studentKey, movement]),
  );

  return seats.map((seat) => {
    const assignment = currentBySeat.get(seat.seatId);
    if (!assignment) {
      return {
        ...seat,
        studentKey: null,
        studentName: null,
        fromSeatId: null,
        distance: null,
        level: "empty",
      };
    }
    const movement = movementByStudent.get(assignment.student_key);
    return {
      ...seat,
      studentKey: assignment.student_key,
      studentName: assignment.student_name || assignment.student_key,
      fromSeatId: movement?.fromSeatId ?? null,
      distance: movement?.distance ?? null,
      level:
        movement?.status === "stayed"
          ? "stable"
          : transitionDistanceLevel(movement?.distance ?? null),
    };
  });
}

export function analyzeRotationMovement(
  plan: RotationPlan,
  layoutSeats: RotationSeatCoordinate[],
): RotationMovementAnalysis {
  const periods = orderedPeriods(plan);
  const seats = orderedSeats(layoutSeats);
  const coordinateBySeat = new Map(seats.map((seat) => [seat.seatId, seat]));
  const transitions = periods.slice(1).map((period, index) =>
    comparePeriods(periods[index], period, coordinateBySeat),
  );
  const periodSeatMaps = periods.map((period) => assignmentMaps(period).bySeat);

  const seatChurn = seats.map((seat): RotationSeatChurn => {
    const occupants = periodSeatMaps.map(
      (assignments) => assignments.get(seat.seatId)?.student_key ?? null,
    );
    let occupantChangeCount = 0;
    for (let index = 1; index < occupants.length; index += 1) {
      if (occupants[index - 1] !== occupants[index]) {
        occupantChangeCount += 1;
      }
    }
    const transitionCount = Math.max(0, periods.length - 1);
    const changeRate =
      transitionCount > 0 ? occupantChangeCount / transitionCount : 0;
    const hasOccupant = occupants.some((occupant) => occupant !== null);
    return {
      ...seat,
      occupantChangeCount,
      transitionCount,
      changeRate,
      hasOccupant,
      level: hasOccupant ? churnRateLevel(changeRate) : "empty",
    };
  });

  const movedStudentKeys = new Set<string>();
  const knownDistances: number[] = [];
  for (const transition of transitions) {
    for (const movement of transition.movements) {
      if (movement.status === "moved") {
        movedStudentKeys.add(movement.studentKey);
        if (movement.distance !== null) {
          knownDistances.push(movement.distance);
        }
      }
    }
  }
  const distanceTotal = knownDistances.reduce((sum, distance) => sum + distance, 0);

  return {
    periods,
    seats,
    transitions,
    seatChurn,
    summary: {
      transitionCount: transitions.length,
      comparableStudentCount: transitions.reduce(
        (sum, transition) => sum + transition.metrics.comparableStudentCount,
        0,
      ),
      movementEventCount: transitions.reduce(
        (sum, transition) => sum + transition.metrics.movedCount,
        0,
      ),
      uniqueMovedStudentCount: movedStudentKeys.size,
      stayedEventCount: transitions.reduce(
        (sum, transition) => sum + transition.metrics.stayedCount,
        0,
      ),
      seatedEventCount: transitions.reduce(
        (sum, transition) => sum + transition.metrics.seatedCount,
        0,
      ),
      unseatedEventCount: transitions.reduce(
        (sum, transition) => sum + transition.metrics.unseatedCount,
        0,
      ),
      knownDistanceCount: knownDistances.length,
      averageDistance:
        knownDistances.length > 0 ? distanceTotal / knownDistances.length : null,
      maximumDistance:
        knownDistances.length > 0 ? Math.max(...knownDistances) : null,
    },
  };
}
