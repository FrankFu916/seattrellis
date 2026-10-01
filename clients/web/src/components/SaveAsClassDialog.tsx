import { useEffect, useRef, useState } from "react";

import type { Translate } from "../i18n/messages";

type SaveAsClassDialogProps = {
  open: boolean;
  t: Translate;
  onClose: () => void;
  onConfirm: (name: string) => void;
  busy?: boolean;
  error?: string | null;
};

/**
 * "Save as class" (G-5): turns the scratch workspace into a named class
 * context after writing a portable document containing source data and drafts.
 */
export function SaveAsClassDialog({
  open,
  t,
  onClose,
  onConfirm,
  busy = false,
  error = null,
}: SaveAsClassDialogProps) {
  const [name, setName] = useState("");
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const busyRef = useRef(busy);
  busyRef.current = busy;
  useEffect(() => {
    if (!open) return;
    const previousFocus = document.activeElement as HTMLElement | null;
    dialogRef.current?.querySelector<HTMLInputElement>("input")?.focus();
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busyRef.current) closeRef.current();
      }
      if (event.key === "Tab") {
        const controls = [...(dialogRef.current?.querySelectorAll<HTMLElement>("input:not(:disabled), button:not(:disabled), [tabindex='0']") ?? [])];
        if (controls.length === 0) { event.preventDefault(); dialogRef.current?.focus(); return; }
        const first = controls[0];
        const last = controls.at(-1);
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => { document.removeEventListener("keydown", onKeyDown); previousFocus?.focus(); };
  }, [open]);

  if (!open) {
    return null;
  }

  const trimmed = name.trim();
  const valid = trimmed.length > 0 && trimmed.length <= 40;

  function confirm() {
    if (!valid || busy) {
      return;
    }
    onConfirm(trimmed);

  }

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={() => { if (!busy) onClose(); }}>
      <div
        className="dialog-card"
        ref={dialogRef}
        tabIndex={-1}
        aria-busy={busy}
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
            disabled={busy}
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
        {error ? <p className="inline-error" role="alert">{error}</p> : null}
        <div className="dialog-actions">
          <button type="button" className="secondary-button" disabled={busy} onClick={onClose}>
            {t("action.close")}
          </button>
          <button
            type="button"
            className="primary-button"
            disabled={!valid || busy}
            onClick={confirm}
          >
            {t(busy ? "classFile.saving" : "saveAs.confirm")}
          </button>
        </div>
      </div>
    </div>
  );
}
