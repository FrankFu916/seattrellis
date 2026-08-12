import { useRef, useState } from "react";

import type { SeatAssignment } from "../api/types";
import type { Translate } from "../i18n/messages";
import {
  CANVAS_GEOMETRY,
  seatAtPoint,
  seatPosition,
  seatsInBox,
  type CanvasGeometry,
} from "../domain/canvasEdit";
import { platformModifierLabel } from "../domain/desktop";

type DragState =
  | {
      mode: "single";
      seatId: string;
      offsetX: number;
      offsetY: number;
      lastX: number;
      lastY: number;
      moved: boolean;
      /** Pointer capture is armed on the first real move, not on press:
          capturing on pointerdown redirects the synthesized click to the
          stage container and seat clicks would never fire. */
      captured: boolean;
    }
  | {
      mode: "batch";
      selectedIds: string[];
      offsetX: number;
      offsetY: number;
      lastX: number;
      lastY: number;
      moved: boolean;
      captured: boolean;
    };

type SeatingCanvasProps = {
  assignments: SeatAssignment[];
  /** Multi-selection owned by the editor (box-select / batch ops). */
  selectedSeatIds?: string[];
  /** 0.6–1.8; 1 = fit. Applied as a CSS scale on the seat grid. */
  zoom?: number;
  interactive?: boolean;
  t: Translate;
  /** Click / keyboard activation of a single seat (select or swap target). */
  onSeatActivate?: (seatId: string) => void;
  /** Rubber-band selection result (never includes locked seats). */
  onSelectChange?: (seatIds: string[]) => void;
  /** Drag-and-drop swap between two unlocked seats. */
  onSwap?: (fromSeatId: string, toSeatId: string) => void;
  /** Dragging a multi-selection onto a seat. */
  onBatchMove?: (selectedIds: string[], dropSeatId: string) => void;
  /** Inline diagnostics badges per seat (D6): "error" / "warning". */
  diagnosticBadges?: Record<string, "error" | "warning">;
  /** Seat the diagnostics list is currently linked to (D6). */
  focusSeatId?: string | null;
  onDiagnosticClick?: (seatId: string) => void;
  onStatus?: (message: string) => void;
};

function toViewBoxPoint(
  clientX: number,
  clientY: number,
  stage: HTMLDivElement | null,
  svg: SVGSVGElement | null,
  viewBoxWidth: number,
  zoom: number,
): { x: number; y: number } {
  const stageRect = stage?.getBoundingClientRect();
  // The SVG renders at container width (CSS scales the viewBox); the scale
  // already includes the zoom transform on the holder. jsdom reports zero
  // rects, so fall back to the bare zoom for tests.
  const svgRect = svg?.getBoundingClientRect();
  const scale =
    svgRect && svgRect.width > 0 ? svgRect.width / viewBoxWidth : zoom;
  return {
    x: (clientX - (stageRect?.left ?? 0)) / scale,
    y: (clientY - (stageRect?.top ?? 0)) / scale,
  };
}

export function SeatingCanvas({
  assignments,
  selectedSeatIds = [],
  zoom = 1,
  interactive = true,
  t,
  onSeatActivate,
  onSelectChange,
  onSwap,
  onBatchMove,
  diagnosticBadges,
  focusSeatId = null,
  onDiagnosticClick,
  onStatus,
}: SeatingCanvasProps) {
  const geometry: CanvasGeometry = CANVAS_GEOMETRY;
  const stageRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const dragRef = useRef<DragState | null>(null);
  const suppressClickRef = useRef(false);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [hoverTarget, setHoverTarget] = useState<string | null>(null);
  const [rubberBand, setRubberBand] = useState<{
    start: { x: number; y: number };
    current: { x: number; y: number };
  } | null>(null);

  const columns =
    Math.max(0, ...assignments.map((assignment) => assignment.column)) + 1;
  const rows =
    Math.max(0, ...assignments.map((assignment) => assignment.row)) + 1;
  const seatWidth = geometry.seatWidth;
  const seatHeight = geometry.seatHeight;
  const columnGap = geometry.columnGap;
  const rowGap = geometry.rowGap;
  const margin = geometry.margin;
  const frontHeight = geometry.frontHeight;
  const width = columns * (seatWidth + columnGap) - columnGap + margin * 2;
  const height =
    rows * (seatHeight + rowGap) - rowGap + margin * 2 + frontHeight;
  const lockedSeatIds = new Set(
    assignments.filter((seat) => seat.locked).map((seat) => seat.seatId),
  );
  const selected = new Set(selectedSeatIds);

  function activate(seatId: string) {
    if (interactive && !lockedSeatIds.has(seatId)) {
      onSeatActivate?.(seatId);
    }
  }

  /** Single-click: toggle the seat in the selection and activate it. */
  function handleSeatClick(seatId: string) {
    if (!interactive) {
      return;
    }
    if (suppressClickRef.current) {
      // The pointer just finished a drag; the click is a drop artifact.
      suppressClickRef.current = false;
      return;
    }
    const next = selected.has(seatId)
      ? selectedSeatIds.filter((id) => id !== seatId)
      : [...selectedSeatIds, seatId];
    onSelectChange?.(next);
    activate(seatId);
  }

  function dragPointerPoint(event: React.PointerEvent) {
    return toViewBoxPoint(
      event.clientX,
      event.clientY,
      stageRef.current,
      svgRef.current,
      width,
      zoom,
    );
  }

  /** Screen scale (viewBox -> px), for overlay positioning. */
  function effectiveScale(): number {
    const svgRect = svgRef.current?.getBoundingClientRect();
    return svgRect && svgRect.width > 0 ? svgRect.width / width : zoom;
  }

  /** The dragged seat's center, used for hover/drop targeting. */
  function dragCenter(dragState: DragState): { x: number; y: number } {
    return {
      x: dragState.lastX - dragState.offsetX + seatWidth / 2,
      y: dragState.lastY - dragState.offsetY + seatHeight / 2,
    };
  }

  function startDrag(
    event: React.PointerEvent,
    seatId: string,
    fromSeat: SeatAssignment,
  ) {
    const point = dragPointerPoint(event);
    const position = seatPosition(fromSeat, geometry);
    if (selected.has(seatId) && selectedSeatIds.length > 1) {
      const next: DragState = {
        mode: "batch",
        selectedIds: selectedSeatIds,
        offsetX: point.x - position.x,
        offsetY: point.y - position.y,
        lastX: point.x,
        lastY: point.y,
        moved: false,
        captured: false,
      };
      dragRef.current = next;
      setDrag(next);
      onStatus?.(t("canvas.batchDragHint", { count: selectedSeatIds.length }));
      return;
    }
    const next: DragState = {
      mode: "single",
      seatId,
      offsetX: point.x - position.x,
      offsetY: point.y - position.y,
      lastX: point.x,
      lastY: point.y,
      moved: false,
      captured: false,
    };
    dragRef.current = next;
    setDrag(next);
    onStatus?.(t("canvas.dragHint"));
  }

  function moveDrag(event: React.PointerEvent) {
    const current = dragRef.current;
    if (!current) {
      return;
    }
    if (!current.captured) {
      // Arm pointer capture on the first real move so a drag keeps
      // receiving move/up events after leaving the stage; a plain press
      // (no move) never captures and the seat click still fires.
      try {
        event.currentTarget.setPointerCapture?.(event.pointerId);
      } catch {
        // jsdom and older engines have no pointer capture; events keep
        // flowing while the pointer stays over the stage.
      }
      current.captured = true;
    }
    const point = dragPointerPoint(event);
    const deltaX = point.x - current.lastX;
    const deltaY = point.y - current.lastY;
    const next: DragState = {
      ...current,
      lastX: point.x,
      lastY: point.y,
      moved: true,
    };
    if (Math.abs(deltaX) + Math.abs(deltaY) > 1) {
      suppressClickRef.current = true;
    }
    dragRef.current = next;
    setDrag(next);
    const center = dragCenter(next);
    const under = seatAtPoint(assignments, center, geometry);
    if (
      under &&
      (next.mode === "single" ? under !== next.seatId : !selected.has(under))
    ) {
      setHoverTarget(under);
    } else {
      setHoverTarget(null);
    }
  }

  function endDrag() {
    const current = dragRef.current;
    if (!current) {
      return;
    }
    const center = dragCenter(current);
    const under = seatAtPoint(assignments, center, geometry);
    if (current.moved && under && !lockedSeatIds.has(under)) {
      if (current.mode === "single") {
        if (under !== current.seatId) {
          onSwap?.(current.seatId, under);
        }
      } else if (!selected.has(under)) {
        onBatchMove?.(current.selectedIds, under);
      }
    }
    dragRef.current = null;
    setDrag(null);
    setHoverTarget(null);
  }

  function handlePointerDown(event: React.PointerEvent) {
    if (!interactive) {
      return;
    }
    // No pointer capture on press: capturing redirects the browser's
    // synthesized click to this container, which would swallow seat clicks
    // entirely. Capture is armed on the first real drag move instead.
    suppressClickRef.current = false;
    const target = event.target as Element;
    if (target.closest?.("[data-diagnostic]")) {
      return;
    }
    const seatEl = target.closest?.("[data-seat-id]");
    if (seatEl) {
      const seatId = seatEl.getAttribute("data-seat-id") as string;
      if (lockedSeatIds.has(seatId)) {
        onStatus?.(t("canvas.lockedSeat"));
        return;
      }
      const seat = assignments.find((item) => item.seatId === seatId);
      if (seat) {
        startDrag(event, seatId, seat);
      }
      return;
    }
    setRubberBand({
      start: dragPointerPoint(event),
      current: dragPointerPoint(event),
    });
  }

  function handlePointerMove(event: React.PointerEvent) {
    if (dragRef.current) {
      moveDrag(event);
    } else if (rubberBand) {
      setRubberBand((band) =>
        band
          ? {
              ...band,
              current: dragPointerPoint(event),
            }
          : band,
      );
    }
  }

  function handlePointerUp() {
    if (dragRef.current) {
      endDrag();
    } else if (rubberBand) {
      setRubberBand((band) => {
        if (!band) {
          return band;
        }
        const ids = seatsInBox(
          assignments,
          {
            x1: band.start.x,
            y1: band.start.y,
            x2: band.current.x,
            y2: band.current.y,
          },
          lockedSeatIds,
          geometry,
        );
        if (ids.length > 0) {
          onSelectChange?.(ids);
          onStatus?.(t("canvas.selectedCount", { count: ids.length }));
        }
        return null;
      });
    }
  }

  function handleKeyDown(event: React.KeyboardEvent) {
    if (!interactive) {
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      return; // handled on the seat itself
    }
    const moves: Record<string, [number, number]> = {
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
    };
    const delta = moves[event.key];
    if (!delta) {
      return;
    }
    const primary = selectedSeatIds.at(-1);
    if (!primary) {
      return;
    }
    event.preventDefault();
    const seat = assignments.find((item) => item.seatId === primary);
    if (!seat) {
      return;
    }
    const [dc, dr] = delta;
    const neighbor = assignments.find(
      (item) =>
        item.row === seat.row + dr && item.column === seat.column + dc,
    );
    if (neighbor && !lockedSeatIds.has(neighbor.seatId)) {
      onSelectChange?.([neighbor.seatId]);
      onStatus?.(t("canvas.seatStatus", { seat: neighbor.seatId }));
    }
  }

  const dragSeat = drag?.mode === "single" ? drag.seatId : null;

  return (
    <div className="canvas-frame">
      <div
        className="canvas-stage"
        data-testid="canvas-stage"
        ref={stageRef}
        role="presentation"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
      >
        <div
          className="seating-holder"
          style={{ transform: `scale(${zoom})`, transformOrigin: "top left" }}
        >
          <svg
            className="seating-canvas"
            ref={svgRef}
            viewBox={`0 0 ${width} ${height}`}
            role={interactive ? undefined : "img"}
            aria-labelledby={
              interactive
                ? undefined
                : "seating-canvas-title seating-canvas-description"
            }
            onKeyDown={handleKeyDown}
          >
            {!interactive ? (
              <>
                <title id="seating-canvas-title">{t("canvas.title")}</title>
                <desc id="seating-canvas-description">
                  {t("canvas.help", { mod: platformModifierLabel() })}
                </desc>
              </>
            ) : null}
            <g className="teacher-desk" aria-hidden="true">
              <rect
                x={width / 2 - 98}
                y={18}
                width="196"
                height="28"
                rx="8"
              />
              <text x={width / 2} y={37} textAnchor="middle">
                {t("canvas.front")}
              </text>
            </g>
            {assignments.map((seat) => {
              const position = seatPosition(seat, geometry);
              const lifted =
                dragSeat === seat.seatId && drag
                  ? {
                      x: drag.lastX - drag.offsetX,
                      y: drag.lastY - drag.offsetY,
                    }
                  : position;
              const studentName = seat.student?.name ?? t("canvas.empty");
              const ariaLabel = t("canvas.seatLabel", {
                row: seat.row + 1,
                column: seat.column + 1,
                student: studentName,
                locked: seat.locked ? t("canvas.locked") : "",
              });
              const classNames = [
                "seat",
                seat.student ? "seat-occupied" : "seat-empty",
                seat.locked ? "seat-locked" : "",
                selected.has(seat.seatId) ? "seat-selected" : "",
                dragSeat === seat.seatId ? "seat-dragging" : "",
                hoverTarget === seat.seatId ? "seat-hover-target" : "",
                focusSeatId === seat.seatId ? "seat-diagnostic-focus" : "",
              ]
                .filter(Boolean)
                .join(" ");

              return (
                <g
                  className={classNames}
                  key={seat.seatId}
                  data-seat-id={seat.seatId}
                  transform={`translate(${lifted.x} ${lifted.y})`}
                  role={interactive ? "button" : undefined}
                  tabIndex={interactive ? 0 : undefined}
                  aria-label={interactive ? ariaLabel : undefined}
                  aria-pressed={
                    interactive ? selected.has(seat.seatId) : undefined
                  }
                  onClick={(event) => {
                    if (interactive) {
                      event.stopPropagation();
                      handleSeatClick(seat.seatId);
                    }
                  }}
                  onKeyDown={(event) => {
                    if (
                      interactive &&
                      (event.key === "Enter" || event.key === " ")
                    ) {
                      event.preventDefault();
                      event.stopPropagation();
                      handleSeatClick(seat.seatId);
                    }
                  }}
                >
                  <rect width={seatWidth} height={seatHeight} rx="10" />
                  <text
                    className="seat-row-label"
                    x="10"
                    y="18"
                    aria-hidden="true"
                  >
                    {seat.seatId}
                  </text>
                  <text
                    className="seat-student-name"
                    x={seatWidth / 2}
                    y="45"
                    textAnchor="middle"
                    aria-hidden="true"
                  >
                    {studentName}
                  </text>
                  {seat.locked ? (
                    <g
                      className="seat-lock-mark"
                      transform={`translate(${seatWidth - 19} 10)`}
                      aria-hidden="true"
                    >
                      <rect x="0" y="5" width="10" height="8" rx="2" />
                      <path d="M2 5V3a3 3 0 0 1 6 0v2" />
                    </g>
                  ) : null}
                  {diagnosticBadges?.[seat.seatId] ? (
                    <g
                      className={`seat-diagnostic seat-diagnostic-${diagnosticBadges[seat.seatId]}`}
                      data-diagnostic="true"
                      transform={`translate(${seatWidth - 26} 2)`}
                      role="button"
                      aria-label={`${seat.seatId}: ${
                        diagnosticBadges[seat.seatId] === "error"
                          ? t("diagnostics.badgeViolation")
                          : t("diagnostics.badgeSuggestion")
                      }`}
                      onClick={(event) => {
                        event.stopPropagation();
                        onDiagnosticClick?.(seat.seatId);
                      }}
                    >
                      {/* Generous invisible hit area (design §6: >= 44px
                          targets where seats are content, not controls). */}
                      <circle cx="12" cy="12" r="17" fill="transparent" />
                      <circle cx="12" cy="12" r="11" />
                      <text x="12" y="15.5" textAnchor="middle">
                        {diagnosticBadges[seat.seatId] === "error" ? "!" : "i"}
                      </text>
                    </g>
                  ) : null}
                </g>
              );
            })}
          </svg>
        </div>
        {rubberBand ? (
          <div
            className="rubber-band"
            data-testid="rubber-band"
            style={{
              left: Math.min(rubberBand.start.x, rubberBand.current.x) * effectiveScale(),
              top: Math.min(rubberBand.start.y, rubberBand.current.y) * effectiveScale(),
              width:
                Math.abs(rubberBand.current.x - rubberBand.start.x) * effectiveScale(),
              height:
                Math.abs(rubberBand.current.y - rubberBand.start.y) * effectiveScale(),
            }}
          />
        ) : null}
        {drag?.mode === "batch" ? (
          <div
            className="batch-ghost"
            data-testid="batch-ghost"
            style={{
              left: (drag.lastX - drag.offsetX) * effectiveScale(),
              top: (drag.lastY - drag.offsetY) * effectiveScale(),
            }}
          >
            {t("canvas.selectedCount", { count: drag.selectedIds.length })}
          </div>
        ) : null}
      </div>
      {interactive ? (
        <p className="canvas-help" aria-hidden="true">
          {t("canvas.help", { mod: platformModifierLabel() })}
        </p>
      ) : null}
    </div>
  );
}
