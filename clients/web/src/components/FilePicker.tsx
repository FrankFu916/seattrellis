import { useCallback, useEffect, useRef, useState } from "react";

import { RosterApiError, fetchTrustedRoot, readTrustedFile } from "../api/client";
import { isTauriDesktop, isTrustedRelativePath, pickFileWithDialog } from "../domain/desktop";
import type { Translate } from "../i18n/messages";

const ROSTER_EXTENSIONS = ["csv", "xlsx", "xls"];

type FilePickerProps = {
  /** Deliver the chosen file; the parent owns the upload flow. */
  onFile: (file: File) => void;
  /** Disable all entries while the parent is uploading/previewing. */
  busy?: boolean;
  /** Name of the file chosen so far; switches the card to "change file". */
  fileName?: string | null;
  t: Translate;
};

/**
 * Unified file selection (PD-D14): one component with three entries —
 * ① native dialog (Tauri desktop), ② drag-and-drop with semantic border
 * highlight (all platforms), ③ typed path relative to the backend's
 * trusted root (all platforms; absolute paths are rejected). The browser
 * additionally keeps the `input[type=file]` fallback.
 */
export function FilePicker({ onFile, busy = false, fileName = null, t }: FilePickerProps) {
  const [dropping, setDropping] = useState(false);
  const [dialogOpening, setDialogOpening] = useState(false);
  const [pathInput, setPathInput] = useState("");
  const [pathReading, setPathReading] = useState(false);
  const [pathError, setPathError] = useState<string | null>(null);
  const [trustedRoot, setTrustedRoot] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const tauriDesktop = isTauriDesktop();

  useEffect(() => {
    let cancelled = false;
    fetchTrustedRoot()
      .then((root) => {
        if (!cancelled) {
          setTrustedRoot(root);
        }
      })
      .catch(() => {
        // The root is a hint, not a gate; the backend enforces it anyway.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const handleDialog = useCallback(async () => {
    setDialogOpening(true);
    setPathError(null);
    try {
      const file = await pickFileWithDialog(ROSTER_EXTENSIONS, t("filePicker.dialogLabel"));
      if (file) {
        onFile(file);
      }
    } catch {
      setPathError(t("filePicker.pathFailed", { message: t("filePicker.dialogFailed") }));
    } finally {
      setDialogOpening(false);
    }
  }, [onFile, t]);

  const handlePathRead = useCallback(async () => {
    const relPath = pathInput.trim();
    if (!isTrustedRelativePath(relPath)) {
      setPathError(t("filePicker.pathInvalid"));
      return;
    }
    setPathReading(true);
    setPathError(null);
    try {
      const file = await readTrustedFile(relPath);
      onFile(file);
      setPathInput("");
    } catch (err) {
      if (err instanceof RosterApiError) {
        const message =
          err.status === 404
            ? t("filePicker.pathMissingFile")
            : err.status === 413
              ? t("filePicker.pathTooLarge")
              : t("filePicker.pathReadFailed");
        setPathError(message);
      } else {
        console.error("Trusted-root file read failed", err);
        setPathError(t("filePicker.pathReadFailed"));
      }
    } finally {
      setPathReading(false);
    }
  }, [onFile, pathInput, t]);

  return (
    <div
      className={`file-picker${dropping ? " file-picker-dropping" : ""}`}
      onDragEnter={(event) => {
        event.preventDefault();
        if (!busy) {
          setDropping(true);
        }
      }}
      onDragOver={(event) => event.preventDefault()}
      onDragLeave={() => setDropping(false)}
      onDrop={(event) => {
        event.preventDefault();
        setDropping(false);
        if (busy) {
          return;
        }
        const file = event.dataTransfer.files?.[0];
        if (file) {
          onFile(file);
        }
      }}
    >
      <span className="file-picker-icon" aria-hidden="true">
        ↑
      </span>
      <strong>{fileName ? t("roster.changeFile") : t("roster.replace")}</strong>
      <small>
        {dropping
          ? t("filePicker.dropActive")
          : fileName
            ? t("roster.selectedFile", { name: fileName })
            : `${t("roster.fileHint")}${t("filePicker.dropHint")}`}
      </small>
      <div className="file-picker-actions">
        {tauriDesktop ? (
          <button
            type="button"
            className="primary-button"
            onClick={handleDialog}
            disabled={busy || dialogOpening}
          >
            {dialogOpening ? t("roster.desktopOpening") : t("roster.desktopOpen")}
          </button>
        ) : (
          <label className="file-picker-browser-button">
            <span>{t("roster.browserOpen")}</span>
            <input
              ref={inputRef}
              type="file"
              accept=".csv,.xlsx,.xls"
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file) {
                  onFile(file);
                }
                if (inputRef.current) {
                  inputRef.current.value = "";
                }
              }}
              disabled={busy}
            />
          </label>
        )}
      </div>

      <div className="file-picker-path">
        <label htmlFor="file-picker-path-input">{t("filePicker.pathLabel")}</label>
        <div className="file-picker-path-row">
          <input
            id="file-picker-path-input"
            type="text"
            value={pathInput}
            onChange={(event) => {
              setPathInput(event.target.value);
              setPathError(null);
            }}
            placeholder={t("filePicker.pathPlaceholder")}
            disabled={busy || pathReading}
            aria-invalid={pathError ? true : undefined}
          />
          <button
            type="button"
            className="secondary-button"
            onClick={handlePathRead}
            disabled={busy || pathReading || pathInput.trim() === ""}
          >
            {pathReading ? t("filePicker.pathReading") : t("filePicker.pathRead")}
          </button>
        </div>
        {trustedRoot ? <small className="file-picker-root">{t("filePicker.rootHint", { root: trustedRoot })}</small> : null}
        {pathError ? (
          <p className="file-picker-path-error" role="alert">
            {pathError}
          </p>
        ) : null}
      </div>
    </div>
  );
}
