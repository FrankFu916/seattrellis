import { useMemo, useState } from "react";

import {
  compileLayoutDraft,
  createLayoutDraft,
  deleteLayoutDraft,
  dispatchLayoutCommand,
  RosterApiError,
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

function errorMessage(error: unknown): string {
  if (error instanceof RosterApiError || error instanceof Error) {
    return error.message;
  }
  return String(error);
}

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
  const [selected, setSelected] = useState<LayoutCellState | null>(null);
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

  async function openEditor(): Promise<void> {
    setBusy("opening");
    setError(null);
    setStatus(null);
    try {
      let sourceLayout: Record<string, unknown> | undefined;
      try {
        sourceLayout = parseLayoutJson(roomSettings.layoutJson);
      } catch (caught) {
        throw new Error(t("layoutEditor.invalidJson", { message: errorMessage(caught) }));
      }
      if (!sourceLayout) {
        try {
          sourceLayout = buildGridLayout(roomSettings);
        } catch (caught) {
          if (caught instanceof InvalidAdvancedSettingError) {
            throw new Error(t("room.invalid"));
          }
          throw caught;
        }
      }
      const state = await createLayoutDraft({
        name: "Custom classroom",
        layout: sourceLayout,
      });
      setLayout(state);
      setSelected(null);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(null);
    }
  }

  async function runCommand(
    action: LayoutCommand["action"],
    operation?: LayoutOperation,
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
      setSelected((current) =>
        current
          ? next.cells.find(
              (cell) => cell.row === current.row && cell.column === current.column,
            ) ?? null
          : null,
      );
    } catch (caught) {
      setError(errorMessage(caught));
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
      setError(errorMessage(caught));
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
    setSelected(null);
    setError(null);
    setStatus(null);
  }

  function setCellKind(kind: LayoutCellKind): void {
    if (!selected) {
      return;
    }
    const payload: Record<string, string | number | null> = {
      row: selected.row,
      column: selected.column,
      kind,
    };
    if (kind === "seat" && selected.seat_id) {
      payload.seat_id = selected.seat_id;
    }
    void runCommand("apply", { kind: "set_cell", payload });
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
                aria-pressed={selected?.kind === kind}
                onClick={() => setCellKind(kind)}
                disabled={!selected || busy !== null}
              >
                {t(`layoutEditor.kind.${label}`)}
              </button>
            ))}
          </div>

          <div
            className="layout-editor-grid"
            style={{ gridTemplateColumns: `repeat(${layout.columns}, minmax(24px, 1fr))` }}
            role="grid"
            aria-label={t("layoutEditor.grid")}
          >
            {cells.map((cell) => (
              <button
                key={cellKey(cell.row, cell.column)}
                className={`layout-cell kind-${cell.kind}${
                  selected && selected.row === cell.row && selected.column === cell.column
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
                onClick={() => setSelected(cell)}
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
              onClick={() => void runCommand("apply", {
                kind: "translate",
                payload: { row_delta: 0, column_delta: -1 },
              })}
              disabled={busy !== null}
            >
              ←
            </button>
            <button
              className="text-button layout-move-button"
              type="button"
              title={t("layoutEditor.moveRight")}
              aria-label={t("layoutEditor.moveRight")}
              onClick={() => void runCommand("apply", {
                kind: "translate",
                payload: { row_delta: 0, column_delta: 1 },
              })}
              disabled={busy !== null}
            >
              →
            </button>
            <button
              className="text-button layout-move-button"
              type="button"
              title={t("layoutEditor.moveUp")}
              aria-label={t("layoutEditor.moveUp")}
              onClick={() => void runCommand("apply", {
                kind: "translate",
                payload: { row_delta: -1, column_delta: 0 },
              })}
              disabled={busy !== null}
            >
              ↑
            </button>
            <button
              className="text-button layout-move-button"
              type="button"
              title={t("layoutEditor.moveDown")}
              aria-label={t("layoutEditor.moveDown")}
              onClick={() => void runCommand("apply", {
                kind: "translate",
                payload: { row_delta: 1, column_delta: 0 },
              })}
              disabled={busy !== null}
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
