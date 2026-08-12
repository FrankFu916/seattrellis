import type { DraftAuditReport } from "../api/types";
import type { MessageKey, Translate } from "../i18n/messages";

/**
 * G-1 term map: program dimension ids -> teacher-facing copy. The decision
 * record (PD-D5-CANDIDATES) freezes the mapping; the UI renders only these
 * labels and never shows `fair_rotation_score`-style ids.
 */
export type DimensionKey =
  | "fair_rotation_score"
  | "avoid_recent_neighbors_score"
  | "score_balance_score"
  | "height_preference_score"
  | "vision_preference_score"
  | "diversity_score"
  | "stability_score";

export const DIMENSION_META: Array<{
  key: DimensionKey;
  term: MessageKey;
  hint: MessageKey;
}> = [
  {
    key: "fair_rotation_score",
    term: "audit.fairRotation",
    hint: "audit.fairRotationHint",
  },
  {
    key: "avoid_recent_neighbors_score",
    term: "audit.avoidRecentNeighbors",
    hint: "audit.avoidRecentNeighborsHint",
  },
  {
    key: "score_balance_score",
    term: "audit.scoreBalance",
    hint: "audit.scoreBalanceHint",
  },
  {
    key: "height_preference_score",
    term: "audit.heightBack",
    hint: "audit.heightBackHint",
  },
  {
    key: "vision_preference_score",
    term: "audit.visionFront",
    hint: "audit.visionFrontHint",
  },
  {
    key: "diversity_score",
    term: "audit.diversity",
    hint: "audit.diversityHint",
  },
  {
    key: "stability_score",
    term: "audit.stability",
    hint: "audit.stabilityHint",
  },
];

export function dimensionLabel(
  key: string,
  t: Translate,
): { term: string; hint: string } {
  const meta = DIMENSION_META.find((item) => item.key === key);
  return meta
    ? { term: t(meta.term), hint: t(meta.hint) }
    : { term: key, hint: "" };
}

export type ReasonCard = {
  /** Teacher-facing reasons, one per strong dimension. */
  reasons: string[];
  hardSatisfied: boolean;
  checkedRuleCount: number;
  violationCount: number;
  /** Available dimensions sorted by score (best first). */
  ranking: Array<{ key: string; label: string; score: number }>;
};

/**
 * Build the recommendation reason card (D5 ①) from the recommended
 * candidate's audit: hard requirements first, then the strongest scored
 * dimensions in plain language.
 */
export function reasonCardFor(
  report: DraftAuditReport,
  t: Translate,
): ReasonCard {
  const breakdown = report.score.breakdown;
  const ranking = DIMENSION_META.flatMap((meta) => {
    const dimension = breakdown[meta.key];
    if (
      dimension?.status !== "available" ||
      typeof dimension.score !== "number"
    ) {
      return [];
    }
    return [
      {
        key: meta.key,
        label: t(meta.term),
        score: dimension.score,
      },
    ];
  }).sort((a, b) => b.score - a.score);

  const top = ranking.slice(0, 2);
  const reasons =
    top.length > 0
      ? [
          t("audit.reasonBest", {
            label: top[0].label,
            score: String(top[0].score),
          }),
          ...(top[1]
            ? [
                t("audit.reasonAlso", {
                  label: top[1].label,
                  score: String(top[1].score),
                }),
              ]
            : []),
        ]
      : [t("audit.reasonNoDimensions")];

  return {
    reasons,
    hardSatisfied: report.audit.hard_constraint_summary.all_satisfied,
    checkedRuleCount: report.audit.hard_constraint_summary.checked_rule_count,
    violationCount: report.audit.hard_constraint_summary.violation_count,
    ranking,
  };
}

/** Seat ids whose occupant differs between two plans (D5 ② diff highlight). */
export function diffSeatIds(
  a: Array<{ seatId: string; student?: { id: string } | undefined }>,
  b: Array<{ seatId: string; student?: { id: string } | undefined }>,
): Set<string> {
  const byId = new Map(b.map((seat) => [seat.seatId, seat]));
  const diff = new Set<string>();
  for (const seat of a) {
    const other = byId.get(seat.seatId);
    if (!other || other.student?.id !== seat.student?.id) {
      diff.add(seat.seatId);
    }
  }
  for (const seat of b) {
    if (!diff.has(seat.seatId)) {
      const other = a.find((item) => item.seatId === seat.seatId);
      if (!other || other.student?.id !== seat.student?.id) {
        diff.add(seat.seatId);
      }
    }
  }
  return diff;
}
