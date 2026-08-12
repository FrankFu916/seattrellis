import type {
  ProjectRotationLoadResponse,
  RotationPlan,
} from "../api/types";
import type { Locale, Translate } from "../i18n/messages";
import { HistoryFilesCard } from "./HistoryFilesCard";
import { ProjectWorkspacePanel } from "./ProjectWorkspacePanel";
import { RotationPlanSummary } from "./RotationPlanSummary";

type HistoryRotationPanelProps = {
  locale: Locale;
  t: Translate;
  rotationPlan: RotationPlan | null;
  rotationDraftIds: string[];
  historyFileNames: string[];
  historySnapshotCount: number;
  historyError: string | null;
  activeRotationPeriod: number;
  onRotationLoad: (result: ProjectRotationLoadResponse) => void;
  onRotationPeriodSelect: (period: number) => void;
  onHistoryFilesChange: (files: File[]) => void;
  onHistoryClear: () => void;
};

/**
 * History / rotation view (PD-D7 interim form until the timeline + period
 * cards land): rotation summary, history snapshots, and the project tools.
 */
export function HistoryRotationPanel({
  locale,
  t,
  rotationPlan,
  rotationDraftIds,
  historyFileNames,
  historySnapshotCount,
  historyError,
  activeRotationPeriod,
  onRotationLoad,
  onRotationPeriodSelect,
  onHistoryFilesChange,
  onHistoryClear,
}: HistoryRotationPanelProps) {
  return (
    <section className="control-panel history-panel" aria-labelledby="history-panel-title">
      <div className="panel-heading">
        <span className="eyebrow">{t("nav.history")}</span>
        <h1 id="history-panel-title">{t("history.title")}</h1>
        <p>{t("history.subtitle")}</p>
      </div>
      <div className="panel-content history-content">
        {rotationPlan ? (
          <RotationPlanSummary
            plan={rotationPlan}
            t={t}
            activePeriod={activeRotationPeriod}
            onPeriodSelect={onRotationPeriodSelect}
          />
        ) : (
          <p className="muted">{t("rotation.none")}</p>
        )}
        <HistoryFilesCard
          fileNames={historyFileNames}
          snapshotCount={historySnapshotCount}
          error={historyError}
          t={t}
          onChange={onHistoryFilesChange}
          onClear={onHistoryClear}
        />
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
    </section>
  );
}
