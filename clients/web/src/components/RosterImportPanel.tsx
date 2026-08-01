import { useCallback, useMemo, useRef, useState } from "react";

import { RosterApiError, previewRosterUpdate, uploadRosterDraft } from "../api/client";
import { demoStudents } from "../api/demo";
import type {
  RosterDraftResponse,
  RosterFieldName,
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
  const inputRef = useRef<HTMLInputElement>(null);

  const friendlyError = useCallback((err: unknown): string => {
    if (err instanceof RosterApiError) {
      return err.message;
    }
    if (err instanceof Error) {
      return err.message;
    }
    return String(err);
  }, []);

  async function handleFileChange(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }
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

  function handleMappingChange(columnIndex: number, field: RosterFieldName | null) {
    setMapping((prev) => ({ ...prev, [columnIndex]: field }));
  }

  async function handlePreview() {
    if (!draft) {
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
      setPhase("error");
      setIsPreviewing(false);
    }
  }

  function handleConfirm() {
    if (preview?.resulting_students) {
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
    } else if (preview && preview.can_apply) {
      // Fallback: if resulting students are not provided but it's safe,
      // derive from changes — but the API always provides resulting_students
      // when can_apply is true. If not, we cannot proceed safely.
      onImportConfirmed(currentStudents);
    }
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

  return (
    <div className="roster-import-panel">
      <label className="file-picker">
        <span className="file-picker-icon" aria-hidden="true">
          ↑
        </span>
        <strong>{t("roster.replace")}</strong>
        <small>
          {selectedFile
            ? t("roster.selectedFile", { name: selectedFile })
            : t("roster.fileHint")}
        </small>
        <input
          ref={inputRef}
          type="file"
          accept=".csv,.xlsx,.xls"
          onChange={handleFileChange}
          disabled={phase === "uploading" || phase === "previewing"}
        />
      </label>

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
                      {field}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </fieldset>

          {draft.mapping_issues.length > 0 ? (
            <ul className="mapping-issues" role="alert">
              {draft.mapping_issues.map((issue, idx) => (
                <li key={idx}>{issue.message}</li>
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
                onChange={() => setMode("incremental")}
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
                onChange={() => setMode("replace")}
              />
              <span>
                <strong>{t("roster.overwrite")}</strong>
                <small>{t("roster.overwriteHint")}</small>
              </span>
            </label>
          </fieldset>

          <button
            type="button"
            className="secondary-button"
            onClick={handlePreview}
            disabled={isPreviewing}
          >
            {isPreviewing ? t("roster.previewing") : t("action.preview")}
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

              <p className={hasConflicts ? "preview-warn" : "preview-ok"}>
                {hasConflicts ? t("roster.hasConflicts") : t("roster.canApply")}
              </p>

              {preview.conflicts.length > 0 ? (
                <details className="conflict-list">
                  <summary>{t("roster.conflicts")}</summary>
                  <ul>
                    {preview.conflicts.map((conflict, idx) => (
                      <li key={idx}>{conflict.message}</li>
                    ))}
                  </ul>
                </details>
              ) : (
                <p className="muted">{t("roster.noConflicts")}</p>
              )}

              <div className="preview-actions">
                <button
                  type="button"
                  className="text-button"
                  onClick={reset}
                >
                  {t("roster.cancel")}
                </button>
                <button
                  type="button"
                  className="primary-button"
                  onClick={handleConfirm}
                  disabled={!preview.can_apply || hasConflicts}
                >
                  {t("roster.confirm")}
                </button>
              </div>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

export { demoStudents as rosterDemoStudents };
