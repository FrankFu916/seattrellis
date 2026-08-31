import { describe, expect, it } from "vitest";

import type { RotationPlan } from "../api/types";
import {
  analyzeRotationMovement,
  churnRateLevel,
  transitionDistanceLevel,
  transitionSeats,
  type RotationSeatCoordinate,
} from "./rotationMovement";

const layout: RotationSeatCoordinate[] = [
  { seatId: "North", row: 0, column: 0 },
  { seatId: "Window", row: 0, column: 2 },
  { seatId: "Rear", row: 1, column: 0 },
];

const plan: RotationPlan = {
  kind: "rotation_plan",
  name: "Class 8–3",
  base_history_count: 0,
  fairness_summary: {},
  pair_repeat_summary: {},
  warnings: [],
  periods: [
    {
      period: 1,
      label: "Week 1",
      snapshot: {
        solver_status: "Solved",
        assignments: [
          { student_key: "S1", student_name: "Alice", seat_id: "North" },
          { student_key: "S2", student_name: "Bob", seat_id: "Window" },
        ],
      },
    },
    {
      period: 2,
      label: "Week 2",
      snapshot: {
        solver_status: "Solved",
        assignments: [
          { student_key: "S1", student_name: "Alice", seat_id: "Window" },
          { student_key: "S2", student_name: "Bob", seat_id: "North" },
        ],
      },
    },
    {
      period: 3,
      label: "Week 3",
      snapshot: {
        solver_status: "Solved",
        assignments: [
          { student_key: "S1", student_name: "Alice", seat_id: "Window" },
          { student_key: "S2", student_name: "Bob", seat_id: "Rear" },
        ],
      },
    },
  ],
};

describe("rotation movement analysis", () => {
  it("aggregates adjacent moves and seat churn using explicit coordinates", () => {
    const analysis = analyzeRotationMovement(
      {
        ...plan,
        periods: [plan.periods[2], plan.periods[0], plan.periods[1]],
      },
      layout,
    );

    expect(analysis.periods.map((period) => period.period)).toEqual([1, 2, 3]);
    expect(analysis.summary).toMatchObject({
      transitionCount: 2,
      comparableStudentCount: 4,
      movementEventCount: 3,
      uniqueMovedStudentCount: 2,
      stayedEventCount: 1,
      knownDistanceCount: 3,
      maximumDistance: 2,
    });
    expect(analysis.summary.averageDistance).toBeCloseTo(5 / 3);

    expect(
      analysis.seatChurn.map((seat) => ({
        seatId: seat.seatId,
        changes: seat.occupantChangeCount,
        level: seat.level,
      })),
    ).toEqual([
      { seatId: "North", changes: 2, level: "very-high" },
      { seatId: "Window", changes: 1, level: "medium" },
      { seatId: "Rear", changes: 1, level: "medium" },
    ]);

    const lastTransitionSeats = transitionSeats(
      analysis.transitions[1],
      analysis.seats,
    );
    expect(lastTransitionSeats.find((seat) => seat.seatId === "Window")).toMatchObject({
      studentKey: "S1",
      fromSeatId: "Window",
      distance: 0,
      level: "stable",
    });
    expect(lastTransitionSeats.find((seat) => seat.seatId === "Rear")).toMatchObject({
      studentKey: "S2",
      fromSeatId: "North",
      distance: 1,
      level: "low",
    });
  });

  it("keeps move facts when coordinates are missing and tracks roster changes", () => {
    const partialPlan: RotationPlan = {
      ...plan,
      periods: [
        {
          period: 1,
          label: "Before",
          snapshot: {
            solver_status: "Solved",
            assignments: [
              { student_key: "S1", student_name: "Alice", seat_id: "Legacy" },
              { student_key: "S3", student_name: "Cara", seat_id: "Old" },
            ],
          },
        },
        {
          period: 2,
          label: "After",
          snapshot: {
            solver_status: "Solved",
            assignments: [
              { student_key: "S1", student_name: "Alice", seat_id: "Target" },
              { student_key: "S2", student_name: "Bob", seat_id: "Other" },
            ],
          },
        },
      ],
    };

    const analysis = analyzeRotationMovement(partialPlan, [
      { seatId: "Target", row: 0, column: 0 },
    ]);
    const transition = analysis.transitions[0];

    expect(transition.metrics).toMatchObject({
      comparableStudentCount: 1,
      movedCount: 1,
      stayedCount: 0,
      seatedCount: 1,
      unseatedCount: 1,
      knownDistanceCount: 0,
      averageDistance: null,
      maximumDistance: null,
    });
    expect(transition.movements.find((movement) => movement.studentKey === "S1")).toMatchObject({
      status: "moved",
      distance: null,
    });
    expect(transitionSeats(transition, analysis.seats)[0]).toMatchObject({
      seatId: "Target",
      level: "unknown",
    });
    expect(analysis.seatChurn[0]).toMatchObject({
      seatId: "Target",
      occupantChangeCount: 1,
      changeRate: 1,
      level: "very-high",
    });
  });

  it("uses fixed, comparable heat bands", () => {
    expect(churnRateLevel(0)).toBe("stable");
    expect(churnRateLevel(0.25)).toBe("low");
    expect(churnRateLevel(0.5)).toBe("medium");
    expect(churnRateLevel(0.75)).toBe("high");
    expect(churnRateLevel(1)).toBe("very-high");

    expect(transitionDistanceLevel(0)).toBe("stable");
    expect(transitionDistanceLevel(1)).toBe("low");
    expect(transitionDistanceLevel(2)).toBe("medium");
    expect(transitionDistanceLevel(3)).toBe("high");
    expect(transitionDistanceLevel(4)).toBe("very-high");
    expect(transitionDistanceLevel(null)).toBe("unknown");
  });
});
