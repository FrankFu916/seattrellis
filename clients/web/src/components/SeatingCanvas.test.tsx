import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { createSeatAssignments, demoStudents } from "../api/demo";
import { createTranslator } from "../i18n/messages";
import { SeatingCanvas } from "./SeatingCanvas";

describe("SeatingCanvas", () => {
  it("lets a keyboard user activate a seat", async () => {
    const user = userEvent.setup();
    const handleActivate = vi.fn();
    const assignments = createSeatAssignments(
      1,
      2,
      demoStudents.slice(0, 1),
    );

    render(
      <SeatingCanvas
        assignments={assignments}
        t={createTranslator("en")}
        onSeatActivate={handleActivate}
      />,
    );

    const firstSeat = screen.getByRole("button", {
      name: /Row 1, seat 1, 林晓雨/,
    });
    firstSeat.focus();
    await user.keyboard("{Enter}");

    expect(handleActivate).toHaveBeenCalledWith("R1C1");
  });

  it("removes seat controls from a read-only preview", () => {
    render(
      <SeatingCanvas
        assignments={createSeatAssignments(
          1,
          1,
          demoStudents.slice(0, 1),
        )}
        interactive={false}
        t={createTranslator("en")}
      />,
    );

    expect(screen.queryByRole("button")).toBeNull();
    expect(
      screen.getByRole("img", { name: /Seating preview/ }),
    ).toBeVisible();
  });
});

