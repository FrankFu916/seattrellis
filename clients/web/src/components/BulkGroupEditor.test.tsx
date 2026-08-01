import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CommonGroupRule, Student } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { BulkGroupEditor } from "./BulkGroupEditor";

const students: Student[] = [
  { id: "S01", name: "Alice" },
  { id: "S02", name: "Bob" },
  { id: "S03", name: "Chen" },
  { id: "S04", name: "Dai" },
];

const existing: CommonGroupRule[] = [
  { id: "existing", name: "Lab A", mode: "separate", students: ["S01", "S02"] },
];

describe("BulkGroupEditor", () => {
  it("previews groups and reports malformed or unknown members", async () => {
    const user = userEvent.setup();
    render(
      <BulkGroupEditor
        students={students}
        existingGroups={existing}
        t={createTranslator("en")}
        onAdd={vi.fn()}
      />,
    );

    await user.click(screen.getByTestId("bulk-group-editor").querySelector("summary")!);
    await user.type(
      screen.getByTestId("bulk-group-input"),
      "Lab A: S01, S03\nLab B: S02, S04\nBroken: S99, S03",
    );

    expect(screen.getByTestId("bulk-group-preview")).toHaveTextContent(
      "1 groups ready to add",
    );
    expect(screen.getByTestId("bulk-group-preview")).toHaveTextContent(
      "group name “Lab A” already exists",
    );
    expect(screen.getByTestId("bulk-group-preview")).toHaveTextContent(
      "student “S99” was not found",
    );
  });

  it("adds together groups and clears the source", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    render(
      <BulkGroupEditor
        students={students}
        existingGroups={[]}
        t={createTranslator("en")}
        onAdd={onAdd}
      />,
    );

    await user.click(screen.getByTestId("bulk-group-editor").querySelector("summary")!);
    await user.selectOptions(screen.getByTestId("bulk-group-mode"), "together");
    const input = screen.getByTestId("bulk-group-input");
    await user.type(input, "Lab A: S01, S02, S03\nLab B: S02, S03, S04");
    await user.click(screen.getByTestId("bulk-group-apply"));

    expect(onAdd).toHaveBeenCalledTimes(1);
    expect(onAdd.mock.calls[0][0]).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: "Lab A", mode: "together", students: ["S01", "S02", "S03"] }),
        expect.objectContaining({ name: "Lab B", mode: "together", students: ["S02", "S03", "S04"] }),
      ]),
    );
    expect(input).toHaveValue("");
  });
});
