import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CommonConstraint, Student } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { BulkConstraintEditor } from "./BulkConstraintEditor";

const students: Student[] = [
  { id: "S01", name: "Alice" },
  { id: "S02", name: "Bob" },
  { id: "S03", name: "Chen" },
];

const existing: CommonConstraint[] = [
  {
    id: "existing",
    kind: "avoid_adjacent",
    first: "S01",
    second: "S02",
    seatId: "",
    distance: 2,
    metric: "graph",
  },
];

describe("BulkConstraintEditor", () => {
  it("previews valid pairs and explains unknown or duplicate rows", async () => {
    const user = userEvent.setup();
    render(
      <BulkConstraintEditor
        students={students}
        seatIds={["R1C1", "R1C2"]}
        existingConstraints={existing}
        t={createTranslator("en")}
        onAdd={vi.fn()}
      />,
    );

    await user.click(screen.getByTestId("bulk-constraint-editor").querySelector("summary")!);
    await user.type(
      screen.getByTestId("bulk-constraint-input"),
      "S01, S02\nS02 -> S03\nS99, S01",
    );

    expect(screen.getByTestId("bulk-constraint-preview")).toHaveTextContent("1 ready to add");
    expect(screen.getByTestId("bulk-constraint-preview")).toHaveTextContent(
      "this relationship already exists",
    );
    expect(screen.getByTestId("bulk-constraint-preview")).toHaveTextContent(
      "student “S99” was not found",
    );
  });

  it("adds fixed-seat rows and clears the input", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    render(
      <BulkConstraintEditor
        students={students}
        seatIds={["R1C1", "R1C2"]}
        existingConstraints={[]}
        t={createTranslator("en")}
        onAdd={onAdd}
      />,
    );

    await user.click(screen.getByTestId("bulk-constraint-editor").querySelector("summary")!);
    await user.selectOptions(screen.getByTestId("bulk-constraint-kind"), "fixed_seat");
    const input = screen.getByTestId("bulk-constraint-input");
    await user.type(input, "S01, R1C1\nS02, R1C2");
    await user.click(screen.getByTestId("bulk-constraint-apply"));

    expect(onAdd).toHaveBeenCalledTimes(1);
    expect(onAdd.mock.calls[0][0]).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ kind: "fixed_seat", first: "S01", seatId: "R1C1" }),
        expect.objectContaining({ kind: "fixed_seat", first: "S02", seatId: "R1C2" }),
      ]),
    );
    expect(input).toHaveValue("");
  });
});

