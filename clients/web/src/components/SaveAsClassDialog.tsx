import { useState } from "react";

import type { Translate } from "../i18n/messages";

type SaveAsClassDialogProps = {
  open: boolean;
  t: Translate;
  onClose: () => void;
  onConfirm: (name: string) => void;
};

/**
 * "Save as class" (G-5): turns the scratch workspace into a named class
 * context. The draft stays session-scoped; persisting class artifacts to the
 * local project service lands with the alpha.1 default-path work.
 */
export function SaveAsClassDialog({
  open,
  t,
  onClose,
  onConfirm,
}: SaveAsClassDialogProps) {
  const [name, setName] = useState("");

  if (!open) {
    return null;
  }

  const trimmed = name.trim();
  const valid = trimmed.length > 0 && trimmed.length <= 40;

  function confirm() {
    if (!valid) {
      return;
    }
    onConfirm(trimmed);
    setName("");
  }

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <div
        className="dialog-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="save-as-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <h2 id="save-as-title">{t("saveAs.title")}</h2>
        <p className="dialog-hint">{t("saveAs.hint")}</p>
        <label className="dialog-field">
          <span>{t("saveAs.name")}</span>
          <input
            autoFocus
            value={name}
            placeholder={t("saveAs.namePlaceholder")}
            maxLength={40}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                confirm();
              }
            }}
          />
        </label>
        <div className="dialog-actions">
          <button type="button" className="secondary-button" onClick={onClose}>
            {t("action.close")}
          </button>
          <button
            type="button"
            className="primary-button"
            disabled={!valid}
            onClick={confirm}
          >
            {t("saveAs.confirm")}
          </button>
        </div>
      </div>
    </div>
  );
}
