import type { ReactNode } from "react";

import type {
  CatalogOption,
  RoomTemplate,
  SeatAssignment,
} from "../api/types";
import type { WorkflowStep } from "../domain/workflow";
import type { Locale, Translate } from "../i18n/messages";

type WorkflowPanelProps = {
  step: WorkflowStep;
  locale: Locale;
  t: Translate;
  studentCount: number;
  selectedFileName: string | null;
  rooms: RoomTemplate[];
  selectedRoomId: string;
  goals: CatalogOption[];
  selectedGoalId: string;
  exportFormats: CatalogOption[];
  selectedExportFormat: string;
  orientation: "portrait" | "landscape";
  showStudentIds: boolean;
  selectedSeat: SeatAssignment | undefined;
  canUndo: boolean;
  isGenerating: boolean;
  rosterSlot?: ReactNode;
  onFileSelected: (name: string | null) => void;
  onRoomChange: (roomId: string) => void;
  onGoalChange: (goalId: string) => void;
  onExportFormatChange: (formatId: string) => void;
  onOrientationChange: (orientation: "portrait" | "landscape") => void;
  onShowStudentIdsChange: (show: boolean) => void;
  onBack: () => void;
  onNext: () => void;
  onGenerate: () => void;
  onUndo: () => void;
  onToggleLock: () => void;
  onPreview: () => void;
};

function optionName(
  option: { name: Record<Locale, string> },
  locale: Locale,
): string {
  return option.name[locale];
}

function optionDescription(
  option: { description: Record<Locale, string> },
  locale: Locale,
): string {
  return option.description[locale];
}

export function WorkflowPanel({
  step,
  locale,
  t,
  studentCount,
  selectedFileName,
  rooms,
  selectedRoomId,
  goals,
  selectedGoalId,
  exportFormats,
  selectedExportFormat,
  orientation,
  showStudentIds,
  selectedSeat,
  canUndo,
  isGenerating,
  rosterSlot,
  onFileSelected,
  onRoomChange,
  onGoalChange,
  onExportFormatChange,
  onOrientationChange,
  onShowStudentIdsChange,
  onBack,
  onNext,
  onGenerate,
  onUndo,
  onToggleLock,
  onPreview,
}: WorkflowPanelProps) {
  const selectedRoom =
    rooms.find((room) => room.id === selectedRoomId) ?? rooms[0];
  const selectedGoal =
    goals.find((goal) => goal.id === selectedGoalId) ?? goals[0];

  return (
    <section className="control-panel" aria-labelledby={`panel-title-${step}`}>
      <div className="panel-heading">
        <span className="eyebrow">{t(`step.${step}`)}</span>
        <h1 id={`panel-title-${step}`}>{t(`step.${step}.title`)}</h1>
        <p>{t(`step.${step}.subtitle`)}</p>
      </div>

      <div className="panel-content">
        {step === "roster" ? (
          rosterSlot ?? (
            <div className="roster-panel">
              <article className="current-roster">
                <div className="roster-avatar-stack" aria-hidden="true">
                  <span>林</span>
                  <span>陈</span>
                  <span>王</span>
                </div>
                <div>
                  <small>{t("roster.current")}</small>
                  <strong>{t("app.className")}</strong>
                  <p>{t("app.students", { count: studentCount })}</p>
                </div>
                <span className="ready-mark" aria-label={t("roster.ready")}>
                  ✓
                </span>
              </article>

              <label className="file-picker">
                <span className="file-picker-icon" aria-hidden="true">
                  ↑
                </span>
                <strong>{t("roster.replace")}</strong>
                <small>
                  {selectedFileName
                    ? t("roster.selectedFile", { name: selectedFileName })
                    : t("roster.fileHint")}
                </small>
                <input
                  type="file"
                  accept=".csv,.xlsx,.xls"
                  onChange={(event) =>
                    onFileSelected(event.target.files?.[0]?.name ?? null)
                  }
                />
              </label>
            </div>
          )
        ) : null}

        {step === "room" ? (
          <fieldset className="choice-list">
            <legend className="sr-only">{t("step.room.title")}</legend>
            {rooms.map((room) => (
              <label
                className="choice-card room-choice"
                data-selected={room.id === selectedRoomId}
                key={room.id}
              >
                <input
                  type="radio"
                  name="room"
                  value={room.id}
                  checked={room.id === selectedRoomId}
                  onChange={() => onRoomChange(room.id)}
                />
                <span
                  className="room-miniature"
                  style={{
                    gridTemplateColumns: `repeat(${Math.min(room.columns, 7)}, 1fr)`,
                  }}
                  aria-hidden="true"
                >
                  {Array.from({
                    length: Math.min(room.rows * room.columns, 21),
                  }).map((_, index) => (
                    <i key={index} />
                  ))}
                </span>
                <span className="choice-copy">
                  <strong>{optionName(room, locale)}</strong>
                  <small>{optionDescription(room, locale)}</small>
                  <em>
                    {t("room.seats", {
                      count: room.rows * room.columns,
                    })}
                  </em>
                </span>
                <span className="choice-check" aria-hidden="true">
                  ✓
                </span>
              </label>
            ))}
          </fieldset>
        ) : null}

        {step === "goal" ? (
          <fieldset className="choice-list">
            <legend className="sr-only">{t("step.goal.title")}</legend>
            {goals.map((goal, index) => (
              <label
                className="choice-card goal-choice"
                data-selected={goal.id === selectedGoalId}
                key={goal.id}
              >
                <input
                  type="radio"
                  name="goal"
                  value={goal.id}
                  checked={goal.id === selectedGoalId}
                  onChange={() => onGoalChange(goal.id)}
                />
                <span className={`goal-symbol goal-symbol-${index + 1}`} aria-hidden="true">
                  {index === 0 ? "↻" : index === 1 ? "⌁" : "◇"}
                </span>
                <span className="choice-copy">
                  <strong>{optionName(goal, locale)}</strong>
                  <small>{optionDescription(goal, locale)}</small>
                </span>
                <span className="choice-check" aria-hidden="true">
                  ✓
                </span>
              </label>
            ))}
          </fieldset>
        ) : null}

        {step === "generate" ? (
          <div className="generate-summary">
            <h2>{t("generate.summary")}</h2>
            <dl>
              <div>
                <dt>{t("generate.roster")}</dt>
                <dd>{t("app.students", { count: studentCount })}</dd>
              </div>
              <div>
                <dt>{t("generate.room")}</dt>
                <dd>
                  {selectedRoom
                    ? optionName(selectedRoom, locale)
                    : t("room.current")}
                </dd>
              </div>
              <div>
                <dt>{t("generate.goal")}</dt>
                <dd>
                  {selectedGoal
                    ? optionName(selectedGoal, locale)
                    : t("goal.current")}
                </dd>
              </div>
            </dl>
            <p className="reassurance-note">
              <span aria-hidden="true">✓</span>
              {t("generate.note")}
            </p>
          </div>
        ) : null}

        {step === "adjust" ? (
          <div className="adjust-tools">
            <div className="selection-status" aria-live="polite">
              <span
                className={selectedSeat ? "selection-dot active" : "selection-dot"}
                aria-hidden="true"
              />
              <p>
                {selectedSeat
                  ? selectedSeat.locked
                    ? t("adjust.lockedSelected", {
                        seat: selectedSeat.seatId,
                      })
                    : t("adjust.oneSelected", {
                        seat: selectedSeat.seatId,
                      })
                  : t("adjust.noneSelected")}
              </p>
            </div>
            <div className="adjust-actions">
              <button
                className="secondary-button"
                type="button"
                onClick={onUndo}
                disabled={!canUndo}
              >
                <span aria-hidden="true">↶</span>
                {t("action.undo")}
              </button>
              <button
                className="secondary-button"
                type="button"
                onClick={onToggleLock}
                disabled={!selectedSeat}
              >
                <span aria-hidden="true">{selectedSeat?.locked ? "○" : "●"}</span>
                {selectedSeat?.locked ? t("action.unlock") : t("action.lock")}
              </button>
            </div>
          </div>
        ) : null}

        {step === "export" ? (
          <div className="export-options">
            <fieldset>
              <legend>{t("export.use")}</legend>
              <div className="segmented-options">
                {exportFormats.map((format) => (
                  <label
                    data-selected={format.id === selectedExportFormat}
                    key={format.id}
                  >
                    <input
                      type="radio"
                      name="export-format"
                      checked={format.id === selectedExportFormat}
                      onChange={() => onExportFormatChange(format.id)}
                    />
                    <strong>{optionName(format, locale)}</strong>
                    <small>{optionDescription(format, locale)}</small>
                  </label>
                ))}
              </div>
            </fieldset>
            <fieldset>
              <legend>{t("export.privacy")}</legend>
              <div className="compact-options">
                <label>
                  <input
                    type="radio"
                    name="details"
                    checked={!showStudentIds}
                    onChange={() => onShowStudentIdsChange(false)}
                  />
                  {t("export.namesOnly")}
                </label>
                <label>
                  <input
                    type="radio"
                    name="details"
                    checked={showStudentIds}
                    onChange={() => onShowStudentIdsChange(true)}
                  />
                  {t("export.studentIds")}
                </label>
              </div>
            </fieldset>
            <fieldset>
              <legend>{t("export.orientation")}</legend>
              <div className="compact-options">
                {(["portrait", "landscape"] as const).map((value) => (
                  <label key={value}>
                    <input
                      type="radio"
                      name="orientation"
                      checked={orientation === value}
                      onChange={() => onOrientationChange(value)}
                    />
                    {t(`export.${value}`)}
                  </label>
                ))}
              </div>
            </fieldset>
          </div>
        ) : null}
      </div>

      <footer className="panel-actions">
        {step !== "roster" ? (
          <button className="text-button" type="button" onClick={onBack}>
            <span aria-hidden="true">←</span>
            {t("action.back")}
          </button>
        ) : (
          <span />
        )}
        {step === "generate" ? (
          <button
            className="primary-button"
            type="button"
            onClick={onGenerate}
            disabled={isGenerating}
          >
            {isGenerating ? t("action.generating") : t("action.generate")}
            <span aria-hidden="true">→</span>
          </button>
        ) : step === "export" ? (
          <button className="primary-button" type="button" onClick={onPreview}>
            {t("action.preview")}
            <span aria-hidden="true">↗</span>
          </button>
        ) : (
          <button className="primary-button" type="button" onClick={onNext}>
            {t("action.next")}
            <span aria-hidden="true">→</span>
          </button>
        )}
      </footer>
    </section>
  );
}

