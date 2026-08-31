import { useState } from "react";

import type {
  HistorySnapshotPayload,
  ProjectRotationLoadResponse,
  RotationPlan,
  SeatAssignment,
  Student,
} from "../api/types";
import {
  snapshotAssignments,
  snapshotIsRestorable,
  snapshotStudents,
} from "../domain/snapshots";
import type { Locale, Translate } from "../i18n/messages";
import { HistoryFilesCard } from "./HistoryFilesCard";
import { ProjectWorkspacePanel } from "./ProjectWorkspacePanel";
import { RotationMovementHeatmap } from "./RotationMovementHeatmap";

type HistoryRotationPanelProps = {
  locale: Locale;
  t: Translate;
  rotationPlan: RotationPlan | null;
  rotationDraftIds: string[];
  historyFileNames: string[];
  historySnapshots: HistorySnapshotPayload[];
  historyError: string | null;
  assignments: SeatAssignment[];
  students: Student[];
  isDirty: boolean;
  activeRotationPeriod: number;
  onRotationLoad: (result: ProjectRotationLoadResponse) => void;
  onRotationPeriodSelect: (period: number) => void;
  onHistoryFilesChange: (files: File[]) => void;
  onHistoryClear: () => void;
  /** Restore a snapshot's roster + plan into the workbench (dirty-guarded). */
  onRestoreSnapshot: (snapshot: HistorySnapshotPayload) => void;
};

/**
 * History / rotation view (D7 fused form): a timeline for reviewing past
 * plans (with restore) and a period-card view for rotation planning — two
 * mental models, one tab switch, no animation.
 */
export function HistoryRotationPanel({
  locale,
  t,
  rotationPlan,
  rotationDraftIds,
  historyFileNames,
  historySnapshots,
  historyError,
  assignments,
  students,
  isDirty,
  activeRotationPeriod,
  onRotationLoad,
  onRotationPeriodSelect,
  onHistoryFilesChange,
  onHistoryClear,
  onRestoreSnapshot,
}: HistoryRotationPanelProps) {
  const [view, setView] = useState<"review" | "plan">("review");

  return (
    <section className="control-panel history-panel" aria-labelledby="history-panel-title">
      <div className="panel-heading">
        <span className="eyebrow">{t("nav.history")}</span>
        <h1 id="history-panel-title">{t("history.title")}</h1>
        <p>{t("history.subtitle")}</p>
        <div className="view-switch" role="tablist" aria-label={t("history.viewLabel")}>
          <button
            type="button"
            role="tab"
            aria-selected={view === "review"}
            data-active={view === "review"}
            onClick={() => setView("review")}
          >
            {t("history.review")}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={view === "plan"}
            data-active={view === "plan"}
            onClick={() => setView("plan")}
          >
            {t("history.plan")}
          </button>
        </div>
      </div>

      <div className="panel-content history-content">
        <HistoryFilesCard
          fileNames={historyFileNames}
          snapshotCount={historySnapshots.length}
          error={historyError}
          t={t}
          onChange={onHistoryFilesChange}
          onClear={onHistoryClear}
        />

        {view === "review" ? (
          <div className="history-timeline" data-testid="history-review">
            {historySnapshots.length === 0 ? (
              <p className="muted">{t("history.reviewEmpty")}</p>
            ) : (
              <ol className="timeline">
                {historySnapshots.map((snapshot, index) => {
                  const restorable = snapshotIsRestorable(snapshot);
                  const roster = snapshotStudents(snapshot);
                  return (
                    <li className="tl-item" key={index}>
                      <div className="tl-head">
                        <span className="tl-title">
                          {t("history.period", { number: index + 1 })}
                        </span>
                        <span className="tl-meta">
                          {historyFileNames[index] ?? ""}
                        </span>
                      </div>
                      <div className="tl-desc">
                        {t("app.students", { count: roster.length })}
                        {restorable ? (
                          <span className="chip chip-green">
                            {t("history.restorable")}
                          </span>
                        ) : (
                          <span className="chip chip-amber">
                            {t("history.notRestorable")}
                          </span>
                        )}
                      </div>
                      <div className="tl-actions">
                        <button
                          type="button"
                          className="secondary-button"
                          disabled={!restorable}
                          onClick={() => onRestoreSnapshot(snapshot)}
                        >
                          {t("history.restore")}
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ol>
            )}
            {historySnapshots.length > 0 ? (
              <p className="note">
                {isDirty
                  ? t("history.restoreDirtyHint")
                  : t("history.restoreHint")}
              </p>
            ) : null}
          </div>
        ) : (
          <div className="history-plan" data-testid="history-plan">
            {!rotationPlan ? (
              <p className="muted">{t("rotation.none")}</p>
            ) : (
              <>
                <RotationMovementHeatmap
                  plan={rotationPlan}
                  layoutSeats={assignments}
                  activePeriod={activeRotationPeriod}
                  locale={locale}
                  t={t}
                />
                <div className="period-cards">
                  {rotationPlan.periods.map((period) => (
                    <button
                      type="button"
                      className="period-card"
                      data-active={period.period === activeRotationPeriod}
                      key={period.period}
                      onClick={() => onRotationPeriodSelect(period.period)}
                    >
                      <span className="pc-head">
                        <span>{period.label || t("history.period", { number: period.period })}</span>
                        <span className="chip chip-blue">
                          {t("history.periodStatus", {
                            status: period.snapshot.solver_status,
                          })}
                        </span>
                      </span>
                      <span className="pc-grid">
                        {period.snapshot.assignments
                          .slice(0, 8)
                          .map((entry) => (
                            <span className="pc-seat" key={entry.seat_id}>
                              <b>{entry.seat_id}</b> {entry.student_name}
                            </span>
                          ))}
                      </span>
                      <span className="small muted">
                        {t("history.periodSeats", {
                          count: period.snapshot.assignments.length,
                        })}
                      </span>
                    </button>
                  ))}
                </div>
                <p className="note">
                  {t("history.periodHint")}
                </p>
              </>
            )}
            <details className="project-tools">
              <summary>{t("project.toolsTitle")}</summary>
              <ProjectWorkspacePanel
                locale={locale}
                t={t}
                rotationPlan={rotationPlan}
                rotationDraftIds={rotationDraftIds}
                onRotationLoad={onRotationLoad}
              />
            </details>
          </div>
        )}
      </div>
    </section>
  );
}

/** Apply a snapshot's roster into the current assignments (pure helper). */
export function restoreSnapshotPlan(
  snapshot: HistorySnapshotPayload,
  currentAssignments: SeatAssignment[],
): { students: Student[]; assignments: SeatAssignment[] } {
  const students = snapshotStudents(snapshot);
  const assignments = snapshotAssignments(snapshot, currentAssignments, students);
  return { students, assignments };
}
