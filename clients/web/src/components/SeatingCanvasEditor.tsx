import { useEffect, useRef, useState } from "react";

import type { SeatAssignment, Student } from "../api/types";
import { nextCanvasZoom } from "../domain/canvasEdit";
import type { Translate } from "../i18n/messages";
import { SeatingCanvas } from "./SeatingCanvas";
import { SeatingTable } from "./SeatingTable";
import { SeatingToolbar, type CanvasView } from "./SeatingToolbar";

type SeatingCanvasEditorProps = {
  assignments: SeatAssignment[];
  students: Student[];
  canUndo: boolean;
  canRedo: boolean;
  diagnosticBadges?: Record<string, "error" | "warning">;
  focusSeatId?: string | null;
  onDiagnosticClick?: (seatId: string) => void;
  t: Translate;
  onSeatActivate: (seatId: string) => void;
  onSwap: (fromSeatId: string, toSeatId: string) => void;
  onBatchMove: (selectedIds: string[], dropSeatId: string) => void;
  onLockSelection: (seatIds: string[]) => void;
  onUnlockSelection: (seatIds: string[]) => void;
  onAssign: (seatId: string, studentId: string | null) => void;
  onUndo: () => void;
  onRedo: () => void;
};

/**
 * The interactive seating editor (D2): canvas and table are two projections
 * of the same draft, sharing one selection, one toolbar, and the same
 * undo/redo stack (G-2).
 */
export function SeatingCanvasEditor({
  assignments,
  students,
  canUndo,
  canRedo,
  diagnosticBadges,
  focusSeatId,
  onDiagnosticClick,
  t,
  onSeatActivate,
  onSwap,
  onBatchMove,
  onLockSelection,
  onUnlockSelection,
  onAssign,
  onUndo,
  onRedo,
}: SeatingCanvasEditorProps) {
  const [view, setView] = useState<CanvasView>("canvas");
  const [zoom, setZoom] = useState(1);
  const [selectedSeatIds, setSelectedSeatIds] = useState<string[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const [focusMode, setFocusMode] = useState(false);
  const zoomTargetRef = useRef<HTMLDivElement>(null);

  // Ctrl+wheel zooms the canvas: on macOS the trackpad pinch maps to this
  // exact event, so one handler covers the trackpad gesture (design §5) and
  // Windows/Linux Ctrl+wheel alike. React attaches wheel listeners passively
  // at the root, which would make preventDefault() a no-op (the page would
  // zoom too); a native non-passive listener stops the page gesture.
  useEffect(() => {
    const target = zoomTargetRef.current;
    if (!target) {
      return undefined;
    }
    function handleWheel(event: WheelEvent) {
      if (!event.ctrlKey) {
        return;
      }
      event.preventDefault();
      setZoom((current) => nextCanvasZoom(current, event.deltaY));
    }
    target.addEventListener("wheel", handleWheel, { passive: false });
    return () => target.removeEventListener("wheel", handleWheel);
  }, []);

  useEffect(() => {
    if (!focusMode) {
      return undefined;
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setFocusMode(false);
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [focusMode]);

  function notify(message: string) {
    setStatus(message);
  }

  function handleSwap(from: string, to: string) {
    notify(t("canvas.statusSwapped", { from, to }));
    onSwap(from, to);
  }

  function handleLockSelection() {
    if (selectedSeatIds.length === 0) {
      return;
    }
    notify(t("canvas.statusLocked", { count: selectedSeatIds.length }));
    onLockSelection(selectedSeatIds);
  }

  function handleUnlockSelection() {
    if (selectedSeatIds.length === 0) {
      return;
    }
    notify(t("canvas.statusUnlocked", { count: selectedSeatIds.length }));
    onUnlockSelection(selectedSeatIds);
  }

  const lockedSelected = assignments.filter(
    (seat) => selectedSeatIds.includes(seat.seatId) && seat.locked,
  ).length;

  return (
    <>
      {focusMode ? (
        <button
          type="button"
          className="canvas-focus-scrim"
          aria-label={t("canvas.exitFocus")}
          onClick={() => setFocusMode(false)}
        />
      ) : null}
      <div className={`seating-editor${focusMode ? " is-focus-mode" : ""}`}>
      <SeatingToolbar
        view={view}
        zoom={zoom}
        focusMode={focusMode}
        selectedCount={selectedSeatIds.length}
        canUndo={canUndo}
        canRedo={canRedo}
        t={t}
        onViewChange={setView}
        onZoomChange={setZoom}
        onFocusModeChange={setFocusMode}
        onLockSelection={handleLockSelection}
        onUnlockSelection={handleUnlockSelection}
        onUndo={onUndo}
        onRedo={onRedo}
      />
      <div className="canvas-zoom-target" ref={zoomTargetRef}>
        {view === "canvas" ? (
          <SeatingCanvas
            assignments={assignments}
            selectedSeatIds={selectedSeatIds}
            zoom={zoom}
            t={t}
            onSeatActivate={onSeatActivate}
            onSelectChange={setSelectedSeatIds}
            onSwap={handleSwap}
            onBatchMove={onBatchMove}
            diagnosticBadges={diagnosticBadges}
            focusSeatId={focusSeatId}
            onDiagnosticClick={onDiagnosticClick}
            onStatus={notify}
          />
        ) : (
          <SeatingTable
            assignments={assignments}
            students={students}
            t={t}
            onAssign={onAssign}
          />
        )}
        <div className="canvas-status" aria-live="polite">
          <span className="canvas-status-text">
            {status ??
              (selectedSeatIds.length > 0
                ? t("canvas.statusSelected", {
                    count: selectedSeatIds.length,
                    locked: lockedSelected,
                  })
                : t("canvas.statusReady"))}
          </span>
          <span className="canvas-status-meta">{t("canvas.syncHint")}</span>
        </div>
      </div>
      </div>
    </>
  );
}
