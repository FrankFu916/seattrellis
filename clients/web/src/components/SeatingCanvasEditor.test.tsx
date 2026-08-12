import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { createSeatAssignments, demoStudents } from "../api/demo";
import { createTranslator } from "../i18n/messages";
import { CANVAS_GEOMETRY } from "../domain/canvasEdit";
import { SeatingCanvasEditor } from "./SeatingCanvasEditor";

const t = createTranslator("en");
const geometry = CANVAS_GEOMETRY;

/** ViewBox coordinates of a seat's center (stage origin in jsdom = 0,0). */
function seatPoint(row: number, column: number) {
  return {
    x:
      geometry.margin +
      column * (geometry.seatWidth + geometry.columnGap) +
      geometry.seatWidth / 2,
    y:
      geometry.margin +
      geometry.frontHeight +
      row * (geometry.seatHeight + geometry.rowGap) +
      geometry.seatHeight / 2,
  };
}

function renderEditor(overrides: Partial<Parameters<typeof SeatingCanvasEditor>[0]> = {}) {
  const handlers = {
    onSeatActivate: vi.fn(),
    onSwap: vi.fn(),
    onBatchMove: vi.fn(),
    onLockSelection: vi.fn(),
    onUnlockSelection: vi.fn(),
    onAssign: vi.fn(),
    onUndo: vi.fn(),
    onRedo: vi.fn(),
  };
  const assignments = createSeatAssignments(2, 3, demoStudents, 4);
  render(
    <SeatingCanvasEditor
      assignments={assignments}
      students={demoStudents}
      canUndo
      canRedo
      t={t}
      {...handlers}
      {...overrides}
    />,
  );
  return { handlers, assignments };
}

describe("SeatingCanvasEditor", () => {
  it("swaps two seats by drag-lift and reports the gesture", () => {
    const { handlers } = renderEditor();
    const stage = screen.getByTestId("canvas-stage");
    const from = seatPoint(0, 0);
    const to = seatPoint(1, 1);
    const seatA = screen.getByRole("button", { name: /Row 1, seat 1, 林晓雨/ });

    fireEvent.pointerDown(seatA, { pointerId: 1, clientX: from.x, clientY: from.y });
    fireEvent.pointerMove(stage, {
      pointerId: 1,
      clientX: to.x + 5,
      clientY: to.y + 5,
    });
    fireEvent.pointerUp(stage, { pointerId: 1, clientX: to.x + 5, clientY: to.y + 5 });

    expect(handlers.onSwap).toHaveBeenCalledWith("R1C1", "R2C2");
    expect(screen.getByText(/Swapped R1C1 ↔ R2C2/)).toBeInTheDocument();
  });

  it("box-selects seats on empty space and locks them as a batch", async () => {
    const user = userEvent.setup();
    const { handlers } = renderEditor();
    const stage = screen.getByTestId("canvas-stage");
    const a = seatPoint(0, 0);
    const b = seatPoint(1, 2);

    fireEvent.pointerDown(stage, { pointerId: 2, clientX: a.x, clientY: a.y });
    fireEvent.pointerMove(stage, { pointerId: 2, clientX: b.x, clientY: b.y });
    fireEvent.pointerUp(stage, { pointerId: 2, clientX: b.x, clientY: b.y });

    expect(screen.getByTestId("selection-chip")).toHaveTextContent("6 selected");
    await user.click(screen.getByRole("button", { name: "Lock selected" }));
    expect(handlers.onLockSelection).toHaveBeenCalledWith([
      "R1C1",
      "R1C2",
      "R1C3",
      "R2C1",
      "R2C2",
      "R2C3",
    ]);
    expect(screen.getByText(/Locked 6 seats/)).toBeInTheDocument();
  });

  it("does not select locked seats in a rubber band", () => {
    const { handlers } = renderEditor({
      assignments: createSeatAssignments(1, 2, demoStudents, 2).map((seat) =>
        seat.seatId === "R1C2" ? { ...seat, locked: true } : seat,
      ),
    });
    const stage = screen.getByTestId("canvas-stage");
    const a = seatPoint(0, 0);
    const b = seatPoint(0, 1);

    fireEvent.pointerDown(stage, { pointerId: 3, clientX: a.x, clientY: a.y });
    fireEvent.pointerMove(stage, { pointerId: 3, clientX: b.x, clientY: b.y });
    fireEvent.pointerUp(stage, { pointerId: 3, clientX: b.x, clientY: b.y });

    expect(screen.getByTestId("selection-chip")).toHaveTextContent("1 selected");
  });

  it("moves a multi-selection as a batch onto a drop seat", () => {
    const { handlers } = renderEditor();
    const stage = screen.getByTestId("canvas-stage");
    const seatA = screen.getByRole("button", { name: /Row 1, seat 1, 林晓雨/ });
    const seatB = screen.getByRole("button", { name: /Row 1, seat 2, 陈子涵/ });

    // Select two seats by clicking, then drag the selection onto R2C1.
    fireEvent.click(seatA);
    fireEvent.click(seatB);
    const from = seatPoint(0, 0);
    const to = seatPoint(1, 0);
    fireEvent.pointerDown(seatA, { pointerId: 6, clientX: from.x, clientY: from.y });
    fireEvent.pointerMove(stage, { pointerId: 6, clientX: to.x + 5, clientY: to.y + 5 });
    fireEvent.pointerUp(stage, { pointerId: 6, clientX: to.x + 5, clientY: to.y + 5 });

    expect(handlers.onBatchMove).toHaveBeenCalledWith(
      expect.arrayContaining(["R1C1", "R1C2"]),
      "R2C1",
    );
  });

  it("switches to the table view and assigns a student there", async () => {
    const user = userEvent.setup();
    const { handlers } = renderEditor();
    await user.click(screen.getByRole("tab", { name: "Table" }));

    const select = screen.getByLabelText("Student at R1C1");
    await user.selectOptions(select, "S03");
    expect(handlers.onAssign).toHaveBeenCalledWith("R1C1", "S03");
    expect(screen.queryByRole("button", { name: /Row 1/ })).toBeNull();
  });

  it("notifies undo and redo from the toolbar", async () => {
    const user = userEvent.setup();
    const { handlers } = renderEditor();
    await user.click(screen.getByRole("button", { name: "Undo last change" }));
    expect(handlers.onUndo).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Redo" }));
    expect(handlers.onRedo).toHaveBeenCalled();
  });

  it("disables redo when the stack is empty", () => {
    renderEditor({ canRedo: false });
    expect(screen.getByRole("button", { name: "Redo" })).toBeDisabled();
  });

  it("opens a large focus view and exits with Escape", async () => {
    const user = userEvent.setup();
    renderEditor();

    await user.click(screen.getByRole("button", { name: "Focus view" }));
    expect(
      screen.getByRole("button", { name: "Exit focus view", pressed: true }),
    ).toBeInTheDocument();
    expect(document.querySelector(".seating-editor.is-focus-mode")).not.toBeNull();

    await user.keyboard("{Escape}");
    expect(
      screen.getByRole("button", { name: "Focus view", pressed: false }),
    ).toBeInTheDocument();
    expect(document.querySelector(".seating-editor.is-focus-mode")).toBeNull();
  });
});
