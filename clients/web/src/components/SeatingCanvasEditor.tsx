import { useState } from "react";

import type { SeatAssignment, Student } from "../api/types";
import type { Translate } from "../i18n/messages";
import { SeatingCanvas } from "./SeatingCanvas";
import { SeatingTable } from "./SeatingTable";
import { SeatingToolbar, type CanvasView } from "./SeatingToolbar";

type SeatingCanvasEditorProps = {
  assignments: SeatAssignment[];
  students: Student[];
  canUndo: boolean;
  canRedo: boolean;
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
    <div className="seating-editor">
      <SeatingToolbar
        view={view}
        zoom={zoom}
        selectedCount={selectedSeatIds.length}
        canUndo={canUndo}
        canRedo={canRedo}
        t={t}
        onViewChange={setView}
        onZoomChange={setZoom}
        onLockSelection={handleLockSelection}
        onUnlockSelection={handleUnlockSelection}
        onUndo={onUndo}
        onRedo={onRedo}
      />
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
  );
}
