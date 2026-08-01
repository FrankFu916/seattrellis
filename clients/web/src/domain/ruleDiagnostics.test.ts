import { describe, expect, it } from "vitest";

import { diagnoseRuleSetJson } from "./ruleDiagnostics";

const students = [
  { id: "S1", name: "Alice" },
  { id: "S2", name: "Bob" },
];

describe("diagnoseRuleSetJson", () => {
  it("reports syntax and root-shape errors before generation", () => {
    expect(diagnoseRuleSetJson("{", students, ["R1C1"])).toEqual([
      { path: "$", code: "invalid_json" },
    ]);
    expect(diagnoseRuleSetJson("[]", students, ["R1C1"])).toEqual([
      { path: "$", code: "root_object" },
    ]);
  });

  it("reports unsupported fields and unknown roster references", () => {
    const diagnostics = diagnoseRuleSetJson(
      JSON.stringify({
        hard: {
          fixed_seats: [{ student: "S9", seat_id: "R9C9" }],
          cannot_be_adjacent: [{ students: ["S1", "S2"] }],
        },
        extra: true,
      }),
      students,
      ["R1C1"],
    );
    expect(diagnostics).toEqual(
      expect.arrayContaining([
        { path: "extra", code: "unknown_field" },
        { path: "hard.fixed_seats[0].student", code: "unknown_student" },
        { path: "hard.fixed_seats[0].seat_id", code: "unknown_seat" },
      ]),
    );
  });

  it("checks groups and hard rule value shapes", () => {
    const diagnostics = diagnoseRuleSetJson(
      JSON.stringify({
        hard: {
          must_be_adjacent: [{ students: ["S1"] }],
          min_distance: [{ students: ["S1", "S2"], distance: 0 }],
        },
        groups: [
          { name: "Pair", students: ["S1"], separate: true, together: true },
          { name: "Pair", students: ["S1", "S2"] },
        ],
      }),
      students,
      ["R1C1"],
    );
    expect(diagnostics).toEqual(
      expect.arrayContaining([
        { path: "hard.must_be_adjacent[0].students", code: "pair_shape" },
        { path: "hard.min_distance[0].distance", code: "distance_value" },
        { path: "groups[0].students", code: "group_members" },
        { path: "groups[0]", code: "group_mode" },
        { path: "groups[1].name", code: "group_shape" },
      ]),
    );
  });

  it("accepts a valid complete rules object", () => {
    expect(
      diagnoseRuleSetJson(
        JSON.stringify({
          schema_version: 1,
          seed: 7,
          hard: {
            fixed_seats: [{ student: "S1", seat_id: "R1C1" }],
            must_be_adjacent: [{ students: ["S1", "S2"] }],
          },
          soft: { fair_rotation: { enabled: true, weight: 10 } },
          groups: [{ name: "Pair", students: ["S1", "S2"], together: true }],
        }),
        students,
        ["R1C1"],
      ),
    ).toEqual([]);
  });
});
