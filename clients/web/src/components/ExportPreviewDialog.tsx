import { useEffect, useRef } from "react";

import type { SeatAssignment } from "../api/types";
import type { Translate } from "../i18n/messages";
import { SeatingCanvas } from "./SeatingCanvas";

type ExportPreviewDialogProps = {
  assignments: SeatAssignment[];
  orientation: "portrait" | "landscape";
  open: boolean;
  t: Translate;
  onClose: () => void;
};

export function ExportPreviewDialog({
  assignments,
  orientation,
  open,
  t,
  onClose,
}: ExportPreviewDialogProps) {
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const previouslyFocused = document.activeElement as HTMLElement | null;
    closeButtonRef.current?.focus();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
        return;
      }
      if (event.key === "Tab") {
        const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
        );
        if (!focusable || focusable.length === 0) {
          event.preventDefault();
          return;
        }
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      previouslyFocused?.focus();
    };
  }, [open, onClose]);

  if (!open) {
    return null;
  }

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        className="preview-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="preview-dialog-title"
        onMouseDown={(event) => event.stopPropagation()}
        ref={dialogRef}
      >
        <header>
          <div>
            <span className="eyebrow">
              {t("export.page", { orientation: t(`export.${orientation}`) })}
            </span>
            <h2 id="preview-dialog-title">{t("export.previewTitle")}</h2>
            <p>{t("export.previewHint")}</p>
          </div>
          <button
            className="icon-button"
            type="button"
            aria-label={t("action.close")}
            onClick={onClose}
            ref={closeButtonRef}
          >
            ×
          </button>
        </header>
        <div className={`preview-page preview-${orientation}`}>
          <div className="preview-page-heading">
            <strong>{t("app.className")}</strong>
            <span>{t("canvas.front")}</span>
          </div>
          <SeatingCanvas
            assignments={assignments}
            interactive={false}
            t={t}
          />
        </div>
        <footer>
          <p>{t("export.fileNote")}</p>
          <button className="primary-button" type="button" disabled>
            {t("action.save")}
          </button>
        </footer>
      </section>
    </div>
  );
}
