import type { Translate } from "../i18n/messages";

type HistoryFilesCardProps = {
  fileNames: string[];
  snapshotCount: number;
  error: string | null;
  t: Translate;
  onChange: (files: File[]) => void;
  onClear: () => void;
};

/**
 * History snapshot upload card. The snapshots participate in fair-rotation
 * and avoid-recent-neighbor scoring; shared by the generate panel and the
 * history view so both see the same input (G-2).
 */
export function HistoryFilesCard({
  fileNames,
  snapshotCount,
  error,
  t,
  onChange,
  onClear,
}: HistoryFilesCardProps) {
  return (
    <fieldset className="advanced-field advanced-field-wide history-input-card">
      <legend>{t("generate.historyTitle")}</legend>
      <p className="advanced-settings-hint">{t("generate.historyHint")}</p>
      <div className="history-input-actions">
        <label className="file-input-button">
          <span>{t("generate.historyChoose")}</span>
          <input
            data-testid="history-json-files"
            type="file"
            accept=".json,application/json"
            multiple
            onChange={(event) => {
              const files = Array.from(event.currentTarget.files ?? []);
              event.currentTarget.value = "";
              onChange(files);
            }}
          />
        </label>
        {snapshotCount > 0 ? (
          <button type="button" className="text-button" onClick={onClear}>
            {t("generate.historyClear")}
          </button>
        ) : null}
      </div>
      {snapshotCount > 0 ? (
        <p className="history-loaded" role="status">
          {t("generate.historyLoaded", {
            count: snapshotCount,
            files: fileNames.join(", "),
          })}
        </p>
      ) : (
        <small>{t("generate.historyEmpty")}</small>
      )}
      {error ? (
        <p className="inline-error" role="alert">
          {error}
        </p>
      ) : null}
    </fieldset>
  );
}
