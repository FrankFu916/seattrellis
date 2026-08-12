import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import type { Student } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { StudentRosterEditor } from "./StudentRosterEditor";

const initialStudents: Student[] = [
  { id: "S01", name: "Alice", score: 92, needs: ["front"] },
  { id: "S02", name: "Bob", score: 84, needs: [] },
];

function Harness() {
  const [students, setStudents] = useState(initialStudents);
  return (
    <StudentRosterEditor
      students={students}
      t={createTranslator("en")}
      onChange={setStudents}
    />
  );
}

describe("StudentRosterEditor", () => {
  it("keeps optional seating data out of the default editing path", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    expect(screen.queryByRole("spinbutton", { name: "Student 1 score" })).toBeNull();
    const details = screen.getByRole("button", { name: "Show seating details" });
    expect(details).toHaveAttribute("aria-expanded", "false");

    await user.click(details);
    expect(screen.getByRole("spinbutton", { name: "Student 1 score" })).toHaveValue(92);
    expect(screen.getByRole("button", { name: "Hide seating details" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("edits fields, adds a student, and removes a row", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const firstName = screen.getByRole("textbox", { name: "Student 1 name" });
    await user.clear(firstName);
    await user.type(firstName, "Alicia");
    expect(firstName).toHaveValue("Alicia");

    await user.click(screen.getByRole("button", { name: "Add student" }));
    expect(screen.getByRole("textbox", { name: "Student 3 ID" })).toHaveValue("S03");
    expect(screen.getByRole("alert")).toHaveTextContent("non-empty, unique ID and name");

    await user.click(screen.getByRole("button", { name: "Remove Bob" }));
    expect(screen.queryByRole("button", { name: "Remove Bob" })).not.toBeInTheDocument();
  });

  it("offers the sample roster when the list is empty (D10)", async () => {
    const user = userEvent.setup();
    const onUseDemo = vi.fn();
    render(
      <StudentRosterEditor
        students={[]}
        t={createTranslator("zh-CN")}
        onChange={vi.fn()}
        onUseDemo={onUseDemo}
      />,
    );

    expect(screen.getByText(/名单为空/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "使用示例名单" }));
    expect(onUseDemo).toHaveBeenCalledTimes(1);
  });
});
