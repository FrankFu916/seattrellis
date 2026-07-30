import {
  getStepIndex,
  workflowSteps,
  type WorkflowStep,
} from "../domain/workflow";
import type { Translate } from "../i18n/messages";

type StepNavigationProps = {
  activeStep: WorkflowStep;
  t: Translate;
  onStepChange: (step: WorkflowStep) => void;
};

export function StepNavigation({
  activeStep,
  t,
  onStepChange,
}: StepNavigationProps) {
  const activeIndex = getStepIndex(activeStep);

  return (
    <nav className="step-navigation" aria-label={t("nav.label")}>
      <ol>
        {workflowSteps.map((step, index) => {
          const state =
            index === activeIndex
              ? "active"
              : index < activeIndex
                ? "complete"
                : "upcoming";
          return (
            <li key={step} data-state={state}>
              <button
                type="button"
                onClick={() => onStepChange(step)}
                aria-current={step === activeStep ? "step" : undefined}
              >
                <span className="step-number" aria-hidden="true">
                  {state === "complete" ? "✓" : index + 1}
                </span>
                <span>{t(`step.${step}`)}</span>
              </button>
            </li>
          );
        })}
      </ol>
    </nav>
  );
}

