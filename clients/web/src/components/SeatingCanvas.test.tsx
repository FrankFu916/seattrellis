import { fireEvent, render, screen } from "@testing-library/react";
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


describe("SeatingCanvas diagnostics badges", () => {
  it("renders a badge per issue seat and notifies on click", async () => {
    const user = userEvent.setup();
    const handleDiagnostic = vi.fn();
    const assignments = createSeatAssignments(1, 2, demoStudents.slice(0, 2)).map(
      (seat) =>
        seat.seatId === "R1C1" ? { ...seat, locked: false } : seat,
    );
    render(
      <SeatingCanvas
        assignments={assignments}
        t={createTranslator("en")}
        diagnosticBadges={{ R1C1: "error" }}
        onDiagnosticClick={handleDiagnostic}
      />,
    );

    const badge = screen.getByRole("button", { name: "R1C1: violation" });
    await user.click(badge);
    expect(handleDiagnostic).toHaveBeenCalledWith("R1C1");
  });

  it("does not start a drag when the pointer lands on a badge", () => {
    const handleSwap = vi.fn();
    const assignments = createSeatAssignments(1, 2, demoStudents.slice(0, 2));
    render(
      <SeatingCanvas
        assignments={assignments}
        t={createTranslator("en")}
        diagnosticBadges={{ R1C1: "error" }}
        onSwap={handleSwap}
      />,
    );
    // Pointer events on the badge are ignored by the drag handler.
    const badge = screen.getByRole("button", { name: "R1C1: violation" });
    fireEvent.pointerDown(badge, { pointerId: 9, clientX: 40, clientY: 40 });
    fireEvent.pointerMove(badge, { pointerId: 9, clientX: 60, clientY: 60 });
    fireEvent.pointerUp(badge, { pointerId: 9, clientX: 60, clientY: 60 });
    expect(handleSwap).not.toHaveBeenCalled();
  });
});
