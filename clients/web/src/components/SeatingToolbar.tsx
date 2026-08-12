import {
  CANVAS_ZOOM_MAX,
  CANVAS_ZOOM_MIN,
  CANVAS_ZOOM_STEP,
} from "../domain/canvasEdit";
import type { Translate } from "../i18n/messages";

export type CanvasView = "canvas" | "table";

type SeatingToolbarProps = {
  view: CanvasView;
  zoom: number;
  selectedCount: number;
  canUndo: boolean;
  canRedo: boolean;
  t: Translate;
  onViewChange: (view: CanvasView) => void;
  onZoomChange: (zoom: number) => void;
  onLockSelection: () => void;
  onUnlockSelection: () => void;
  onUndo: () => void;
  onRedo: () => void;
};

/**
 * Canvas card toolbar (D2): view switch, zoom, selection chip, batch
 * lock/unlock, and the shared undo/redo stack.
 */
export function SeatingToolbar({
  view,
  zoom,
  selectedCount,
  canUndo,
  canRedo,
  t,
  onViewChange,
  onZoomChange,
  onLockSelection,
  onUnlockSelection,
  onUndo,
  onRedo,
}: SeatingToolbarProps) {
  return (
    <div className="canvas-toolbar">
      <div className="view-switch" role="tablist" aria-label={t("canvas.viewLabel")}>
        <button
          type="button"
          role="tab"
          aria-selected={view === "canvas"}
          data-active={view === "canvas"}
          onClick={() => onViewChange("canvas")}
        >
          {t("canvas.viewCanvas")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={view === "table"}
          data-active={view === "table"}
          onClick={() => onViewChange("table")}
        >
          {t("canvas.viewTable")}
        </button>
      </div>
      <div className="zoom-controls">
        <button
          type="button"
          aria-label={t("canvas.zoomOut")}
          onClick={() => onZoomChange(Math.max(CANVAS_ZOOM_MIN, zoom - CANVAS_ZOOM_STEP))}
        >
          −
        </button>
        <span className="zoom-value">{Math.round(zoom * 100)}%</span>
        <button
          type="button"
          aria-label={t("canvas.zoomIn")}
          onClick={() => onZoomChange(Math.min(CANVAS_ZOOM_MAX, zoom + CANVAS_ZOOM_STEP))}
        >
          +
        </button>
      </div>
      {selectedCount > 0 ? (
        <span className="ctx-chip" data-testid="selection-chip">
          {t("canvas.selectedCount", { count: selectedCount })}
        </span>
      ) : null}
      <button
        type="button"
        className="secondary-button"
        disabled={selectedCount === 0}
        onClick={onLockSelection}
      >
        {t("canvas.lockSelected")}
      </button>
      <button
        type="button"
        className="secondary-button"
        disabled={selectedCount === 0}
        onClick={onUnlockSelection}
      >
        {t("canvas.unlockSelected")}
      </button>
      <span className="toolbar-spacer" aria-hidden="true" />
      <button
        type="button"
        className="secondary-button"
        disabled={!canUndo}
        onClick={onUndo}
      >
        {t("action.undo")}
      </button>
      <button
        type="button"
        className="secondary-button"
        disabled={!canRedo}
        onClick={onRedo}
      >
        {t("action.redo")}
      </button>
    </div>
  );
}
