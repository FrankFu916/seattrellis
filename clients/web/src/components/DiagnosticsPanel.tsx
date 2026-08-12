import type { DraftAuditReport, Student } from "../api/types";
import {
  fixAllOperations,
  issuesFromAudit,
  type DiagnosticIssue,
} from "../domain/diagnostics";
import type { Translate } from "../i18n/messages";

type DiagnosticsPanelProps = {
  report: DraftAuditReport | null;
  students: Student[];
  /** Seat id the canvas currently focuses (two-way link, D6). */
  focusSeatId: string | null;
  t: Translate;
  onFocusChange: (seatId: string | null) => void;
  /** Dispatch Rust editing operations (one atomic command). */
  onFix: (operations: Array<{ kind: string; payload: Record<string, string | number> }>) => void;
};

/**
 * Feasibility diagnostics (D6): severity-sorted issue list driven by the
 * Rust diagnostics report, linked to the canvas badges, with one-click
 * fixes that go through the Rust editing protocol and are re-validated by
 * the next audit fetch.
 */
export function DiagnosticsPanel({
  report,
  students,
  focusSeatId,
  t,
  onFocusChange,
  onFix,
}: DiagnosticsPanelProps) {
  const issues = issuesFromAudit(report, students);
  const errors = issues.filter((issue) => issue.severity === "error");
  const fixable = issues.filter(
    (issue) => issue.severity === "error" && issue.fix,
  );

  function fixIssue(issue: DiagnosticIssue) {
    if (!issue.fix) {
      return;
    }
    onFix([
      {
        kind: "move_student",
        payload: {
          student_key: issue.fix.student,
          seat_id: issue.fix.seatId,
        },
      },
    ]);
  }

  return (
    <section className="side-card diagnostics-panel" aria-labelledby="diagnostics-title">
      <header>
        <div>
          <span className="eyebrow">{t("diagnostics.title")}</span>
          <h2 id="diagnostics-title">
            {t("diagnostics.title")}
            {errors.length > 0 ? (
              <span className="diag-count" data-testid="diag-error-count">
                {errors.length}
              </span>
            ) : null}
          </h2>
        </div>
        {fixable.length > 0 ? (
          <button
            type="button"
            className="secondary-button"
            data-testid="fix-all"
            onClick={() => onFix(fixAllOperations(issues))}
          >
            {t("diagnostics.fixAll")}
          </button>
        ) : null}
      </header>
      <div className="diagnostics-body">
        {issues.length === 0 ? (
          <p className="muted diag-empty" data-testid="diag-empty">
            {t("diagnostics.allGood")}
          </p>
        ) : (
          <div className="issue-list">
            {issues.map((issue) => {
              const linked =
                focusSeatId !== null && issue.seatIds.includes(focusSeatId);
              return (
                <div
                  className={`issue-row issue-row-${issue.severity}`}
                  data-linked={linked}
                  key={issue.id}
                  onClick={() => {
                    if (issue.seatIds.length > 0) {
                      onFocusChange(linked ? null : issue.seatIds[0]);
                    }
                  }}
                >
                  <span
                    className={`chip ${
                      issue.severity === "error" ? "chip-red" : "chip-amber"
                    }`}
                  >
                    {issue.severity === "error"
                      ? t("rules.hard")
                      : t("rules.soft")}
                  </span>
                  <div className="i-body">
                    <div className="i-title">{t(issue.titleKey, issue.args)}</div>
                    {issue.seatIds.length > 0 ? (
                      <div className="i-seats">
                        {issue.seatIds.map((seatId) => (
                          <code key={seatId}>{seatId}</code>
                        ))}
                      </div>
                    ) : null}
                    {issue.severity === "error" && issue.fix ? (
                      <div className="i-fix">
                        <button
                          type="button"
                          className="primary-button"
                          data-testid={`fix-${issue.id}`}
                          onClick={(event) => {
                            event.stopPropagation();
                            fixIssue(issue);
                          }}
                        >
                          {t("diagnostics.fix")}
                        </button>
                      </div>
                    ) : null}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
