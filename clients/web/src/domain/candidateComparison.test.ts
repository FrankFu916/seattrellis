import { describe, expect, it } from "vitest";
import type { SeatAssignment } from "../api/types";
import {
  candidateLabel,
  compareCandidateStudents,
} from "./candidateComparison";

const seat = (
  seatId: string,
  id?: string,
  name = "Same name",
): SeatAssignment => ({
  seatId,
  row: 0,
  column: 0,
  locked: false,
  student: id ? { id, name } : undefined,
});

describe("candidate student comparison", () => {
  it("matches stable IDs despite duplicate names and reports seating/unseating", () => {
    const changes = compareCandidateStudents(
      [
        seat("North", "1"),
        seat("Window", "2"),
        seat("Rear", "3"),
        seat("Extra"),
      ],
      [seat("North", "2"), seat("Window", "1"), seat("Rear", "4")],
    );
    expect(changes).toEqual([
      {
        studentId: "1",
        studentName: "Same name",
        fromSeatId: "North",
        toSeatId: "Window",
        changed: true,
      },
      {
        studentId: "2",
        studentName: "Same name",
        fromSeatId: "Window",
        toSeatId: "North",
        changed: true,
      },
      {
        studentId: "3",
        studentName: "Same name",
        fromSeatId: "Rear",
        toSeatId: null,
        changed: true,
      },
      {
        studentId: "4",
        studentName: "Same name",
        fromSeatId: null,
        toSeatId: "Rear",
        changed: true,
      },
    ]);
  });

  it("does not count renamed students as moved and ignores empty seats", () => {
    expect(
      compareCandidateStudents(
        [seat("Desk", "1", "Old")],
        [seat("Desk", "1", "New"), seat("Empty")],
      ),
    ).toEqual([
      {
        studentId: "1",
        studentName: "New",
        fromSeatId: "Desk",
        toSeatId: "Desk",
        changed: false,
      },
    ]);
    expect(compareCandidateStudents([], [])).toEqual([]);
  });

  it("labels candidates beyond Z", () => {
    expect([0, 25, 26, 27, 51, 52].map(candidateLabel)).toEqual([
      "A",
      "Z",
      "AA",
      "AB",
      "AZ",
      "BA",
    ]);
  });
});
