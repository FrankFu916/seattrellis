import type { SeatAssignment } from "../api/types";
import type { Translate } from "../i18n/messages";

type SeatingCanvasProps = {
  assignments: SeatAssignment[];
  selectedSeatId?: string | null;
  interactive?: boolean;
  t: Translate;
  onSeatActivate?: (seatId: string) => void;
};

export function SeatingCanvas({
  assignments,
  selectedSeatId = null,
  interactive = true,
  t,
  onSeatActivate,
}: SeatingCanvasProps) {
  const columns =
    Math.max(0, ...assignments.map((assignment) => assignment.column)) + 1;
  const rows =
    Math.max(0, ...assignments.map((assignment) => assignment.row)) + 1;
  const seatWidth = 116;
  const seatHeight = 70;
  const columnGap = 18;
  const rowGap = 18;
  const margin = 28;
  const frontHeight = 64;
  const width = columns * (seatWidth + columnGap) - columnGap + margin * 2;
  const height =
    rows * (seatHeight + rowGap) - rowGap + margin * 2 + frontHeight;

  function activate(seatId: string) {
    if (interactive) {
      onSeatActivate?.(seatId);
    }
  }

  return (
    <div className="canvas-frame">
      <svg
        className="seating-canvas"
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-labelledby="seating-canvas-title seating-canvas-description"
      >
        <title id="seating-canvas-title">{t("canvas.title")}</title>
        <desc id="seating-canvas-description">{t("canvas.help")}</desc>
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
          const x = margin + seat.column * (seatWidth + columnGap);
          const y =
            margin + frontHeight + seat.row * (seatHeight + rowGap);
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
            selectedSeatId === seat.seatId ? "seat-selected" : "",
          ]
            .filter(Boolean)
            .join(" ");

          return (
            <g
              className={classNames}
              key={seat.seatId}
              transform={`translate(${x} ${y})`}
              role={interactive ? "button" : undefined}
              tabIndex={interactive ? 0 : undefined}
              aria-label={ariaLabel}
              aria-pressed={
                interactive ? selectedSeatId === seat.seatId : undefined
              }
              onClick={() => activate(seat.seatId)}
              onKeyDown={(event) => {
                if (
                  interactive &&
                  (event.key === "Enter" || event.key === " ")
                ) {
                  event.preventDefault();
                  activate(seat.seatId);
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
            </g>
          );
        })}
      </svg>
      {interactive ? <p className="canvas-help">{t("canvas.help")}</p> : null}
    </div>
  );
}

