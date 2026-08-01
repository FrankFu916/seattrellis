import type { RotationPlan } from "../api/types";
import type { Translate } from "../i18n/messages";

type RotationPlanSummaryProps = {
  plan: RotationPlan;
  t: Translate;
};

function numberValue(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

export function RotationPlanSummary({ plan, t }: RotationPlanSummaryProps) {
  const repeatedPairs = numberValue(plan.pair_repeat_summary.repeated_pair_count);
  const maxPairOccurrences = numberValue(
    plan.pair_repeat_summary.max_occurrences,
  );

  return (
    <section className="rotation-plan-summary" data-testid="rotation-plan-summary">
      <div className="rotation-plan-heading">
        <div>
          <span className="eyebrow">{t("rotation.summary")}</span>
          <h2>{plan.name}</h2>
        </div>
        <span className="rotation-period-count">
          {t("rotation.periods", { count: plan.periods.length })}
        </span>
      </div>
      <div className="rotation-plan-metrics">
        <span>
          {t("rotation.repeatedPairs", { count: repeatedPairs })}
        </span>
        <span>
          {t("rotation.maxPairOccurrences", { count: maxPairOccurrences })}
        </span>
      </div>
      <ol className="rotation-period-list">
        {plan.periods.map((period) => (
          <li key={period.period}>
            <strong>
              {t("rotation.period", {
                period: period.period,
                label: period.label,
              })}
            </strong>
            <small>
              {t("rotation.assignedSeats", {
                count: period.snapshot.assignments.length,
              })}
            </small>
          </li>
        ))}
      </ol>
      {plan.warnings.map((warning, index) => (
        <p className="rotation-plan-warning" role="status" key={`${warning}-${index}`}>
          {t("rotation.warning", { message: warning })}
        </p>
      ))}
      <p className="muted">{t("rotation.firstPeriodNote")}</p>
    </section>
  );
}
