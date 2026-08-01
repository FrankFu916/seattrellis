import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

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
});
