import type { ClassContext } from "../domain/navigation";
import type { Translate } from "../i18n/messages";

type ClassContextGuideProps = {
  context: ClassContext;
  t: Translate;
  onOpenTools: () => void;
};

/**
 * Guidance for a freshly selected class context (W4): the workbench resets to
 * a scratch draft when a class is chosen from the sidebar, so an explicit
 * notice replaces the "empty demo under the class name" confusion and points
 * at the project tools that load the class's saved data.
 */
export function ClassContextGuide({
  context,
  t,
  onOpenTools,
}: ClassContextGuideProps) {
  if (context.kind !== "class") {
    return null;
  }
  return (
    <section
      className="first-run class-guide"
      data-testid="class-guide"
      aria-labelledby="class-guide-title"
    >
      <div className="first-run-heading">
        <strong id="class-guide-title">{t("ctx.classGuideTitle")}</strong>
        <small>{t("ctx.classGuideBody")}</small>
        <button type="button" className="text-button" onClick={onOpenTools}>
          {t("ctx.classGuideAction")}
        </button>
      </div>
    </section>
  );
}
