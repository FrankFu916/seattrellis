import type { MessageKey, Translate } from "../i18n/messages";
import { CheckIcon } from "./icons";

export type FirstRunProgress = {
  roster: boolean;
  room: boolean;
  rules: boolean;
  generate: boolean;
  export: boolean;
};

type FirstRunChecklistProps = {
  progress: FirstRunProgress;
  t: Translate;
  onDismiss: () => void;
};

const STEPS: Array<{
  key: keyof FirstRunProgress;
  label: MessageKey;
}> = [
  { key: "roster", label: "firstRun.step1" },
  { key: "room", label: "firstRun.step2" },
  { key: "rules", label: "firstRun.step3" },
  { key: "generate", label: "firstRun.step4" },
  { key: "export", label: "firstRun.step5" },
];

/**
 * First-run guidance (D1): shown only while the class has no history and not
 * dismissed; each step ticks as the teacher reaches it; the strip hides
 * itself after the first plan is generated ("用过即收").
 */
export function FirstRunChecklist({
  progress,
  t,
  onDismiss,
}: FirstRunChecklistProps) {
  const currentStep = STEPS.findIndex((step) => !progress[step.key]);

  return (
    <section
      className="first-run"
      aria-label={t("firstRun.title")}
      aria-describedby="first-run-hint"
    >
      <div className="first-run-heading">
        <strong>{t("firstRun.title")}</strong>
        <small id="first-run-hint">{t("firstRun.hint")}</small>
        <button
          type="button"
          className="text-button"
          onClick={onDismiss}
        >
          {t("firstRun.dismiss")}
        </button>
      </div>
      <ol className="first-run-steps">
        {STEPS.map((step, index) => {
          const done = progress[step.key];
          return (
            <li
              key={step.key}
              data-done={done}
              data-current={!done && index === currentStep}
            >
              <span className="first-run-tick" aria-hidden="true">
                {done ? <CheckIcon size={13} /> : index + 1}
              </span>
              <span>{t(step.label)}</span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
