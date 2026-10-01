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
  onSave?: () => void;
  onOpen?: () => void;
  onCancelGenerate?: () => void;
  isSaving?: boolean;
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
  onSave,
  onOpen,
  onCancelGenerate,
  isSaving = false,
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
      {onOpen ? <button type="button" className="secondary-button" disabled={isSaving} onClick={onOpen}>{t("classFile.open")}</button> : null}
      {onSave ? <button type="button" className="secondary-button" disabled={isSaving || isGenerating} onClick={onSave}>{t(isSaving ? "classFile.saving" : "classFile.save")}</button> : null}
      {isGenerating && onCancelGenerate ? <button type="button" className="secondary-button" onClick={onCancelGenerate}>{t("generate.cancel")}</button> : null}
      <button
          type="button"
          className="secondary-button ctx-save-as"
          onClick={onSaveAsClass}
          disabled={isSaving || isGenerating}
        >
          {t("ctx.saveAsClass")}
      </button>
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
