import type { ClassContext, ContextAction } from "../domain/navigation";
import type { Translate } from "../i18n/messages";

type ContextBarProps = {
  context: ClassContext;
  viewLabel: string;
  meta: string | null;
  action: ContextAction;
  isGenerating: boolean;
  canGenerate: boolean;
  t: Translate;
  onAction: (action: ContextAction) => void;
  onSaveAsClass: () => void;
};

/** All exports enter the same workspace; no hidden, stateful quick-save path. */
export function ContextBar({
  context,
  viewLabel,
  meta,
  action,
  isGenerating,
  canGenerate,
  t,
  onAction,
  onSaveAsClass,
}: ContextBarProps) {
  const disabled = isGenerating || (action.kind === "generate" && !canGenerate);
  return (
    <header className="context-bar">
      <div className="ctx-identity">
        <span className="ctx-context">
          {context.kind === "class" ? context.name : t("ctx.tempName")}
        </span>
        <span className="ctx-separator" aria-hidden="true">
          /
        </span>
        <span className="ctx-view">{viewLabel}</span>
      </div>
      {meta ? <span className="ctx-chip">{meta}</span> : null}
      <span className="ctx-spacer" aria-hidden="true" />
      {context.kind === "temp" ? (
        <button
          type="button"
          className="secondary-button ctx-save-as"
          onClick={onSaveAsClass}
        >
          {t("ctx.saveAsClass")}
        </button>
      ) : null}
      <div className="ctx-action">
        <button
          type="button"
          className="primary-button"
          disabled={disabled}
          onClick={() => onAction(action)}
        >
          {t(action.label)}
          {action.kind === "navigate" ? (
            <span aria-hidden="true">
              {action.target === "canvas" ? "←" : "→"}
            </span>
          ) : null}
        </button>
      </div>
    </header>
  );
}
