import type { Diagnostic } from "../domain/workflow";
import type { Translate } from "../i18n/messages";

type DiagnosticsPanelProps = {
  diagnostics: Diagnostic[];
  t: Translate;
};

export function DiagnosticsPanel({
  diagnostics,
  t,
}: DiagnosticsPanelProps) {
  return (
    <section className="side-card diagnostics-card" aria-labelledby="checks-title">
      <header>
        <span className="card-icon card-icon-check" aria-hidden="true">
          ✓
        </span>
        <div>
          <h2 id="checks-title">{t("diagnostics.title")}</h2>
          <p>{t("diagnostics.subtitle")}</p>
        </div>
      </header>
      <ul aria-live="polite">
        {diagnostics.map((diagnostic) => (
          <li key={diagnostic.id} data-tone={diagnostic.tone}>
            <span aria-hidden="true" />
            {t(diagnostic.message, { count: diagnostic.count ?? 0 })}
          </li>
        ))}
      </ul>
    </section>
  );
}

