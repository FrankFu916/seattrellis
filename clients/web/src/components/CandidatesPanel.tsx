import { useEffect, useMemo, useState } from "react";

import { fetchDraftAudit } from "../api/client";
import type {
  DraftAuditReport,
  SeatAssignment,
} from "../api/types";
import {
  diffSeatIds,
  dimensionLabel,
  reasonCardFor,
  type DimensionKey,
} from "../domain/auditTerms";
import { describeApiError } from "../domain/errorMessages";
import type { Locale, Translate } from "../i18n/messages";

export type CandidateMeta = {
  draft_id: string;
  total_score: number;
  recommended: boolean;
  assignments: SeatAssignment[];
};

export type ReproInfo = {
  seed: string;
  solver: string;
  timeLimitSeconds: number;
  historyCount: number;
};

type CandidatesPanelProps = {
  candidates: CandidateMeta[];
  repro: ReproInfo;
  locale: Locale;
  t: Translate;
  onChoose: (draftId: string) => void;
};

function labelOf(index: number): string {
  return String.fromCharCode(65 + index);
}

/** D5 fused form: recommendation reason -> diff highlight -> details. */
export function CandidatesPanel({
  candidates,
  repro,
  locale,
  t,
  onChoose,
}: CandidatesPanelProps) {
  const recommendedIndex = candidates.findIndex(
    (candidate) => candidate.recommended,
  );
  const recommended = candidates[recommendedIndex] ?? candidates[0];
  const [compareIndex, setCompareIndex] = useState(() =>
    candidates.length > 1 && recommendedIndex !== 1 ? 1 : 0,
  );
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailMode, setDetailMode] = useState<"scores" | "rules">("scores");
  const [audits, setAudits] = useState<Record<string, DraftAuditReport>>({});
  const [auditError, setAuditError] = useState<string | null>(null);

  const compared = candidates[compareIndex] ?? recommended;

  useEffect(() => {
    let current = true;
    const wanted = new Set(
      [recommended?.draft_id, compared?.draft_id].filter(
        (id): id is string => Boolean(id),
      ),
    );
    void Promise.all(
      [...wanted].map(async (draftId) => {
        try {
          const report = await fetchDraftAudit(draftId);
          return { draftId, report };
        } catch (error) {
          return {
            draftId,
            report: null,
            message: describeApiError(error, t, "audit.auditFailed"),
          };
        }
      }),
    ).then((results) => {
      if (!current) {
        return;
      }
      const next: Record<string, DraftAuditReport> = {};
      let message: string | null = null;
      for (const result of results) {
        if (result.report) {
          next[result.draftId] = result.report;
        } else if (!message) {
          message = result.message;
        }
      }
      setAudits(next);
      setAuditError(message);
    });
    return () => {
      current = false;
    };
  }, [recommended?.draft_id, compared?.draft_id, t]);

  const recommendedAudit = recommended ? audits[recommended.draft_id] : undefined;
  const comparedAudit = compared ? audits[compared.draft_id] : undefined;
  const reason = useMemo(
    () => (recommendedAudit ? reasonCardFor(recommendedAudit, t) : null),
    [recommendedAudit, t],
  );
  const diff = useMemo(() => {
    if (!recommended || !compared || recommended.draft_id === compared.draft_id) {
      return null;
    }
    return diffSeatIds(recommended.assignments, compared.assignments);
  }, [recommended, compared]);

  const allDimensions: DimensionKey[] = [
    "fair_rotation_score",
    "avoid_recent_neighbors_score",
    "score_balance_score",
    "height_preference_score",
    "vision_preference_score",
    "diversity_score",
    "stability_score",
  ];

  function dimensionScore(
    report: DraftAuditReport | undefined,
    key: DimensionKey,
  ): number | null {
    const dimension = report?.score.breakdown[key];
    if (dimension?.status !== "available" || typeof dimension.score !== "number") {
      return null;
    }
    return dimension.score;
  }

  return (
    <section className="candidates-panel" aria-label={t("audit.candidateCount", { count: candidates.length })}>
      <header className="cand-head">
        <span className="chip chip-green">
          {t("audit.candidateCount", { count: candidates.length })}
        </span>
        {recommended ? (
          <span className="small muted">
            {t("audit.recommended")} · {labelOf(recommendedIndex)} ·{" "}
            {t("audit.points", { score: String(Math.round(recommended.total_score)) })}
          </span>
        ) : null}
        <span className="cand-spacer" aria-hidden="true" />
        <button
          type="button"
          className="secondary-button"
          data-testid="repro-toggle"
          onClick={() => setDetailOpen((open) => !open)}
        >
          {t("audit.detailTitle")}
          <span aria-hidden="true">{detailOpen ? "▴" : "▾"}</span>
        </button>
        {recommended ? (
          <button
            type="button"
            className="primary-button"
            onClick={() => onChoose(recommended.draft_id)}
          >
            {t("audit.choose", { label: labelOf(recommendedIndex) })}
          </button>
        ) : null}
      </header>

      {detailOpen ? (
        <div className="cand-detail" data-testid="cand-detail">
          <span className="repro-line">
            {t("audit.reproLine", {
              seed: repro.seed || "auto",
              solver: repro.solver,
              candidate: recommended?.draft_id ?? "-",
              history: String(repro.historyCount),
            })}
          </span>
        </div>
      ) : null}

      {reason && recommended ? (
        <div className="rec-card">
          <span className="rec-badge" aria-hidden="true">
            {labelOf(recommendedIndex)}
          </span>
          <div>
            <div className="rec-title">{t("audit.choose", { label: labelOf(recommendedIndex) })}</div>
            <div className="rec-body">
              {reason.reasons.map((item, index) => (
                <span key={index}>{item}</span>
              ))}
            </div>
            <div className="rec-hard">
              <span
                className={`chip ${
                  reason.hardSatisfied ? "chip-green" : "chip-red"
                }`}
              >
                {reason.hardSatisfied
                  ? t("audit.hardSatisfied", {
                      count: String(reason.checkedRuleCount),
                    })
                  : t("audit.hardViolations", {
                      count: String(reason.violationCount),
                    })}
              </span>
            </div>
          </div>
        </div>
      ) : null}

      {auditError ? (
        <p className="inline-error" role="alert">
          {auditError}
        </p>
      ) : null}

      {diff ? (
        <>
          <div className="plain-diff">
            {t("audit.plainDiff", { label: labelOf(compareIndex) })}
            <b>{t("audit.diffLegend", { count: String(diff.size) })}</b>
          </div>
          <div className="cand-compare">
            <div className="cand-plan">
              <div className="cand-plan-head">
                <span className="chip chip-blue">
                  {labelOf(recommendedIndex)} · {t("audit.recommended")}
                </span>
                <select
                  aria-label={t("audit.choose", { label: labelOf(recommendedIndex) })}
                  value={recommended.draft_id}
                  onChange={(event) => {
                    const index = candidates.findIndex(
                      (candidate) => candidate.draft_id === event.target.value,
                    );
                    onChoose(candidates[index].draft_id);
                  }}
                >
                  {candidates.map((candidate, index) => (
                    <option key={candidate.draft_id} value={candidate.draft_id}>
                      {labelOf(index)}
                    </option>
                  ))}
                </select>
              </div>
              <MiniSeatGrid assignments={recommended.assignments} diff={diff} t={t} />
            </div>
            <div className="cand-plan">
              <div className="cand-plan-head">
                <span className="chip chip-gray">{labelOf(compareIndex)}</span>
                <select
                  aria-label={t("audit.choose", { label: labelOf(compareIndex) })}
                  value={compared.draft_id}
                  onChange={(event) =>
                    setCompareIndex(
                      candidates.findIndex(
                        (candidate) => candidate.draft_id === event.target.value,
                      ),
                    )
                  }
                >
                  {candidates.map((candidate, index) => (
                    <option key={candidate.draft_id} value={candidate.draft_id}>
                      {labelOf(index)}
                    </option>
                  ))}
                </select>
              </div>
              <MiniSeatGrid assignments={compared.assignments} diff={diff} t={t} />
            </div>
          </div>
        </>
      ) : null}

      {recommendedAudit && comparedAudit ? (
        <div className="cand-details">
          <div className="view-switch" role="tablist">
            <button
              type="button"
              role="tab"
              aria-selected={detailMode === "scores"}
              data-active={detailMode === "scores"}
              onClick={() => setDetailMode("scores")}
            >
              {t("audit.scoreTableTitle")}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={detailMode === "rules"}
              data-active={detailMode === "rules"}
              onClick={() => setDetailMode("rules")}
            >
              {t("audit.perRuleTitle")}
            </button>
          </div>
          {detailMode === "scores" ? (
            <table className="mini score-table">
              <thead>
                <tr>
                  <th>{t("audit.scoreTableTitle")}</th>
                  <th>{labelOf(recommendedIndex)}</th>
                  <th>{labelOf(compareIndex)}</th>
                  <th>{t("audit.explanation")}</th>
                </tr>
              </thead>
              <tbody>
                {allDimensions.map((key) => {
                  const meta = dimensionLabel(key, t);
                  const a = dimensionScore(recommendedAudit, key);
                  const b = dimensionScore(comparedAudit, key);
                  return (
                    <tr key={key}>
                      <td>
                        <b>{meta.term}</b>
                        <small>{meta.hint}</small>
                      </td>
                      <td className="num">{a === null ? "—" : a}</td>
                      <td className="num">{b === null ? "—" : b}</td>
                      <td className="muted small">{meta.hint}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          ) : (
            <div className="rule-details">
              {allDimensions.map((key) => {
                const meta = dimensionLabel(key, t);
                const dimension = recommendedAudit.score.breakdown[key];
                if (dimension?.status !== "available") {
                  return null;
                }
                return (
                  <details className="rule-detail" key={key}>
                    <summary>
                      <span className="chip chip-amber">{t("rules.soft")}</span>
                      {meta.term}
                      <span className="num">{dimension.score ?? "—"}/100</span>
                    </summary>
                    <div className="rd-body">
                      {meta.hint}
                      {dimension.details && Object.keys(dimension.details).length > 0 ? (
                        <pre className="json-view">
                          {JSON.stringify(dimension.details, null, 2)}
                        </pre>
                      ) : null}
                    </div>
                  </details>
                );
              })}
            </div>
          )}
        </div>
      ) : null}
    </section>
  );
}

function MiniSeatGrid({
  assignments,
  diff,
  t,
}: {
  assignments: SeatAssignment[];
  diff: Set<string>;
  t: Translate;
}) {
  const columns =
    Math.max(0, ...assignments.map((seat) => seat.column)) + 1;
  const rows = Math.max(0, ...assignments.map((seat) => seat.row)) + 1;
  const byId = new Map(assignments.map((seat) => [seat.seatId, seat]));
  return (
    <div
      className="mini-grid"
      style={{ gridTemplateColumns: `repeat(${columns}, 1fr)` }}
    >
      {Array.from({ length: rows * columns }, (_, index) => {
        const row = Math.floor(index / columns);
        const column = index % columns;
        const seat = byId.get(`R${row + 1}C${column + 1}`);
        if (!seat) {
          return <span className="mini-cell mini-cell-empty" key={index} />;
        }
        const changed = diff.has(seat.seatId);
        return (
          <span
            className={`mini-cell${changed ? " mini-cell-diff" : ""}`}
            title={seat.seatId}
            key={seat.seatId}
          >
            <small>{seat.seatId}</small>
            {seat.student?.name ?? ""}
            {changed ? <em className="mini-diff-tag">{t("audit.changed")}</em> : null}
          </span>
        );
      })}
    </div>
  );
}
