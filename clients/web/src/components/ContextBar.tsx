import { useEffect, useRef, useState } from "react";

import type { CatalogOption } from "../api/types";
import type {
  ClassContext,
  ContextAction,
} from "../domain/navigation";
import type { Locale, Translate } from "../i18n/messages";

type ContextBarProps = {
  context: ClassContext;
  viewLabel: string;
  meta: string | null;
  action: ContextAction;
  exportFormats: CatalogOption[];
  locale: Locale;
  isGenerating: boolean;
  canGenerate: boolean;
  t: Translate;
  onAction: (action: ContextAction) => void;
  onQuickExport: (formatId: string) => void;
  onExportSettings: () => void;
  onSaveAsClass: () => void;
};

function actionLabel(
  action: ContextAction,
  t: Translate,
): string | null {
  switch (action.kind) {
    case "navigate":
    case "generate":
    case "preview":
      return t(action.label);
    case "exportMenu":
      return t("ctx.export");
  }
}

export function ContextBar({
  context,
  viewLabel,
  meta,
  action,
  exportFormats,
  locale,
  isGenerating,
  canGenerate,
  t,
  onAction,
  onQuickExport,
  onExportSettings,
  onSaveAsClass,
}: ContextBarProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const isTemp = context.kind === "temp";

  useEffect(() => {
    if (!menuOpen) {
      return;
    }
    function closeOnOutside(event: MouseEvent) {
      if (!menuRef.current?.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    }
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", closeOnOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOnOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [menuOpen]);

  const isExportMenu = action.kind === "exportMenu";
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
      {isTemp ? (
        <button
          type="button"
          className="secondary-button ctx-save-as"
          onClick={onSaveAsClass}
        >
          {t("ctx.saveAsClass")}
        </button>
      ) : null}
      <div className="ctx-action" ref={menuRef}>
        {isExportMenu ? (
          <>
            <button
              type="button"
              className="primary-button"
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              onClick={() => setMenuOpen((open) => !open)}
            >
              {t("ctx.export")}
              <span className="ctx-caret" aria-hidden="true">
                ▾
              </span>
            </button>
            {menuOpen ? (
              <div className="ctx-menu" role="menu">
                {exportFormats.map((format) => (
                  <button
                    type="button"
                    role="menuitem"
                    key={format.id}
                    onClick={() => {
                      setMenuOpen(false);
                      onQuickExport(format.id);
                    }}
                  >
                    {format.name[locale]}
                  </button>
                ))}
                <div className="ctx-menu-separator" role="separator" />
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setMenuOpen(false);
                    onExportSettings();
                  }}
                >
                  {t("ctx.exportSettings")}
                </button>
              </div>
            ) : null}
          </>
        ) : (
          <button
            type="button"
            className="primary-button"
            disabled={disabled}
            onClick={() => onAction(action)}
          >
            {actionLabel(action, t)}
            {action.kind === "navigate" ? (
              <span aria-hidden="true">→</span>
            ) : null}
          </button>
        )}
      </div>
    </header>
  );
}
