import { describe, expect, it } from "vitest";

import type {
  AdvancedSolveSettings,
  CommonConstraint,
  CustomRoomSettings,
  Student,
} from "../api/types";
import {
  buildGenerateClassRequest,
  InvalidAdvancedSettingError,
} from "./generation";

const students: Student[] = [
  { id: "S1", name: "Alice" },
  { id: "S2", name: "Bob" },
];

const defaults: AdvancedSolveSettings = {
  candidateCount: 1,
  seed: "",
  timeLimitSeconds: 10,
  backend: "auto",
  customRulesJson: "",
};

const defaultRoom: CustomRoomSettings = {
  enabled: false,
  rows: 5,
  columns: 6,
  aisleColumns: "",
  disabledSeats: "",
  layoutJson: "",
};

const noConstraints: CommonConstraint[] = [];

describe("buildGenerateClassRequest", () => {
  it("keeps the ordinary flow on a room template and built-in goal", () => {
    const request = buildGenerateClassRequest({
      className: "Demo class",
      students,
      selectedRoomId: "compact",
      selectedGoalId: "daily-rotation",
      settings: defaults,
      roomSettings: defaultRoom,
      constraints: noConstraints,
      preferences: [],
    });

    expect(request).toMatchObject({
      draft: {
        name: "Demo class",
        room: { template_id: "compact" },
        goal: { goal_id: "daily-rotation" },
      },
      options: {
        candidate_count: 1,
        time_limit_seconds: 10,
        backend: "auto",
      },
    });
    expect(request.draft.room).not.toHaveProperty("layout");
    expect(request.draft.goal).not.toHaveProperty("custom_rules");
  });

  it("sends custom rules, layout, and solver options when advanced settings are used", () => {
    const request = buildGenerateClassRequest({
      className: "Custom class",
      students,
      selectedRoomId: "compact",
      selectedGoalId: "daily-rotation",
      settings: {
        ...defaults,
        candidateCount: 5,
        seed: "42",
        timeLimitSeconds: 30,
        backend: "ortools",
        customRulesJson: '{"vision_front":{"enabled":true}}',
      },
      roomSettings: {
        ...defaultRoom,
        enabled: true,
        layoutJson: '{"name":"Lab","rows":1,"columns":2}',
      },
      constraints: [
        {
          id: "constraint-1",
          kind: "avoid_adjacent",
          first: "S1",
          second: "S2",
          seatId: "",
          distance: 2,
          metric: "graph",
        },
      ],
      preferences: ["fair_rotation"],
    });

    expect(request.draft.goal).toEqual({
      goal_id: "custom",
      custom_rules: { vision_front: { enabled: true } },
      hard_rules: {
        fixed_seats: [],
        must_be_adjacent: [],
        cannot_be_adjacent: [{ students: ["S1", "S2"] }],
        min_distance: [],
      },
      rules_overlay: { soft: { fair_rotation: { enabled: true } } },
    });
    expect(request.draft.room).toEqual({
      layout: { name: "Lab", rows: 1, columns: 2 },
    });
    expect(request.options).toEqual({
      candidate_count: 5,
      seed: 42,
      time_limit_seconds: 30,
      backend: "ortools",
    });
  });

  it("builds an irregular classroom from the common room controls", () => {
    const request = buildGenerateClassRequest({
      className: "Room class",
      students,
      selectedRoomId: "compact",
      selectedGoalId: "daily-rotation",
      settings: defaults,
      roomSettings: {
        ...defaultRoom,
        enabled: true,
        rows: 2,
        columns: 3,
        aisleColumns: "2",
        disabledSeats: "2-3",
        layoutJson: "",
      },
      constraints: noConstraints,
      preferences: [],
    });

    const seats = request.draft.room.layout?.seats as Array<Record<string, unknown>>;
    expect(seats).toHaveLength(6);
    expect(seats.filter((seat) => seat.enabled)).toHaveLength(3);
    expect(seats[1]).toMatchObject({ enabled: false, zone: "aisle" });
  });

  it("combines minimum distance with other hard seating requests", () => {
    const request = buildGenerateClassRequest({
      className: "Distance class",
      students,
      selectedRoomId: "compact",
      selectedGoalId: "daily-rotation",
      settings: defaults,
      roomSettings: defaultRoom,
      constraints: [
        {
          id: "distance-1",
          kind: "min_distance",
          first: "S1",
          second: "S2",
          seatId: "",
          distance: 2,
          metric: "graph",
        },
        {
          id: "fixed-1",
          kind: "fixed_seat",
          first: "S1",
          second: "",
          seatId: "R1C1",
          distance: 2,
          metric: "graph",
        },
      ],
      preferences: [],
    });

    expect(request.draft.goal.hard_rules).toEqual({
      fixed_seats: [{ student: "S1", seat_id: "R1C1" }],
      must_be_adjacent: [],
      cannot_be_adjacent: [],
      min_distance: [{ students: ["S1", "S2"], distance: 2, metric: "graph" }],
    });
  });

  it.each([
    ["rules", "[]"],
    ["layout", "not-json"],
    ["seed", "1.5"],
  ] as const)("rejects invalid %s input", (kind, value) => {
    const settings = { ...defaults };
    if (kind === "rules") settings.customRulesJson = value;
    if (kind === "layout") {
      settings.customRulesJson = "";
    }
    if (kind === "seed") settings.seed = value;

    expect(() =>
      buildGenerateClassRequest({
        className: "Demo class",
        students,
        selectedRoomId: "compact",
        selectedGoalId: "daily-rotation",
        settings,
        roomSettings:
          kind === "layout"
            ? { ...defaultRoom, enabled: true, layoutJson: value }
            : defaultRoom,
        constraints: noConstraints,
        preferences: [],
      }),
    ).toThrowError(new InvalidAdvancedSettingError(kind));
  });
});
