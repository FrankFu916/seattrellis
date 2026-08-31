import {
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";

import type { RotationPlan, SeatAssignment } from "../api/types";
import {
  analyzeRotationMovement,
  transitionSeats,
  type RotationHeatLevel,
  type RotationTransition,
} from "../domain/rotationMovement";
import type { Locale, MessageKey, Translate } from "../i18n/messages";

type RotationMovementHeatmapProps = {
  plan: RotationPlan;
  layoutSeats: SeatAssignment[];
  activePeriod: number;
  locale: Locale;
  t: Translate;
};

type ViewMode = "overview" | "transition";

const levels: RotationHeatLevel[] = [
  "stable",
  "low",
  "medium",
  "high",
  "very-high",
  "unknown",
  "empty",
];

function periodLabel(
  period: RotationTransition["from"],
  t: Translate,
): string {
  return period.label || t("history.period", { number: period.period });
}

function transitionLabel(
  transition: RotationTransition,
  t: Translate,
): string {
  return `${periodLabel(transition.from, t)} → ${periodLabel(transition.to, t)}`;
}

function numberText(value: number | null, locale: Locale): string {
  if (value === null) {
    return "—";
  }
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value);
}

function movementDistanceText(
  value: number | null,
  locale: Locale,
  t: Translate,
): string {
  if (value === null) {
    return t("history.movement.distanceUnknown");
  }
  if (value === 1) {
    return t("history.movement.distanceValueOne");
  }
  return t("history.movement.distanceValue", {
    distance: numberText(value, locale),
  });
}

function legendKey(
  mode: ViewMode,
  level: RotationHeatLevel,
): MessageKey {
  if (level === "unknown") {
    return "history.movement.legend.unknown";
  }
  if (level === "empty") {
    return "history.movement.legend.empty";
  }
  return `history.movement.legend.${mode}.${level}` as MessageKey;
}

export function RotationMovementHeatmap({
  plan,
  layoutSeats,
  activePeriod,
  locale,
  t,
}: RotationMovementHeatmapProps) {
  const analysis = useMemo(
    () => analyzeRotationMovement(plan, layoutSeats),
    [plan, layoutSeats],
  );
  const [mode, setMode] = useState<ViewMode>("overview");
  const [requestedTargetPeriod, setRequestedTargetPeriod] = useState<number | null>(
    null,
  );
  const overviewTabRef = useRef<HTMLButtonElement>(null);
  const transitionTabRef = useRef<HTMLButtonElement>(null);
  const requestedTransition = analysis.transitions.find(
    (transition) => transition.to.period === requestedTargetPeriod,
  );
  const activeTransition = analysis.transitions.find(
    (transition) => transition.to.period === activePeriod,
  );
  const selectedTransition =
    requestedTransition ?? activeTransition ?? analysis.transitions[0] ?? null;
  const transitionSeatData = selectedTransition
    ? transitionSeats(selectedTransition, analysis.seats)
    : [];
  const columns = Math.max(0, ...analysis.seats.map((seat) => seat.column)) + 1;
  const movedStudents =
    selectedTransition?.movements.filter(
      (movement) => movement.status === "moved",
    ) ?? [];

  if (analysis.transitions.length === 0) {
    return (
      <section className="movement-card" data-testid="rotation-movement-heatmap">
        <div className="movement-heading">
          <div>
            <span className="eyebrow">{t("history.movement.eyebrow")}</span>
            <h2>{t("history.movement.title")}</h2>
          </div>
          <span className="chip chip-blue">
            {t("history.movement.generatedBadge")}
          </span>
        </div>
        <p className="muted">{t("history.movement.notEnoughPeriods")}</p>
      </section>
    );
  }

  const overviewMetrics = [
    {
      value: numberText(analysis.summary.transitionCount, locale),
      label: t("history.movement.metric.transitions"),
    },
    {
      value: numberText(analysis.summary.movementEventCount, locale),
      label: t("history.movement.metric.moveEvents"),
    },
    {
      value: numberText(analysis.summary.uniqueMovedStudentCount, locale),
      label: t("history.movement.metric.movedStudents"),
    },
    {
      value: numberText(analysis.summary.averageDistance, locale),
      label: t("history.movement.metric.averageDistance"),
    },
    {
      value: numberText(analysis.summary.maximumDistance, locale),
      label: t("history.movement.metric.maximumDistance"),
    },
  ];

  const transitionMetrics = selectedTransition
    ? [
        {
          value: `${selectedTransition.metrics.movedCount}/${selectedTransition.metrics.comparableStudentCount}`,
          label: t("history.movement.metric.movedComparable"),
        },
        {
          value: numberText(selectedTransition.metrics.stayedCount, locale),
          label: t("history.movement.metric.stayed"),
        },
        {
          value: `${selectedTransition.metrics.seatedCount}/${selectedTransition.metrics.unseatedCount}`,
          label: t("history.movement.metric.seatedUnseated"),
        },
        {
          value: numberText(selectedTransition.metrics.averageDistance, locale),
          label: t("history.movement.metric.averageDistance"),
        },
        {
          value: numberText(selectedTransition.metrics.maximumDistance, locale),
          label: t("history.movement.metric.maximumDistance"),
        },
      ]
    : [];
  const metricItems = mode === "overview" ? overviewMetrics : transitionMetrics;
  const knownDistanceCount =
    mode === "overview"
      ? analysis.summary.knownDistanceCount
      : (selectedTransition?.metrics.knownDistanceCount ?? 0);
  const movementCount =
    mode === "overview"
      ? analysis.summary.movementEventCount
      : (selectedTransition?.metrics.movedCount ?? 0);
  const visibleLevels =
    mode === "overview"
      ? levels.filter((level) => level !== "unknown")
      : levels;

  function handleTabKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    let nextMode: ViewMode | null = null;
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      nextMode = mode === "overview" ? "transition" : "overview";
    } else if (event.key === "Home") {
      nextMode = "overview";
    } else if (event.key === "End") {
      nextMode = "transition";
    }
    if (!nextMode) {
      return;
    }
    event.preventDefault();
    setMode(nextMode);
    (nextMode === "overview" ? overviewTabRef : transitionTabRef).current?.focus();
  }

  return (
    <section className="movement-card" data-testid="rotation-movement-heatmap">
      <div className="movement-heading">
        <div>
          <span className="eyebrow">{t("history.movement.eyebrow")}</span>
          <h2>{t("history.movement.title")}</h2>
          <p>{t("history.movement.subtitle")}</p>
        </div>
        <span className="chip chip-blue">
          {t("history.movement.generatedBadge")}
        </span>
      </div>

      <div
        className="view-switch movement-view-switch"
        role="tablist"
        aria-orientation="horizontal"
        aria-label={t("history.movement.viewLabel")}
      >
        <button
          ref={overviewTabRef}
          type="button"
          id="movement-overview-tab"
          role="tab"
          aria-controls="movement-heatmap-panel"
          aria-selected={mode === "overview"}
          data-active={mode === "overview"}
          tabIndex={mode === "overview" ? 0 : -1}
          onClick={() => setMode("overview")}
          onKeyDown={handleTabKeyDown}
        >
          {t("history.movement.overview")}
        </button>
        <button
          ref={transitionTabRef}
          type="button"
          id="movement-transition-tab"
          role="tab"
          aria-controls="movement-heatmap-panel"
          aria-selected={mode === "transition"}
          data-active={mode === "transition"}
          tabIndex={mode === "transition" ? 0 : -1}
          onClick={() => setMode("transition")}
          onKeyDown={handleTabKeyDown}
        >
          {t("history.movement.transition")}
        </button>
      </div>

      <div
        id="movement-heatmap-panel"
        role="tabpanel"
        aria-labelledby={
          mode === "overview"
            ? "movement-overview-tab"
            : "movement-transition-tab"
        }
        className="movement-tab-panel"
        tabIndex={0}
      >
        {mode === "transition" && selectedTransition ? (
          <label className="movement-transition-picker">
            <span>{t("history.movement.transitionPicker")}</span>
            <select
              value={selectedTransition.to.period}
              onChange={(event) =>
                setRequestedTargetPeriod(Number(event.target.value))
              }
            >
              {analysis.transitions.map((transition) => (
                <option key={transition.to.period} value={transition.to.period}>
                  {transitionLabel(transition, t)}
                </option>
              ))}
            </select>
          </label>
        ) : null}

        <div
          className="movement-metrics"
          aria-label={t("history.movement.metricsLabel")}
        >
          {metricItems.map((metric) => (
            <div className="movement-metric" key={metric.label}>
              <strong>{metric.value}</strong>
              <span>{metric.label}</span>
            </div>
          ))}
        </div>

        {knownDistanceCount < movementCount ? (
          <p className="movement-distance-note">
            {t("history.movement.distancePartial", {
              known: knownDistanceCount,
              total: movementCount,
            })}
          </p>
        ) : null}

        {analysis.seats.length === 0 ? (
          <p className="muted">{t("history.movement.noCoordinates")}</p>
        ) : (
          <div className="movement-visual">
            <div className="movement-front">{t("canvas.front")}</div>
            <div
              className="movement-grid"
              role="list"
              aria-label={
                mode === "overview"
                  ? t("history.movement.overviewGridLabel")
                  : t("history.movement.transitionGridLabel")
              }
              style={{
                gridTemplateColumns: `repeat(${columns}, minmax(72px, 1fr))`,
              }}
            >
              {mode === "overview"
                ? analysis.seatChurn.map((seat) => {
                    const label = seat.hasOccupant
                      ? t("history.movement.overviewSeatLabel", {
                          seat: seat.seatId,
                          changes: seat.occupantChangeCount,
                          transitions: seat.transitionCount,
                        })
                      : t("history.movement.alwaysEmptySeatLabel", {
                          seat: seat.seatId,
                        });
                    return (
                      <div
                        className="movement-seat"
                        data-level={seat.level}
                        data-testid={`movement-seat-${seat.seatId}`}
                        role="listitem"
                        aria-label={label}
                        title={label}
                        key={seat.seatId}
                        style={{
                          gridColumn: seat.column + 1,
                          gridRow: seat.row + 1,
                        }}
                      >
                        <small>{seat.seatId}</small>
                        <strong>
                          {seat.hasOccupant
                            ? `${seat.occupantChangeCount}/${seat.transitionCount}`
                            : "—"}
                        </strong>
                      </div>
                    );
                  })
                : transitionSeatData.map((seat) => {
                    const label = seat.studentName
                      ? t("history.movement.transitionSeatLabel", {
                          seat: seat.seatId,
                          student: seat.studentName,
                          from:
                            seat.fromSeatId ??
                            t("history.movement.newlySeated"),
                          distance: movementDistanceText(
                            seat.distance,
                            locale,
                            t,
                          ),
                        })
                      : t("history.movement.emptySeatLabel", {
                          seat: seat.seatId,
                        });
                    return (
                      <div
                        className="movement-seat movement-seat-transition"
                        data-level={seat.level}
                        data-testid={`movement-seat-${seat.seatId}`}
                        role="listitem"
                        aria-label={label}
                        title={label}
                        key={seat.seatId}
                        style={{
                          gridColumn: seat.column + 1,
                          gridRow: seat.row + 1,
                        }}
                      >
                        <small>{seat.seatId}</small>
                        <strong>{seat.studentName ?? "—"}</strong>
                        {seat.studentName ? (
                          <span>
                            {movementDistanceText(seat.distance, locale, t)}
                          </span>
                        ) : null}
                      </div>
                    );
                  })}
            </div>
          </div>
        )}

        <div
          className="movement-legend"
          aria-label={t("history.movement.legendLabel")}
        >
          {visibleLevels.map((level) => (
            <span key={level}>
              <i data-level={level} aria-hidden="true" />
              {t(legendKey(mode, level))}
            </span>
          ))}
        </div>

        {mode === "transition" ? (
          <div className="movement-list">
            <h3>{t("history.movement.listTitle")}</h3>
            {movedStudents.length === 0 ? (
              <p className="muted">{t("history.movement.noMoves")}</p>
            ) : (
              <ol>
                {movedStudents.map((movement) => (
                  <li key={movement.studentKey}>
                    <strong>{movement.studentName}</strong>
                    <span>
                      {movement.fromSeatId} → {movement.toSeatId}
                    </span>
                    <em>
                      {movementDistanceText(movement.distance, locale, t)}
                    </em>
                  </li>
                ))}
              </ol>
            )}
          </div>
        ) : null}
      </div>

      <p className="movement-generated-note">
        {t("history.movement.generatedHint")}
      </p>
    </section>
  );
}
