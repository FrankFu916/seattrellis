import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  hasDesktopBridge,
  openDesktopRosterFile,
  RosterApiError,
  previewRosterUpdate,
  uploadRosterDraft,
} from "../api/client";
import { demoStudents } from "../api/demo";
import type {
  RosterDraftResponse,
  RosterConflictItem,
  RosterFieldName,
  RosterMappingIssueItem,
  RosterMappingItem,
  RosterUpdateMode,
  RosterUpdatePreviewResponse,
  Student,
} from "../api/types";
import type { Locale, Translate } from "../i18n/messages";

const FIELD_OPTIONS: RosterFieldName[] = [
  "name",
  "student_id",
  "gender",
  "height_cm",
  "score",
  "vision",
  "tags",
  "needs",
  "notes",
];

type Phase = "idle" | "uploading" | "mapping" | "previewing" | "error";

type RosterImportPanelProps = {
  locale: Locale;
  t: Translate;
  currentStudents: Student[];
  currentRevision: number;
  onImportConfirmed: (students: Student[]) => void;
};

function fieldLabel(field: RosterFieldName | null, t: Translate): string {
  if (!field) {
    return t("roster.identityField");
  }
  return t(`roster.field.${field}` as Parameters<Translate>[0]);
}

function mappingIssueMessage(issue: RosterMappingIssueItem, t: Translate): string {
  switch (issue.code) {
    case "missing_identity":
      return t("roster.mappingIssueMissingIdentity");
    case "ambiguous_header":
      return t("roster.mappingIssueAmbiguous", {
        field: fieldLabel(issue.field, t),
      });
    default:
      // Older local services returned a human-readable English message but
      // did not include the stable issue code. Never expose that transport
      // text in the teacher-facing panel.
      return t("roster.mappingIssueGeneric");
  }
}

function conflictMessage(conflict: RosterConflictItem, t: Translate): string {
  switch (conflict.code) {
    case "duplicate_existing_student_id":
      return t("roster.conflictDuplicateExistingId");
    case "duplicate_incoming_student_id":
      return t("roster.conflictDuplicateIncomingId");
    case "duplicate_incoming_name":
      return t("roster.conflictDuplicateIncomingName");
    case "existing_student_matched_twice":
      return t("roster.conflictMatchedTwice");
    case "duplicate_resulting_identifier":
      return t("roster.conflictDuplicateResult");
    case "ambiguous_student_id":
      return t("roster.conflictAmbiguousId");
    case "ambiguous_name":
      return t("roster.conflictAmbiguousName");
    case "student_id_name_mismatch":
      return t("roster.conflictIdNameMismatch");
    default:
      return t("roster.conflictGeneric");
  }
}

export function RosterImportPanel({
  locale,
  t,
  currentStudents,
  currentRevision,
  onImportConfirmed,
}: RosterImportPanelProps) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [isPreviewing, setIsPreviewing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<RosterDraftResponse | null>(null);
  const [mapping, setMapping] = useState<Record<number, RosterFieldName | null>>({});
  const [mode, setMode] = useState<RosterUpdateMode>("incremental");
  const [preview, setPreview] = useState<RosterUpdatePreviewResponse | null>(null);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [desktopReady, setDesktopReady] = useState(false);
  const [desktopOpening, setDesktopOpening] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const updateDesktopReady = () => setDesktopReady(hasDesktopBridge());
    updateDesktopReady();
    window.addEventListener("pywebviewready", updateDesktopReady);
    return () => window.removeEventListener("pywebviewready", updateDesktopReady);
  }, []);

  const friendlyError = useCallback((err: unknown): string => {
    if (err instanceof RosterApiError) {
      switch (err.code) {
        case "roster_file_required":
          return t("roster.errorFileRequired");
        case "roster_file_too_large":
          return t("roster.errorFileTooLarge");
        case "invalid_roster_file":
          return t("roster.errorInvalidFile");
        case "roster_mapping_rejected":
          return t("roster.errorMappingRejected");
        case "feature_unavailable":
          return t("roster.errorUnavailable");
        case "roster_draft_not_found":
          return t("roster.errorExpired");
        default:
          return t("roster.errorGeneric");
      }
    }
    if (err instanceof Error) {
      // Do not expose implementation details (for example a raw fetch or
      // parser error) in the teacher-facing panel.  Keep the original error
      // available to the browser console while presenting a useful action.
      console.error("Roster import failed", err);
      return t("roster.errorGeneric");
    }
    return t("roster.errorGeneric");
  }, [t]);

  async function uploadFile(file: File) {
    setSelectedFile(file.name);
    setPhase("uploading");
    setError(null);
    setDraft(null);
    setPreview(null);
    try {
      const response = await uploadRosterDraft(file);
      const initial: Record<number, RosterFieldName | null> = {};
      for (const col of response.columns) {
        const suggested = response.suggested_mapping.find(
          (item) => item.column_index === col.index,
        );
        initial[col.index] = suggested ? suggested.field : null;
      }
      setMapping(initial);
      setDraft(response);
      setPhase("mapping");
    } catch (err) {
      setError(friendlyError(err));
      setPhase("error");
    }
  }

  async function handleFileChange(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (file) {
      await uploadFile(file);
    }
  }

  function fileFromDesktop(payload: DesktopRosterFile): File {
    const binary = atob(payload.content_base64);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return new File([bytes], payload.name, { type: payload.content_type });
  }

  async function handleDesktopFile() {
    setDesktopOpening(true);
    setError(null);
    try {
      const payload = await openDesktopRosterFile();
      if (payload) {
        await uploadFile(fileFromDesktop(payload));
      }
    } catch (err) {
      setError(friendlyError(err));
      setPhase("error");
    } finally {
      setDesktopOpening(false);
    }
  }

  function handleMappingChange(columnIndex: number, field: RosterFieldName | null) {
    setMapping((prev) => ({ ...prev, [columnIndex]: field }));
    // A preview belongs to the exact mapping and mode used to create it.
    // Clear it as soon as either input changes so the confirmation button can
    // never apply an outdated result.
    setPreview(null);
    setError(null);
  }

  async function handlePreview() {
    if (!draft) {
      return;
    }
    const hasIdentityMapping = Object.values(mapping).some(
      (field) => field === "name" || field === "student_id",
    );
    if (!hasIdentityMapping) {
      setError(t("roster.mappingIssueMissingIdentity"));
      return;
    }
    setPhase("previewing");
    setIsPreviewing(true);
    setError(null);
    const items: RosterMappingItem[] = [];
    for (const [colStr, field] of Object.entries(mapping)) {
      if (field) {
        items.push({ field, column_index: Number(colStr) });
      }
    }
    const updatedFields = Array.from(
      new Set(items.map((item) => item.field)),
    );
    try {
      const result = await previewRosterUpdate(draft.draft_id, {
        mapping: items,
        mode,
        current_students: currentStudents,
        current_revision: currentRevision,
        updated_fields: updatedFields,
      });
      setPreview(result);
      setPhase("mapping");
      setIsPreviewing(false);
    } catch (err) {
      setError(friendlyError(err));
      // Keep the draft and mapping visible so the teacher can correct the
      // selection and retry.  A preview failure is not an upload failure.
      setPhase("mapping");
      setIsPreviewing(false);
    }
  }

  function handleConfirm() {
    if (!preview?.can_apply || hasConflicts || !preview.resulting_students) {
      return;
    }
    onImportConfirmed(
      preview.resulting_students.map((student) => ({
        id: student.student_id || student.name || "",
        name: student.name || student.student_id || "",
        gender: student.gender,
        heightCm: student.height_cm,
        score: student.score,
        vision: student.vision,
        tags: student.tags,
        needs: student.needs,
        notes: student.notes,
        attributes: student.attributes,
      })),
    );
    reset();
  }

  function reset() {
    setPhase("idle");
    setIsPreviewing(false);
    setDraft(null);
    setPreview(null);
    setError(null);
    setMapping({});
    setSelectedFile(null);
    setMode("incremental");
    if (inputRef.current) {
      inputRef.current.value = "";
    }
  }

  const columnLabel = (index: number, header: string): string =>
    t("roster.column", { index: index + 1, header });

  const actionSummary = useMemo(() => {
    if (!preview) {
      return null;
    }
    const counts = preview.action_counts;
    return [
      { key: "add", value: counts.add ?? 0, label: t("roster.actionAdd", { count: counts.add ?? 0 }) },
      { key: "update", value: counts.update ?? 0, label: t("roster.actionUpdate", { count: counts.update ?? 0 }) },
      { key: "unchanged", value: counts.unchanged ?? 0, label: t("roster.actionUnchanged", { count: counts.unchanged ?? 0 }) },
      { key: "remove", value: counts.remove ?? 0, label: t("roster.actionRemove", { count: counts.remove ?? 0 }) },
      { key: "conflict", value: counts.conflict ?? 0, label: t("roster.actionConflict", { count: counts.conflict ?? 0 }) },
    ].filter((item) => item.value > 0);
  }, [preview, t]);

  const hasConflicts = (preview?.action_counts.conflict ?? 0) > 0 || (preview?.conflicts.length ?? 0) > 0;
  const canConfirm = Boolean(
    preview?.can_apply && preview.resulting_students && !hasConflicts,
  );

  const currentStep = preview
    ? 3
    : phase === "mapping" || phase === "previewing"
      ? 2
      : 1;

  return (
    <div className="roster-import-panel">
      <div className="roster-import-heading">
        <div>
          <span className="eyebrow">{t("roster.importEyebrow")}</span>
          <h3>{t("roster.importTitle")}</h3>
          <p>{t("roster.importHint")}</p>
        </div>
        <ol className="roster-import-steps" aria-label={t("roster.importSteps")}>
          {["roster.stepChoose", "roster.stepMap", "roster.stepConfirm"].map(
            (key, index) => {
              const step = index + 1;
              const complete = step < currentStep;
              const active = step === currentStep;
              return (
                <li
                  key={key}
                  className={active ? "active" : complete ? "complete" : ""}
                >
                  <span>{complete ? "✓" : step}</span>
                  {t(key as Parameters<Translate>[0])}
                </li>
              );
            },
          )}
        </ol>
      </div>

      <div className="file-picker">
        <span className="file-picker-icon" aria-hidden="true">
          ↑
        </span>
        <strong>{selectedFile ? t("roster.changeFile") : t("roster.replace")}</strong>
        <small>
          {selectedFile
            ? t("roster.selectedFile", { name: selectedFile })
            : t("roster.fileHint")}
        </small>
        <div className="file-picker-actions">
          {desktopReady ? (
            <button
              type="button"
              className="primary-button"
              onClick={handleDesktopFile}
              disabled={desktopOpening || phase === "uploading" || phase === "previewing"}
            >
              {desktopOpening ? t("roster.desktopOpening") : t("roster.desktopOpen")}
            </button>
          ) : null}
          <label className="file-picker-browser-button">
            <span>{desktopReady ? t("roster.browserOpen") : t("roster.replace")}</span>
            <input
              ref={inputRef}
              type="file"
              accept=".csv,.xlsx,.xls"
              onChange={handleFileChange}
              disabled={phase === "uploading" || phase === "previewing"}
            />
          </label>
        </div>
      </div>

      {phase === "uploading" ? (
        <p className="roster-status" aria-live="polite">
          {t("roster.uploading")}
        </p>
      ) : null}

      {phase === "error" && error ? (
        <p className="roster-error" role="alert">
          {t("roster.uploadFailed", { message: error })}
        </p>
      ) : null}

      {phase === "mapping" && draft ? (
        <div className="roster-mapping-section">
          <h3>{t("roster.previewTitle")}</h3>
          <p className="muted">{t("roster.previewHint")}</p>

          {draft.headerless ? (
            <div className="roster-import-note" role="status">
              <strong>{t("roster.headerlessTitle")}</strong>
              <span>{t("roster.headerlessHint")}</span>
            </div>
          ) : null}

          <div className="roster-preview-table" role="table">
            <div className="roster-preview-head" role="row">
              {draft.columns.map((col) => (
                <span key={col.index} role="columnheader">
                  {columnLabel(col.index, col.header)}
                </span>
              ))}
            </div>
            {draft.preview_rows.map((row) => (
              <div key={row.row_number} role="row" className="roster-preview-row">
                {row.cells.map((cell, idx) => (
                  <span key={idx} role="cell">
                    {cell === null ? "" : String(cell)}
                  </span>
                ))}
              </div>
            ))}
          </div>

          <fieldset className="mapping-fieldset">
            <legend>{t("roster.mapping")}</legend>
            <p className="mapping-help">{t("roster.mappingHint")}</p>
            {draft.columns.map((col) => (
              <label key={col.index} className="mapping-row">
                <span>{columnLabel(col.index, col.header)}</span>
                <select
                  value={mapping[col.index] ?? ""}
                  onChange={(event) =>
                    handleMappingChange(
                      col.index,
                      (event.target.value || null) as RosterFieldName | null,
                    )
                  }
                >
                  <option value="">{t("roster.notMapped")}</option>
                  {FIELD_OPTIONS.map((field) => (
                    <option key={field} value={field}>
                      {fieldLabel(field, t)}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </fieldset>

          {draft.mapping_issues.length > 0 ? (
            <ul className="mapping-issues" role="alert">
              {draft.mapping_issues.map((issue, idx) => (
                <li key={idx}>
                  {mappingIssueMessage(issue, t)}
                  {issue.column_indices.length > 0 ? (
                    <small>
                      {t("roster.columnsToCheck", {
                        columns: issue.column_indices.map((index) => index + 1).join(", "),
                      })}
                    </small>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : null}

          <fieldset className="mode-fieldset">
            <legend>{t("roster.mode")}</legend>
            <label className={mode === "incremental" ? "selected" : ""}>
              <input
                type="radio"
                name="roster-mode"
                checked={mode === "incremental"}
                onChange={() => {
                  setMode("incremental");
                  setPreview(null);
                  setError(null);
                }}
              />
              <span>
                <strong>{t("roster.incremental")}</strong>
                <small>{t("roster.incrementalHint")}</small>
              </span>
            </label>
            <label className={mode === "replace" ? "selected" : ""}>
              <input
                type="radio"
                name="roster-mode"
                checked={mode === "replace"}
                onChange={() => {
                  setMode("replace");
                  setPreview(null);
                  setError(null);
                }}
              />
              <span>
                <strong>{t("roster.overwrite")}</strong>
                <small>{t("roster.overwriteHint")}</small>
              </span>
            </label>
          </fieldset>

          <button
            type="button"
            className="primary-button roster-preview-button"
            onClick={handlePreview}
            disabled={isPreviewing}
          >
            {isPreviewing ? t("roster.previewing") : t("roster.previewAction")}
          </button>

          {isPreviewing ? (
            <p className="roster-status" aria-live="polite">
              {t("roster.previewing")}
            </p>
          ) : null}

          {error && !isPreviewing ? (
            <p className="roster-error" role="alert">
              {t("roster.previewFailed", { message: error })}
            </p>
          ) : null}

          {preview ? (
            <div className="preview-result">
              <div className="action-summary" aria-live="polite">
                {actionSummary?.map((item) => (
                  <span key={item.key} className="action-badge">
                    {item.label}
                  </span>
                ))}
              </div>

              <p className={!canConfirm ? "preview-warn" : "preview-ok"}>
                {hasConflicts
                  ? t("roster.hasConflicts")
                  : canConfirm
                    ? t("roster.canApply")
                    : t("roster.previewIncomplete")}
              </p>

              {preview.conflicts.length > 0 ? (
                <details className="conflict-list">
                  <summary>{t("roster.conflicts")}</summary>
                  <ul>
                    {preview.conflicts.map((conflict, idx) => (
                      <li key={idx}>
                        {conflictMessage(conflict, t)}
                        {conflict.incoming_index !== null ? (
                          <small>
                            {t("roster.rowToCheck", {
                              row: conflict.incoming_index + 2,
                            })}
                          </small>
                        ) : null}
                      </li>
                    ))}
                  </ul>
                </details>
              ) : (
                <p className="muted">{t("roster.noConflicts")}</p>
              )}

            </div>
          ) : null}

          <div className="roster-confirm-card">
            <div>
              <strong>{t("roster.confirmTitle")}</strong>
              <p className="preview-confirm-hint">
                {canConfirm ? t("roster.confirmHint") : t("roster.confirmPendingHint")}
              </p>
            </div>
            <div className="preview-actions">
              <button type="button" className="text-button" onClick={reset}>
                {t("roster.cancel")}
              </button>
              <button
                type="button"
                className="primary-button"
                onClick={handleConfirm}
                disabled={!canConfirm}
              >
                {t("roster.confirm")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

export { demoStudents as rosterDemoStudents };
