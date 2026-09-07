import { useMemo, useState, type MouseEvent } from "react";

import {
  compileLayoutDraft,
  createLayoutDraft,
  deleteLayoutDraft,
  dispatchLayoutCommand,
} from "../api/client";
import type {
  CustomRoomSettings,
  LayoutCellKind,
  LayoutCellState,
  LayoutCommand,
  LayoutOperation,
  LayoutStateResponse,
} from "../api/types";
import { buildGridLayout, InvalidAdvancedSettingError } from "../domain/generation";
import { describeApiError } from "../domain/errorMessages";
import type { Translate } from "../i18n/messages";

type LayoutEditorPanelProps = {
  roomSettings: CustomRoomSettings;
  t: Translate;
  onRoomSettingsChange: (changes: Partial<CustomRoomSettings>) => void;
};

const CELL_KINDS: Array<{ kind: LayoutCellKind; label: "seat" | "aisle" | "platform" | "empty" }> = [
  { kind: "seat", label: "seat" },
  { kind: "aisle", label: "aisle" },
  { kind: "platform", label: "platform" },
  { kind: "empty", label: "empty" },
];

function commandId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Carries an already-localized sentence through a catch block untouched —
 * used where the failure cause is known (bad JSON, invalid room settings)
 * and the raw error text must not leak into the panel.
 */
class LocalizedError extends Error {}

function parseLayoutJson(source: string): Record<string, unknown> | undefined {
  const text = source.trim();
  if (!text) {
    return undefined;
  }
  const parsed: unknown = JSON.parse(text);
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("Layout JSON must be an object.");
  }
  return parsed as Record<string, unknown>;
}

function cellKey(row: number, column: number): string {
  return `${row}:${column}`;
}

export function LayoutEditorPanel({
  roomSettings,
  t,
  onRoomSettingsChange,
}: LayoutEditorPanelProps) {
  const [layout, setLayout] = useState<LayoutStateResponse | null>(null);
  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(() => new Set());
  const [selectionAnchor, setSelectionAnchor] = useState<LayoutCellState | null>(null);
  const [busy, setBusy] = useState<"opening" | "saving" | "command" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const cells = useMemo(() => {
    if (!layout) {
      return [];
    }
    const byPosition = new Map(
      layout.cells.map((cell) => [cellKey(cell.row, cell.column), cell]),
    );
    return Array.from({ length: layout.rows * layout.columns }, (_, index) => {
      const row = Math.floor(index / layout.columns) + 1;
      const column = (index % layout.columns) + 1;
      return (
        byPosition.get(cellKey(row, column)) ?? {
          row,
          column,
          kind: "empty" as const,
          seat_id: null,
        }
      );
    });
  }, [layout]);

  const selectedCells = useMemo(
    () => cells.filter((cell) => selectedKeys.has(cellKey(cell.row, cell.column))),
    [cells, selectedKeys],
  );

  async function openEditor(): Promise<void> {
    setBusy("opening");
    setError(null);
    setStatus(null);
    try {
      let sourceLayout: Record<string, unknown> | undefined;
      try {
        sourceLayout = parseLayoutJson(roomSettings.layoutJson);
      } catch {
        throw new LocalizedError(t("layoutEditor.fileInvalid"));
      }
      if (!sourceLayout) {
        try {
          sourceLayout = buildGridLayout(roomSettings);
        } catch (caught) {
          if (caught instanceof InvalidAdvancedSettingError) {
            throw new LocalizedError(t("room.invalid"));
          }
          throw caught;
        }
      }
      const state = await createLayoutDraft({
        name: "Custom classroom",
        layout: sourceLayout,
      });
      setLayout(state);
      setSelectedKeys(new Set());
      setSelectionAnchor(null);
    } catch (caught) {
      setError(
        caught instanceof LocalizedError
          ? caught.message
          : describeApiError(caught, t, "layoutEditor.actionFailed"),
      );
    } finally {
      setBusy(null);
    }
  }

  async function runCommand(
    action: LayoutCommand["action"],
    operation?: LayoutOperation,
    selectionAfter?: (current: Set<string>, next: LayoutStateResponse) => Set<string>,
  ): Promise<void> {
    if (!layout) {
      return;
    }
    setBusy("command");
    setError(null);
    setStatus(null);
    try {
      const command: LayoutCommand = {
        command_id: commandId(),
        draft_id: layout.draft_id,
        base_revision: layout.revision,
        action,
        ...(operation ? { operation } : {}),
      };
      const next = await dispatchLayoutCommand(layout.draft_id, command);
      setLayout(next);
      const available = new Set(next.cells.map((cell) => cellKey(cell.row, cell.column)));
      setSelectedKeys((current) => {
        const requested = selectionAfter ? selectionAfter(current, next) : current;
        return new Set([...requested].filter((key) => available.has(key)));
      });
    } catch (caught) {
      setError(describeApiError(caught, t, "layoutEditor.actionFailed"));
    } finally {
      setBusy(null);
    }
  }

  async function saveLayout(): Promise<void> {
    if (!layout) {
      return;
    }
    setBusy("saving");
    setError(null);
    setStatus(null);
    try {
      const compiled = await compileLayoutDraft(layout.draft_id);
      onRoomSettingsChange({
        layoutJson: JSON.stringify(compiled.layout, null, 2),
        rows: layout.rows,
        columns: layout.columns,
        aisleColumns: "",
        disabledSeats: "",
      });
      setStatus(t("layoutEditor.saved"));
    } catch (caught) {
      setError(describeApiError(caught, t, "layoutEditor.actionFailed"));
    } finally {
      setBusy(null);
    }
  }

  async function closeEditor(): Promise<void> {
    if (layout) {
      try {
        await deleteLayoutDraft(layout.draft_id);
      } catch {
        // The local draft may already have expired; closing the panel is safe.
      }
    }
    setLayout(null);
    setSelectedKeys(new Set());
    setSelectionAnchor(null);
    setError(null);
    setStatus(null);
  }

  function setCellKind(kind: LayoutCellKind): void {
    if (selectedCells.length === 0) {
      return;
    }
    const updates = selectedCells.map((cell) => ({
      row: cell.row,
      column: cell.column,
      kind,
      ...(kind === "seat" && cell.seat_id ? { seat_id: cell.seat_id } : {}),
    }));
    void runCommand("apply", {
      kind: updates.length === 1 ? "set_cell" : "set_cells",
      payload: updates.length === 1 ? updates[0] : { cells: updates },
    });
  }

  function selectCell(cell: LayoutCellState, event: MouseEvent<HTMLButtonElement>): void {
    const key = cellKey(cell.row, cell.column);
    if (event.shiftKey && selectionAnchor) {
      const minRow = Math.min(selectionAnchor.row, cell.row);
      const maxRow = Math.max(selectionAnchor.row, cell.row);
      const minColumn = Math.min(selectionAnchor.column, cell.column);
      const maxColumn = Math.max(selectionAnchor.column, cell.column);
      setSelectedKeys(
        new Set(
          cells
            .filter(
              (candidate) =>
                candidate.row >= minRow &&
                candidate.row <= maxRow &&
                candidate.column >= minColumn &&
                candidate.column <= maxColumn,
            )
            .map((candidate) => cellKey(candidate.row, candidate.column)),
        ),
      );
      return;
    }
    setSelectionAnchor(cell);
    if (event.metaKey || event.ctrlKey) {
      setSelectedKeys((current) => {
        const next = new Set(current);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
        }
        return next;
      });
      return;
    }
    setSelectedKeys(new Set([key]));
  }

  function moveSelection(rowDelta: number, columnDelta: number): void {
    if (selectedCells.length === 0) {
      void runCommand("apply", {
        kind: "translate",
        payload: { row_delta: rowDelta, column_delta: columnDelta },
      });
      return;
    }
    const positions = selectedCells.map(({ row, column }) => ({ row, column }));
    void runCommand(
      "apply",
      {
        kind: "translate_cells",
        payload: {
          cells: positions,
          row_delta: rowDelta,
          column_delta: columnDelta,
        },
      },
      () =>
        new Set(
          positions.map(({ row, column }) =>
            cellKey(row + rowDelta, column + columnDelta),
          ),
        ),
    );
  }

  function canMove(rowDelta: number, columnDelta: number): boolean {
    if (!layout) {
      return false;
    }
    const chosen = selectedCells.length > 0
      ? selectedCells.filter((cell) => cell.kind !== "empty")
      : cells.filter((cell) => cell.kind !== "empty");
    if (chosen.length === 0) {
      return false;
    }
    const chosenKeys = new Set(chosen.map((cell) => cellKey(cell.row, cell.column)));
    const occupiedKeys = new Set(
      cells
        .filter((cell) => cell.kind !== "empty")
        .map((cell) => cellKey(cell.row, cell.column)),
    );
    return chosen.every((cell) => {
      const row = cell.row + rowDelta;
      const column = cell.column + columnDelta;
      const target = cellKey(row, column);
      return (
        row >= 1 &&
        row <= layout.rows &&
        column >= 1 &&
        column <= layout.columns &&
        (!occupiedKeys.has(target) || chosenKeys.has(target))
      );
    });
  }

  return (
    <section className="layout-editor-card" aria-labelledby="layout-editor-title">
      <div className="layout-editor-heading">
        <div>
          <h3 id="layout-editor-title">{t("layoutEditor.title")}</h3>
          <p>{t("layoutEditor.hint")}</p>
        </div>
        {layout ? (
          <button
            className="text-button"
            type="button"
            onClick={() => void closeEditor()}
            disabled={busy !== null}
          >
            {t("layoutEditor.close")}
          </button>
        ) : (
          <button
            className="secondary-button"
            type="button"
            onClick={() => void openEditor()}
            disabled={busy !== null}
            data-testid="layout-editor-open"
          >
            {busy === "opening" ? t("layoutEditor.opening") : t("layoutEditor.open")}
          </button>
        )}
      </div>

      {layout ? (
        <>
          <div className="layout-editor-toolbar" aria-label={t("layoutEditor.toolbar")}>
            {CELL_KINDS.map(({ kind, label }) => (
              <button
                key={kind}
                className={`layout-kind-button kind-${kind}`}
                type="button"
                aria-pressed={
                  selectedCells.length > 0 && selectedCells.every((cell) => cell.kind === kind)
                }
                onClick={() => setCellKind(kind)}
                disabled={selectedCells.length === 0 || busy !== null}
              >
                {t(`layoutEditor.kind.${label}`)}
              </button>
            ))}
            <span className="layout-selection-count" role="status">
              {t("layoutEditor.selectionCount", { count: selectedCells.length })}
            </span>
            <button
              className="text-button"
              type="button"
              onClick={() => setSelectedKeys(new Set())}
              disabled={selectedCells.length === 0 || busy !== null}
            >
              {t("layoutEditor.clearSelection")}
            </button>
          </div>

          <div
            className="layout-editor-grid"
            style={{ gridTemplateColumns: `repeat(${layout.columns}, minmax(24px, 1fr))` }}
            role="grid"
            aria-multiselectable="true"
            aria-label={t("layoutEditor.grid")}
          >
            {cells.map((cell) => (
              <button
                key={cellKey(cell.row, cell.column)}
                className={`layout-cell kind-${cell.kind}${
                  selectedKeys.has(cellKey(cell.row, cell.column))
                    ? " is-selected"
                    : ""
                }`}
                type="button"
                role="gridcell"
                aria-label={t("layoutEditor.cell", {
                  row: cell.row,
                  column: cell.column,
                  kind: t(`layoutEditor.kind.${cell.kind}`),
                })}
                aria-selected={selectedKeys.has(cellKey(cell.row, cell.column))}
                onClick={(event) => selectCell(cell, event)}
                disabled={busy !== null}
              >
                {cell.kind === "seat" ? cell.seat_id : cell.kind === "platform" ? "▰" : ""}
              </button>
            ))}
          </div>

          <div className="layout-editor-actions">
            <button
              className="text-button"
              type="button"
              onClick={() => void runCommand("apply", {
                kind: "insert_row",
                payload: { index: layout.rows + 1 },
              })}
              disabled={busy !== null}
            >
              {t("layoutEditor.addRow")}
            </button>
            <button
              className="text-button"
              type="button"
              onClick={() => void runCommand("apply", {
                kind: "insert_column",
                payload: { index: layout.columns + 1 },
              })}
              disabled={busy !== null}
            >
              {t("layoutEditor.addColumn")}
            </button>
            <button
              className="text-button"
              type="button"
              onClick={() => void runCommand("apply", {
                kind: "delete_row",
                payload: { index: layout.rows },
              })}
              disabled={busy !== null || layout.rows <= 1}
            >
              {t("layoutEditor.removeRow")}
            </button>
            <button
              className="text-button"
              type="button"
              onClick={() => void runCommand("apply", {
                kind: "delete_column",
                payload: { index: layout.columns },
              })}
              disabled={busy !== null || layout.columns <= 1}
            >
              {t("layoutEditor.removeColumn")}
            </button>
            <button
              className="text-button layout-move-button"
              type="button"
              title={t("layoutEditor.moveLeft")}
              aria-label={t("layoutEditor.moveLeft")}
              onClick={() => moveSelection(0, -1)}
              disabled={busy !== null || !canMove(0, -1)}
            >
              ←
            </button>
            <button
              className="text-button layout-move-button"
              type="button"
              title={t("layoutEditor.moveRight")}
              aria-label={t("layoutEditor.moveRight")}
              onClick={() => moveSelection(0, 1)}
              disabled={busy !== null || !canMove(0, 1)}
            >
              →
            </button>
            <button
              className="text-button layout-move-button"
              type="button"
              title={t("layoutEditor.moveUp")}
              aria-label={t("layoutEditor.moveUp")}
              onClick={() => moveSelection(-1, 0)}
              disabled={busy !== null || !canMove(-1, 0)}
            >
              ↑
            </button>
            <button
              className="text-button layout-move-button"
              type="button"
              title={t("layoutEditor.moveDown")}
              aria-label={t("layoutEditor.moveDown")}
              onClick={() => moveSelection(1, 0)}
              disabled={busy !== null || !canMove(1, 0)}
            >
              ↓
            </button>
            <button
              className="text-button"
              type="button"
              onClick={() => void runCommand("apply", {
                kind: "mirror_horizontal",
                payload: {},
              })}
              disabled={busy !== null}
            >
              {t("layoutEditor.mirror")}
            </button>
            <button
              className="text-button"
              type="button"
              onClick={() => void runCommand("apply", {
                kind: "flip_vertical",
                payload: {},
              })}
              disabled={busy !== null}
            >
              {t("layoutEditor.flip")}
            </button>
            <button
              className="text-button"
              type="button"
              onClick={() => void runCommand("undo")}
              disabled={busy !== null || layout.undo_depth === 0}
            >
              {t("layoutEditor.undo")}
            </button>
            <button
              className="text-button"
              type="button"
              onClick={() => void runCommand("redo")}
              disabled={busy !== null || layout.redo_depth === 0}
            >
              {t("layoutEditor.redo")}
            </button>
          </div>

          <div className="layout-editor-footer">
            <small>
              {t("layoutEditor.seatCount", { count: layout.usable_seat_count })}
            </small>
            <button
              className="primary-button"
              type="button"
              onClick={() => void saveLayout()}
              disabled={busy !== null || layout.usable_seat_count === 0}
              data-testid="layout-editor-save"
            >
              {busy === "saving" ? t("layoutEditor.saving") : t("layoutEditor.save")}
            </button>
          </div>
        </>
      ) : (
        <p className="layout-editor-empty">{t("layoutEditor.empty")}</p>
      )}

      {status ? <p className="layout-editor-status" role="status">{status}</p> : null}
      {error ? <p className="layout-editor-error" role="alert">{error}</p> : null}
    </section>
  );
}
