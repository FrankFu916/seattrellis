import { useEffect, useMemo, useState } from "react";

import { fetchDraftAudit } from "../api/client";
import type { DraftAuditReport, SeatAssignment } from "../api/types";
import {
  diffSeatIds,
  dimensionLabel,
  reasonCardFor,
  type DimensionKey,
} from "../domain/auditTerms";
import { describeApiError } from "../domain/errorMessages";
import {
  candidateLabel as labelOf,
  compareCandidateStudents,
} from "../domain/candidateComparison";
import type { Locale, Translate } from "../i18n/messages";

export type CandidateMeta = {
  draft_id: string;
  total_score: number;
  recommended: boolean;
  assignments: SeatAssignment[];
  revision: number;
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
  activeDraftId?: string | null;
  onChoose: (draftId: string) => void;
};

/** D5 fused form: recommendation reason -> diff highlight -> details. */
export function CandidatesPanel({
  candidates,
  repro,
  locale,
  t,
  activeDraftId,
  onChoose,
}: CandidatesPanelProps) {
  const recommendedIndex = Math.max(
    0,
    candidates.findIndex((candidate) => candidate.recommended),
  );
  const recommended = candidates[recommendedIndex] ?? candidates[0];
  const [leftId, setLeftId] = useState<string | null>(
    recommended?.draft_id ?? null,
  );
  const [rightId, setRightId] = useState<string | null>(
    () =>
      candidates.find(
        (candidate) => candidate.draft_id !== recommended?.draft_id,
      )?.draft_id ??
      recommended?.draft_id ??
      null,
  );
  const [query, setQuery] = useState("");
  const [changesOnly, setChangesOnly] = useState(true);
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailMode, setDetailMode] = useState<"scores" | "rules">("scores");
  const [audits, setAudits] = useState<Record<string, DraftAuditReport>>({});
  const [auditError, setAuditError] = useState<string | null>(null);

  const left =
    candidates.find((candidate) => candidate.draft_id === leftId) ??
    recommended;
  const compared =
    candidates.find((candidate) => candidate.draft_id === rightId) ??
    candidates.find((candidate) => candidate.draft_id !== left?.draft_id) ??
    left;
  const leftIndex = candidates.indexOf(left);
  const compareIndex = candidates.indexOf(compared);
  const auditKey = [recommended, left, compared]
    .filter(Boolean)
    .map((candidate) => `${candidate.draft_id}:${candidate.revision}`)
    .join("|");
  const [loadedAuditKey, setLoadedAuditKey] = useState("");

  useEffect(() => {
    let current = true;
    const wanted = new Set(
      [recommended?.draft_id, left?.draft_id, compared?.draft_id].filter(
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
      setLoadedAuditKey(auditKey);
    });
    return () => {
      current = false;
    };
  }, [auditKey, t]);

  const auditsReady = loadedAuditKey === auditKey;
  const recommendedAudit =
    recommended && auditsReady ? audits[recommended.draft_id] : undefined;
  const leftAudit = left && auditsReady ? audits[left.draft_id] : undefined;
  const comparedAudit =
    compared && auditsReady ? audits[compared.draft_id] : undefined;
  const reason = useMemo(
    () => (recommendedAudit ? reasonCardFor(recommendedAudit, t) : null),
    [recommendedAudit, t],
  );
  const diff = useMemo(() => {
    return diffSeatIds(left?.assignments ?? [], compared?.assignments ?? []);
  }, [left, compared]);
  const movements = useMemo(
    () =>
      compareCandidateStudents(
        left?.assignments ?? [],
        compared?.assignments ?? [],
      ),
    [left, compared],
  );
  const changedCount = movements.filter((movement) => movement.changed).length;
  const search = query.trim().toLocaleLowerCase(locale);
  const visibleMovements = movements.filter(
    (movement) =>
      (!changesOnly || movement.changed) &&
      (!search ||
        [
          movement.studentName,
          movement.studentId,
          movement.fromSeatId,
          movement.toSeatId,
        ].some((value) => value?.toLocaleLowerCase(locale).includes(search))),
  );

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
    if (
      dimension?.status !== "available" ||
      typeof dimension.score !== "number"
    ) {
      return null;
    }
    return dimension.score;
  }

  return (
    <section
      className="candidates-panel"
      aria-label={t("audit.candidateCount", { count: candidates.length })}
    >
      <header className="cand-head">
        <span className="chip chip-green">
          {t("audit.candidateCount", { count: candidates.length })}
        </span>
        {recommended ? (
          <span className="small muted">
            {t("audit.recommended")} · {labelOf(recommendedIndex)} ·{" "}
            {t("audit.generatedScore", {
              score: String(Math.round(recommended.total_score)),
            })}
          </span>
        ) : null}
        <span className="cand-spacer" aria-hidden="true" />
        <button
          type="button"
          className="secondary-button"
          data-testid="repro-toggle"
          aria-expanded={detailOpen}
          onClick={() => setDetailOpen((open) => !open)}
        >
          {t("audit.detailTitle")}
          <span aria-hidden="true">{detailOpen ? "▴" : "▾"}</span>
        </button>
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
            <div className="rec-title">
              {t("audit.choose", { label: labelOf(recommendedIndex) })}
            </div>
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

      {auditError && auditsReady ? (
        <p className="inline-error" role="alert">
          {auditError}
        </p>
      ) : null}

      {left && compared ? (
        <>
          <div className="plain-diff">
            {t("audit.comparePair", {
              left: labelOf(leftIndex),
              right: labelOf(compareIndex),
            })}{" "}
            <b>{t("audit.diffLegend", { count: String(diff.size) })}</b>
          </div>
          <div className="cand-compare">
            {[left, compared].map((candidate, side) => (
              <div className="cand-plan" key={side}>
                <div className="cand-plan-head">
                  <select
                    aria-label={t(
                      side === 0 ? "audit.compareLeft" : "audit.compareRight",
                    )}
                    value={candidate.draft_id}
                    onChange={(event) =>
                      (side === 0 ? setLeftId : setRightId)(event.target.value)
                    }
                  >
                    {candidates.map((option, index) => (
                      <option key={option.draft_id} value={option.draft_id}>
                        {labelOf(index)}
                        {option.recommended
                          ? ` · ${t("audit.recommended")}`
                          : ""}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="secondary-button"
                    disabled={candidate.draft_id === activeDraftId}
                    onClick={() => onChoose(candidate.draft_id)}
                  >
                    {candidate.draft_id === activeDraftId
                      ? t("audit.currentPlan")
                      : t("audit.choose", {
                          label: labelOf(candidates.indexOf(candidate)),
                        })}
                  </button>
                </div>
                <MiniSeatGrid
                  assignments={candidate.assignments}
                  diff={diff}
                  t={t}
                />
              </div>
            ))}
          </div>
          <details className="cand-movements">
            <summary>
              {t("audit.studentChanges", { count: changedCount })}
            </summary>
            <div className="cand-movement-controls">
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                aria-label={t("audit.searchChanges")}
                placeholder={t("audit.searchChanges")}
              />
              <label>
                <input
                  type="checkbox"
                  checked={changesOnly}
                  onChange={(event) => setChangesOnly(event.target.checked)}
                />
                {t("audit.onlyChanges")}
              </label>
            </div>
            <div className="cand-table-scroll">
              <table className="score-table">
                <caption className="sr-only">
                  {t("audit.comparePair", {
                    left: labelOf(leftIndex),
                    right: labelOf(compareIndex),
                  })}
                </caption>
                <thead>
                  <tr>
                    <th scope="col">{t("audit.student")}</th>
                    <th scope="col">{labelOf(leftIndex)}</th>
                    <th scope="col">{labelOf(compareIndex)}</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleMovements.map((movement) => (
                    <tr key={movement.studentId}>
                      <th scope="row">
                        {movement.studentName}{" "}
                        <small>{movement.studentId}</small>
                      </th>
                      <td>{movement.fromSeatId ?? t("audit.notSeated")}</td>
                      <td>{movement.toSeatId ?? t("audit.notSeated")}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {visibleMovements.length === 0 ? (
                <p role="status">{t("audit.noChangesFound")}</p>
              ) : null}
            </div>
          </details>
        </>
      ) : null}

      {leftAudit && comparedAudit ? (
        <div className="cand-details">
          <div
            className="view-switch"
            role="group"
            aria-label={t("audit.detailTitle")}
          >
            <button
              type="button"
              aria-pressed={detailMode === "scores"}
              data-active={detailMode === "scores"}
              onClick={() => setDetailMode("scores")}
            >
              {t("audit.scoreTableTitle")}
            </button>
            <button
              type="button"
              aria-pressed={detailMode === "rules"}
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
                  <th>{labelOf(leftIndex)}</th>
                  <th>{labelOf(compareIndex)}</th>
                  <th>{t("audit.explanation")}</th>
                </tr>
              </thead>
              <tbody>
                {allDimensions.map((key) => {
                  const meta = dimensionLabel(key, t);
                  const a = dimensionScore(leftAudit, key);
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
                const dimension = leftAudit.score.breakdown[key];
                if (dimension?.status !== "available") {
                  return null;
                }
                return (
                  <details className="rule-detail" key={key}>
                    <summary>
                      <span className="chip chip-amber">{t("rules.soft")}</span>
                      {labelOf(leftIndex)} · {meta.term}
                      <span className="num">{dimension.score ?? "—"}/100</span>
                    </summary>
                    <div className="rd-body">
                      {meta.hint}
                      {dimension.details &&
                      Object.keys(dimension.details).length > 0 ? (
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
  const columns = Math.max(0, ...assignments.map((seat) => seat.column)) + 1;
  const rows = Math.max(0, ...assignments.map((seat) => seat.row)) + 1;
  return (
    <div
      className="mini-grid"
      style={{
        gridTemplateColumns: `repeat(${columns}, minmax(64px, 1fr))`,
        gridTemplateRows: `repeat(${rows}, minmax(46px, auto))`,
      }}
    >
      {assignments.map((seat) => {
        const changed = diff.has(seat.seatId);
        return (
          <span
            className={`mini-cell${changed ? " mini-cell-diff" : ""}`}
            title={seat.seatId}
            style={{ gridRow: seat.row + 1, gridColumn: seat.column + 1 }}
            key={seat.seatId}
          >
            <small>{seat.seatId}</small>
            {seat.student?.name ?? ""}
            {changed ? (
              <em className="mini-diff-tag">{t("audit.changed")}</em>
            ) : null}
          </span>
        );
      })}
    </div>
  );
}
