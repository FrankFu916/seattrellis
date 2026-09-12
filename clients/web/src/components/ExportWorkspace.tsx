import { useEffect, useMemo, useRef, useState } from "react";

import { exportDraft, previewExportDraft, RosterApiError } from "../api/client";
import type { CatalogOption } from "../api/types";
import { saveBlobWithDialog } from "../domain/desktop";
import { formatExportWarning } from "../domain/exportWarnings";
import {
  extractHtmlChartPreview,
  InvalidExportPreviewError,
} from "../domain/exportPreview";
import {
  availableExportFormats,
  buildExportRequest,
  DEFAULT_EXPORT_FORMAT,
  hasPaperLayout,
  isOfficeFormat,
  needsRenderedPreview,
  type ExportSettings,
} from "../domain/export";
import type { Locale, Translate } from "../i18n/messages";

type Props = {
  draftId: string | null;
  revision: number;
  title: string;
  formats: CatalogOption[];
  locale: Locale;
  t: Translate;
  onExported: () => void;
};

type PreparedExport = {
  key: string;
  blob: Blob;
  filename: string;
  previewUrl: string;
  format: string;
  warnings: string[];
  previewWarnings: string[];
};

function exportError(error: unknown, t: Translate): string {
  if (error instanceof InvalidExportPreviewError)
    return t("export.previewInvalid");
  if (error instanceof Error && error.name === "TimeoutError")
    return t("export.timeout");
  if (error instanceof RosterApiError && error.status === 409)
    return t("export.staleDraft");
  return t("export.prepareFailed");
}

export function ExportWorkspace({
  draftId,
  revision,
  title,
  formats,
  locale,
  t,
  onExported,
}: Props) {
  const supportedFormats = useMemo(
    () => availableExportFormats(formats),
    [formats],
  );
  const [settings, setSettings] = useState<ExportSettings>(() => ({
    format: supportedFormats.some(
      (format) => format.id === DEFAULT_EXPORT_FORMAT,
    )
      ? DEFAULT_EXPORT_FORMAT
      : (supportedFormats[0]?.id ?? DEFAULT_EXPORT_FORMAT),
    anonymize: false,
    showStudentIds: false,
    orientation: "landscape",
    paper: "a4",
    margin: 12,
  }));
  const [prepared, setPrepared] = useState<PreparedExport | null>(null);
  const [phase, setPhase] = useState<"idle" | "preparing" | "saving" | "saved">(
    "idle",
  );
  const [error, setError] = useState<string | null>(null);
  const [cancelled, setCancelled] = useState(false);
  const pending = useRef<AbortController | null>(null);
  const saving = useRef(false);
  const mounted = useRef(true);
  const request = buildExportRequest(
    settings,
    draftId ?? "",
    revision,
    title,
    locale,
  );
  const requestKey = JSON.stringify(request);
  const latestKey = useRef(requestKey);
  latestKey.current = requestKey;
  const current = prepared?.key === requestKey ? prepared : null;
  const currentPreview = useRef(current);
  currentPreview.current = current;
  const paperLayout = hasPaperLayout(settings.format);
  const selectedFormat = supportedFormats.find(
    (format) => format.id === settings.format,
  );
  const busy = phase === "preparing" || phase === "saving";

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      pending.current?.abort();
      pending.current = null;
    };
  }, []);

  useEffect(() => {
    pending.current?.abort();
    pending.current = null;
    setPrepared(null);
    setPhase("idle");
    setError(null);
    setCancelled(false);
  }, [requestKey]);

  useEffect(
    () => () => {
      if (prepared) URL.revokeObjectURL(prepared.previewUrl);
    },
    [prepared],
  );

  function updateSettings(changes: Partial<ExportSettings>) {
    if (saving.current) return;
    pending.current?.abort();
    pending.current = null;
    setSettings((previous) => ({ ...previous, ...changes }));
  }

  async function prepare() {
    if (!draftId || !selectedFormat || pending.current || saving.current)
      return;
    const controller = new AbortController();
    pending.current = controller;
    const key = requestKey;
    setPhase("preparing");
    setError(null);
    setCancelled(false);
    try {
      const [file, preview] = await Promise.all([
        exportDraft(request, controller.signal),
        needsRenderedPreview(settings.format)
          ? previewExportDraft(request, controller.signal)
          : Promise.resolve(null),
      ]);
      if (
        !mounted.current ||
        pending.current !== controller ||
        latestKey.current !== key
      )
        return;
      const previewBlob =
        settings.format === "print-html"
          ? await extractHtmlChartPreview(file.blob)
          : (preview?.blob ?? file.blob);
      if (
        !mounted.current ||
        pending.current !== controller ||
        latestKey.current !== key
      )
        return;
      const warnings = [...new Set(file.warnings ?? [])];
      setPrepared({
        key,
        blob: file.blob,
        filename: file.filename,
        previewUrl: URL.createObjectURL(previewBlob),
        format: settings.format,
        warnings,
        previewWarnings: [...new Set(preview?.warnings ?? [])].filter(
          (warning) => !warnings.includes(warning),
        ),
      });
      setPhase("idle");
    } catch (failure) {
      // Cancel the companion document preview if either response fails.
      controller.abort();
      if (
        mounted.current &&
        pending.current === controller &&
        latestKey.current === key
      ) {
        if (failure instanceof RosterApiError && failure.status === 409)
          setPrepared(null);
        setError(exportError(failure, t));
        setPhase("idle");
      }
    } finally {
      if (pending.current === controller) pending.current = null;
    }
  }

  function cancelPreparation() {
    pending.current?.abort();
    pending.current = null;
    setPhase("idle");
    setCancelled(true);
  }

  function rejectPreview(artifact: PreparedExport) {
    if (
      !mounted.current ||
      currentPreview.current !== artifact ||
      latestKey.current !== artifact.key
    )
      return;
    // An image error can arrive after a newer request/revision. Only the
    // still-displayed artifact may invalidate this workspace's save action.
    currentPreview.current = null;
    pending.current?.abort();
    pending.current = null;
    setPrepared(null);
    setPhase("idle");
    setCancelled(false);
    setError(t("export.previewDisplayFailed"));
  }

  async function save() {
    if (
      !current ||
      currentPreview.current !== current ||
      saving.current ||
      pending.current
    )
      return;
    saving.current = true;
    setPhase("saving");
    setError(null);
    try {
      const outcome = await saveBlobWithDialog(current.filename, current.blob);
      if (
        !mounted.current ||
        latestKey.current !== current.key ||
        currentPreview.current !== current
      )
        return;
      if (outcome === "cancelled") {
        setPhase("idle");
        return;
      }
      if (outcome === "unavailable") {
        const url = URL.createObjectURL(current.blob);
        const link = document.createElement("a");
        link.href = url;
        link.download = current.filename;
        document.body.appendChild(link);
        link.click();
        link.remove();
        window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      }
      setPhase("saved");
      onExported();
    } catch {
      if (mounted.current && latestKey.current === current.key && currentPreview.current === current) {
        setError(t("export.saveFailed"));
        setPhase("idle");
      }
    } finally {
      saving.current = false;
    }
  }

  return (
    <section
      className="export-workspace"
      aria-labelledby="export-workspace-title"
    >
      <header className="export-workspace-heading">
        <div>
          <span className="eyebrow">{t("export.workspaceEyebrow")}</span>
          <h1 id="export-workspace-title">{t("export.workspaceTitle")}</h1>
          <p>{t("export.workspaceHint")}</p>
        </div>
        <span className="export-class-title">{title}</span>
      </header>
      <div className="export-workspace-body">
        <div className="export-settings">
          <fieldset disabled={phase === "saving"}>
            <legend>
              <span>01</span> {t("export.format")}
            </legend>
            <label className="export-field">
              <span className="sr-only">{t("export.format")}</span>
              <select
                value={settings.format}
                onChange={(event) =>
                  updateSettings({ format: event.target.value })
                }
              >
                {supportedFormats.map((format) => (
                  <option key={format.id} value={format.id}>
                    {format.name[locale]}
                  </option>
                ))}
              </select>
            </label>
            <p className="export-field-hint">
              {selectedFormat?.description[locale]}
            </p>
          </fieldset>
          <fieldset disabled={phase === "saving"}>
            <legend>
              <span>02</span> {t("export.namePolicy")}
            </legend>
            <div className="export-name-options">
              {([false, true] as const).map((anonymize) => (
                <label
                  key={String(anonymize)}
                  data-selected={settings.anonymize === anonymize}
                >
                  <input
                    type="radio"
                    name="export-names"
                    checked={settings.anonymize === anonymize}
                    onChange={() => updateSettings({ anonymize })}
                  />
                  <span>
                    <strong>
                      {t(
                        anonymize
                          ? "export.anonymousNames"
                          : "export.realNames",
                      )}
                    </strong>
                    <small>
                      {t(
                        anonymize
                          ? "export.anonymousHint"
                          : "export.realNamesHint",
                      )}
                    </small>
                  </span>
                </label>
              ))}
            </div>
            {!settings.anonymize ? (
              <label className="export-checkbox">
                <input
                  type="checkbox"
                  checked={settings.showStudentIds}
                  onChange={(event) =>
                    updateSettings({ showStudentIds: event.target.checked })
                  }
                />
                {t("export.showStudentIds")}
              </label>
            ) : null}
            <p className="export-field-hint">{t("export.safeContentHint")}</p>
          </fieldset>
          <fieldset disabled={phase === "saving"}>
            <legend>
              <span>03</span> {t("export.layout")}
            </legend>
            {paperLayout ? (
              <>
                <div className="export-layout-fields">
                  <label className="export-field">
                    <span>{t("export.paperSize")}</span>
                    <select
                      value={settings.paper}
                      onChange={(event) =>
                        updateSettings({
                          paper: event.target.value as ExportSettings["paper"],
                        })
                      }
                    >
                      <option value="a4">A4</option>
                      <option value="a3">A3</option>
                      <option value="letter">Letter</option>
                    </select>
                  </label>
                  <label className="export-field">
                    <span>{t("export.orientation")}</span>
                    <select
                      value={settings.orientation}
                      onChange={(event) =>
                        updateSettings({
                          orientation: event.target
                            .value as ExportSettings["orientation"],
                        })
                      }
                    >
                      <option value="landscape">{t("export.landscape")}</option>
                      <option value="portrait">{t("export.portrait")}</option>
                    </select>
                  </label>
                </div>
                <label className="export-field">
                  <span>{t("export.margins")}</span>
                  <select
                    value={settings.margin}
                    onChange={(event) =>
                      updateSettings({ margin: Number(event.target.value) })
                    }
                  >
                    <option value={12}>{t("export.marginNormal")}</option>
                    <option value={6}>{t("export.marginNarrow")}</option>
                    <option value={20}>{t("export.marginWide")}</option>
                  </select>
                </label>
                <p className="export-field-hint">{t("export.autoFitHint")}</p>
              </>
            ) : (
              <p className="export-field-hint">
                {t(
                  settings.format === "pptx"
                    ? "export.slideLayoutHint"
                    : "export.sheetLayoutHint",
                )}
              </p>
            )}
          </fieldset>
        </div>
        <section
          className="export-preview"
          aria-labelledby="export-preview-title"
          aria-busy={phase === "preparing"}
        >
          <header>
            <h2 id="export-preview-title">{t("export.previewTitle")}</h2>
            <span className="export-preview-badge" role="status">
              {phase === "preparing"
                ? t("export.preparing")
                : current
                  ? t("export.previewReady")
                  : t("export.previewNotReady")}
            </span>
          </header>
          <div className="export-preview-content">
            {current ? (
              <img
                key={`${current.key}:${current.previewUrl}`}
                src={current.previewUrl}
                alt={t("export.documentPreview")}
                title={t("export.documentPreview")}
                onError={() => rejectPreview(current)}
              />
            ) : (
              <div className="export-preview-empty">
                <span
                  className={
                    phase === "preparing"
                      ? "export-progress-mark is-spinning"
                      : "export-progress-mark"
                  }
                  aria-hidden="true"
                >
                  {phase === "preparing" ? "◌" : "▤"}
                </span>
                <h3>
                  {t(
                    phase === "preparing"
                      ? "export.preparing"
                      : "export.prepareTitle",
                  )}
                </h3>
                <p>
                  {t(
                    phase === "preparing"
                      ? "export.preparingHint"
                      : "export.prepareHint",
                  )}
                </p>
              </div>
            )}
          </div>
          <footer>
            <p className="export-preview-note">
              {t(
                settings.format === "pdf"
                  ? "export.pdfPreviewHint"
                  : settings.format === "print-html"
                    ? "export.htmlPreviewHint"
                    : isOfficeFormat(settings.format)
                      ? "export.officePreviewHint"
                      : "export.exactPreviewHint",
              )}
            </p>
            {current && current.warnings.length > 0 ? (
              <div className="export-quality-notes" role="status">
                <strong>{t("export.warnings")}</strong>
                <ul>
                  {current.warnings.map((warning, index) => (
                    <li key={index}>{formatExportWarning(warning, t)}</li>
                  ))}
                </ul>
              </div>
            ) : null}
            {current && current.previewWarnings.length > 0 ? (
              <div className="export-quality-notes" role="status">
                <strong>{t("export.previewWarnings")}</strong>
                <ul>
                  {current.previewWarnings.map((warning, index) => (
                    <li key={index}>{formatExportWarning(warning, t)}</li>
                  ))}
                </ul>
              </div>
            ) : null}
            {error ? (
              <p className="export-error" role="alert">
                {error}
              </p>
            ) : null}
            {!draftId ? <p role="alert">{t("export.noDraft")}</p> : null}
            {phase === "saved" ? (
              <p className="export-feedback" role="status">
                {t("export.saved", { filename: current?.filename ?? "" })}
              </p>
            ) : cancelled ? (
              <p role="status">{t("export.cancelled")}</p>
            ) : null}
            <div className="export-workspace-actions">
              <button
                className={current ? "secondary-button" : "primary-button"}
                type="button"
                disabled={!draftId || !selectedFormat || busy}
                onClick={() => void prepare()}
              >
                {t(
                  phase === "preparing"
                    ? "export.preparing"
                    : error && !current
                      ? "export.retry"
                      : current
                        ? "export.regenerate"
                        : "export.prepare",
                )}
              </button>
              {phase === "preparing" ? (
                <button
                  className="secondary-button"
                  type="button"
                  onClick={cancelPreparation}
                >
                  {t("export.cancelPreparation")}
                </button>
              ) : null}
              <button
                className="primary-button"
                type="button"
                disabled={!current || busy}
                onClick={() => void save()}
              >
                {phase === "saving" ? t("action.saving") : t("export.saveFile")}
              </button>
            </div>
          </footer>
        </section>
      </div>
    </section>
  );
}
