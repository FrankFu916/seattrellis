import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  compileLayoutDraft,
  createLayoutDraft,
  deleteLayoutDraft,
  dispatchLayoutCommand,
} from "../api/client";
import type { CustomRoomSettings, LayoutStateResponse } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { LayoutEditorPanel } from "./LayoutEditorPanel";

vi.mock("../api/client", () => ({
  compileLayoutDraft: vi.fn(),
  createLayoutDraft: vi.fn(),
  deleteLayoutDraft: vi.fn(),
  dispatchLayoutCommand: vi.fn(),
  RosterApiError: class RosterApiError extends Error {},
}));

const settings: CustomRoomSettings = {
  enabled: true,
  rows: 2,
  columns: 2,
  aisleColumns: "",
  disabledSeats: "",
  layoutJson: "",
};

const initialState: LayoutStateResponse = {
  api_version: "1",
  kind: "seattrellis_layout_state",
  draft_id: "layout-1",
  revision: 0,
  name: "Custom classroom",
  rows: 2,
  columns: 2,
  cells: [
    { row: 1, column: 1, kind: "seat", seat_id: "R1C1" },
    { row: 1, column: 2, kind: "seat", seat_id: "R1C2" },
    { row: 2, column: 1, kind: "seat", seat_id: "R2C1" },
    { row: 2, column: 2, kind: "seat", seat_id: "R2C2" },
  ],
  undo_depth: 0,
  redo_depth: 0,
  usable_seat_count: 4,
};

describe("LayoutEditorPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(createLayoutDraft).mockResolvedValue(initialState);
    vi.mocked(dispatchLayoutCommand).mockImplementation(async (_draftId, command) => ({
      ...initialState,
      revision: 1,
      undo_depth: command.action === "apply" ? 1 : 0,
      cells: initialState.cells.map((cell) =>
        cell.row === 1 && cell.column === 1
          ? { ...cell, kind: "aisle", seat_id: null }
          : cell,
      ),
      usable_seat_count: 3,
    }));
    vi.mocked(compileLayoutDraft).mockResolvedValue({
      api_version: "1",
      draft_id: "layout-1",
      revision: 1,
      layout: { layout_id: "custom-grid", seats: [] },
    });
    vi.mocked(deleteLayoutDraft).mockResolvedValue(undefined);
  });

  it("opens a grid, changes a cell kind, and saves the compiled layout", async () => {
    const user = userEvent.setup();
    const onRoomSettingsChange = vi.fn();
    render(
      <LayoutEditorPanel
        roomSettings={settings}
        t={createTranslator("en")}
        onRoomSettingsChange={onRoomSettingsChange}
      />,
    );

    await user.click(screen.getByTestId("layout-editor-open"));
    expect(await screen.findByRole("grid", { name: "Classroom layout grid" })).toBeInTheDocument();

    await user.click(screen.getByRole("gridcell", { name: /Row 1, column 1/ }));
    await user.click(screen.getByRole("button", { name: "Aisle" }));
    await waitFor(() => {
      expect(dispatchLayoutCommand).toHaveBeenCalledWith(
        "layout-1",
        expect.objectContaining({
          action: "apply",
          operation: expect.objectContaining({
            kind: "set_cell",
            payload: expect.objectContaining({ row: 1, column: 1, kind: "aisle" }),
          }),
        }),
      );
    });

    await user.click(screen.getByTestId("layout-editor-save"));
    await waitFor(() => {
      expect(onRoomSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({
          layoutJson: expect.stringContaining('"layout_id": "custom-grid"'),
          rows: 2,
          columns: 2,
        }),
      );
    });
  });
});
