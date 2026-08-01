import type { ReactNode } from "react";

import type {
  AdvancedSolveSettings,
  CatalogOption,
  CommonConstraint,
  CommonPreferenceId,
  CustomRoomSettings,
  RotationPlan,
  RotationSettings,
  RoomTemplate,
  SeatAssignment,
  Student,
} from "../api/types";
import type { WorkflowStep } from "../domain/workflow";
import type { Locale, MessageKey, Translate } from "../i18n/messages";
import { LayoutEditorPanel } from "./LayoutEditorPanel";
import { RotationPlanSummary } from "./RotationPlanSummary";

const PREFERENCE_OPTIONS: Array<{
  id: CommonPreferenceId;
  label: MessageKey;
  description: MessageKey;
}> = [
  { id: "vision_front", label: "preference.visionFront", description: "preference.visionFrontHint" },
  { id: "height_back", label: "preference.heightBack", description: "preference.heightBackHint" },
  { id: "fair_rotation", label: "preference.fairRotation", description: "preference.fairRotationHint" },
  { id: "avoid_recent_neighbors", label: "preference.avoidNeighbors", description: "preference.avoidNeighborsHint" },
  { id: "score_position", label: "preference.scorePosition", description: "preference.scorePositionHint" },
  { id: "score_distribution", label: "preference.scoreDistribution", description: "preference.scoreDistributionHint" },
  { id: "mentor_pairing", label: "preference.mentorPairing", description: "preference.mentorPairingHint" },
];

type WorkflowPanelProps = {
  step: WorkflowStep;
  locale: Locale;
  t: Translate;
  studentCount: number;
  rosterValid: boolean;
  students: Student[];
  seatIds: string[];
  selectedFileName: string | null;
  rooms: RoomTemplate[];
  selectedRoomId: string;
  goals: CatalogOption[];
  selectedGoalId: string;
  exportFormats: CatalogOption[];
  selectedExportFormat: string;
  orientation: "portrait" | "landscape";
  showStudentIds: boolean;
  advancedSettings: AdvancedSolveSettings;
  rotationSettings: RotationSettings;
  rotationPlan: RotationPlan | null;
  activeRotationPeriod: number;
  roomSettings: CustomRoomSettings;
  constraints: CommonConstraint[];
  preferences: CommonPreferenceId[];
  error: string | null;
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
  onAdvancedSettingsChange: (
    changes: Partial<AdvancedSolveSettings>,
  ) => void;
  onRotationSettingsChange: (changes: Partial<RotationSettings>) => void;
  onRotationPeriodSelect: (period: number) => void;
  onRoomSettingsChange: (changes: Partial<CustomRoomSettings>) => void;
  onConstraintAdd: () => void;
  onConstraintChange: (
    id: string,
    changes: Partial<CommonConstraint>,
  ) => void;
  onConstraintRemove: (id: string) => void;
  onPreferenceToggle: (id: CommonPreferenceId) => void;
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
  rosterValid,
  students,
  seatIds,
  selectedFileName,
  rooms,
  selectedRoomId,
  goals,
  selectedGoalId,
  exportFormats,
  selectedExportFormat,
  orientation,
  showStudentIds,
  advancedSettings,
  rotationSettings,
  rotationPlan,
  activeRotationPeriod,
  roomSettings,
  constraints,
  preferences,
  error,
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
  onAdvancedSettingsChange,
  onRotationSettingsChange,
  onRotationPeriodSelect,
  onRoomSettingsChange,
  onConstraintAdd,
  onConstraintChange,
  onConstraintRemove,
  onPreferenceToggle,
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
          <div className="room-settings">
            <fieldset className="choice-list">
              <legend className="sr-only">{t("step.room.title")}</legend>
              {rooms.map((room) => (
                <label
                  className="choice-card room-choice"
                  data-selected={!roomSettings.enabled && room.id === selectedRoomId}
                  key={room.id}
                >
                  <input
                    type="radio"
                    name="room"
                    value={room.id}
                    checked={!roomSettings.enabled && room.id === selectedRoomId}
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
                    <em>{t("room.seats", { count: room.rows * room.columns })}</em>
                  </span>
                  <span className="choice-check" aria-hidden="true">✓</span>
                </label>
              ))}
            </fieldset>
            <section className="custom-room-card" aria-labelledby="custom-room-title">
              <label className="custom-room-toggle">
                <input
                  data-testid="custom-room-toggle"
                  type="checkbox"
                  checked={roomSettings.enabled}
                  onChange={(event) =>
                    onRoomSettingsChange({ enabled: event.target.checked })
                  }
                />
                <span>
                  <strong id="custom-room-title">{t("room.customTitle")}</strong>
                  <small>{t("room.customHint")}</small>
                </span>
              </label>
              {roomSettings.enabled ? (
                <div className="custom-room-fields">
                  <label className="advanced-field">
                    <span>{t("room.rows")}</span>
                    <input
                      type="number"
                      min={1}
                      max={30}
                      value={roomSettings.rows}
                      onChange={(event) =>
                        onRoomSettingsChange({
                          rows: Math.min(30, Math.max(1, Number(event.target.value) || 1)),
                        })
                      }
                    />
                  </label>
                  <label className="advanced-field">
                    <span>{t("room.columns")}</span>
                    <input
                      type="number"
                      min={1}
                      max={30}
                      value={roomSettings.columns}
                      onChange={(event) =>
                        onRoomSettingsChange({
                          columns: Math.min(30, Math.max(1, Number(event.target.value) || 1)),
                        })
                      }
                    />
                  </label>
                  <label className="advanced-field advanced-field-wide">
                    <span>{t("room.aisles")}</span>
                    <input
                      value={roomSettings.aisleColumns}
                      placeholder={t("room.aislesPlaceholder")}
                      onChange={(event) =>
                        onRoomSettingsChange({ aisleColumns: event.target.value })
                      }
                    />
                    <small>{t("room.aislesHint")}</small>
                  </label>
                  <label className="advanced-field advanced-field-wide">
                    <span>{t("room.disabledSeats")}</span>
                    <input
                      value={roomSettings.disabledSeats}
                      placeholder={t("room.disabledSeatsPlaceholder")}
                      onChange={(event) =>
                        onRoomSettingsChange({ disabledSeats: event.target.value })
                      }
                    />
                    <small>{t("room.disabledSeatsHint")}</small>
                  </label>
                  <label className="advanced-field advanced-field-wide">
                    <span>{t("room.layoutJson")}</span>
                    <textarea
                      rows={4}
                      value={roomSettings.layoutJson}
                      placeholder={t("room.layoutJsonPlaceholder")}
                      onChange={(event) =>
                        onRoomSettingsChange({ layoutJson: event.target.value })
                      }
                    />
                    <small>{t("room.layoutJsonHint")}</small>
                  </label>
                  <LayoutEditorPanel
                    roomSettings={roomSettings}
                    t={t}
                    onRoomSettingsChange={onRoomSettingsChange}
                  />
                </div>
              ) : null}
            </section>
          </div>
        ) : null}

        {step === "goal" ? (
          <div className="goal-settings">
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
                  <span className="choice-check" aria-hidden="true">✓</span>
                </label>
              ))}
            </fieldset>
            <fieldset className="preference-list">
              <legend>{t("preference.title")}</legend>
              <p className="muted">{t("preference.hint")}</p>
              {PREFERENCE_OPTIONS.map((option) => (
                <label key={option.id} className="preference-row">
                  <input
                    type="checkbox"
                    checked={preferences.includes(option.id)}
                    onChange={() => onPreferenceToggle(option.id)}
                  />
                  <span>
                    <strong>{t(option.label)}</strong>
                    <small>{t(option.description)}</small>
                  </span>
                </label>
              ))}
            </fieldset>
            <section className="constraints-card" aria-labelledby="constraints-title">
              <div className="constraints-heading">
                <div>
                  <h2 id="constraints-title">{t("constraints.title")}</h2>
                  <p>{t("constraints.hint")}</p>
                </div>
                <button
                  className="secondary-button"
                  type="button"
                  onClick={onConstraintAdd}
                  disabled={students.length < 1}
                >
                  {t("constraints.add")}
                </button>
              </div>
              {constraints.length === 0 ? (
                <p className="muted">{t("constraints.empty")}</p>
              ) : (
                <div className="constraint-list">
                  {constraints.map((constraint) => (
                    <div className="constraint-row" key={constraint.id}>
                      <select
                        aria-label={t("constraints.type")}
                        value={constraint.kind}
                        onChange={(event) =>
                          onConstraintChange(constraint.id, {
                            kind: event.target.value as CommonConstraint["kind"],
                          })
                        }
                      >
                        <option value="avoid_adjacent">{t("constraints.avoidAdjacent")}</option>
                        <option value="must_adjacent">{t("constraints.mustAdjacent")}</option>
                        <option value="fixed_seat">{t("constraints.fixedSeat")}</option>
                        <option value="min_distance">{t("constraints.minDistance")}</option>
                      </select>
                      <select
                        aria-label={t("constraints.student")}
                        value={constraint.first}
                        onChange={(event) =>
                          onConstraintChange(constraint.id, { first: event.target.value })
                        }
                      >
                        <option value="">{t("constraints.chooseStudent")}</option>
                        {students.map((student) => (
                          <option key={student.id} value={student.id}>
                            {student.name} · {student.id}
                          </option>
                        ))}
                      </select>
                      {constraint.kind === "fixed_seat" ? (
                        <input
                          aria-label={t("constraints.seat")}
                          list="available-seat-ids"
                          value={constraint.seatId}
                          placeholder={t("constraints.seatPlaceholder")}
                          onChange={(event) =>
                            onConstraintChange(constraint.id, { seatId: event.target.value })
                          }
                        />
                      ) : (
                        <select
                          aria-label={t("constraints.otherStudent")}
                          value={constraint.second}
                          onChange={(event) =>
                            onConstraintChange(constraint.id, { second: event.target.value })
                          }
                        >
                          <option value="">{t("constraints.chooseStudent")}</option>
                          {students.map((student) => (
                            <option key={student.id} value={student.id}>
                              {student.name} · {student.id}
                            </option>
                          ))}
                        </select>
                      )}
                      {constraint.kind === "min_distance" ? (
                        <>
                          <label className="constraint-detail-field">
                            <span className="sr-only">{t("constraints.distance")}</span>
                            <input
                              type="number"
                              min={0.1}
                              step={0.1}
                              aria-label={t("constraints.distance")}
                              value={constraint.distance}
                              onChange={(event) =>
                                onConstraintChange(constraint.id, {
                                  distance: Math.max(0.1, Number(event.target.value) || 0.1),
                                })
                              }
                            />
                          </label>
                          <select
                            aria-label={t("constraints.metric")}
                            value={constraint.metric}
                            onChange={(event) =>
                              onConstraintChange(constraint.id, {
                                metric: event.target.value as CommonConstraint["metric"],
                              })
                            }
                          >
                            <option value="graph">{t("constraints.metricGraph")}</option>
                            <option value="euclidean">{t("constraints.metricEuclidean")}</option>
                          </select>
                        </>
                      ) : null}
                      <button
                        className="icon-button"
                        type="button"
                        aria-label={t("constraints.remove")}
                        onClick={() => onConstraintRemove(constraint.id)}
                      >
                        ×
                      </button>
                    </div>
                  ))}
                </div>
              )}
              <datalist id="available-seat-ids">
                {seatIds.map((seatId) => <option key={seatId} value={seatId} />)}
              </datalist>
            </section>
          </div>
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
                  {roomSettings.enabled
                    ? t("generate.customLayout")
                    : selectedRoom
                    ? optionName(selectedRoom, locale)
                    : t("room.current")}
                </dd>
              </div>
              <div>
                <dt>{t("generate.goal")}</dt>
                <dd>
                  {advancedSettings.customRulesJson.trim()
                    ? t("generate.customRules")
                    : selectedGoal
                    ? optionName(selectedGoal, locale)
                    : t("goal.current")}
                </dd>
              </div>
            </dl>
            <p className="reassurance-note">
              <span aria-hidden="true">✓</span>
              {t("generate.note")}
            </p>
            {!rosterValid ? (
              <p className="inline-error" role="alert">
                {t("generate.rosterInvalid")}
              </p>
            ) : null}
            <details className="advanced-settings">
              <summary>{t("generate.advanced")}</summary>
              <p className="advanced-settings-hint">
                {t("generate.advancedHint")}
              </p>
              <div className="advanced-fields">
                <label className="advanced-field">
                  <span>{t("generate.candidateCount")}</span>
                  <input
                    type="number"
                    min={1}
                    max={20}
                    value={advancedSettings.candidateCount}
                    onChange={(event) =>
                      onAdvancedSettingsChange({
                        candidateCount: Math.min(
                          20,
                          Math.max(1, Number(event.target.value) || 1),
                        ),
                      })
                    }
                  />
                </label>
                <label className="advanced-field">
                  <span>{t("generate.timeLimit")}</span>
                  <input
                    type="number"
                    min={0.1}
                    max={300}
                    step={0.1}
                    value={advancedSettings.timeLimitSeconds}
                    onChange={(event) =>
                      onAdvancedSettingsChange({
                        timeLimitSeconds: Math.min(
                          300,
                          Math.max(0.1, Number(event.target.value) || 0.1),
                        ),
                      })
                    }
                  />
                </label>
                <label className="advanced-field">
                  <span>{t("generate.backend")}</span>
                  <select
                    value={advancedSettings.backend}
                    onChange={(event) =>
                      onAdvancedSettingsChange({
                        backend: event.target.value as AdvancedSolveSettings["backend"],
                      })
                    }
                  >
                    <option value="auto">{t("generate.backendAuto")}</option>
                    <option value="fallback">
                      {t("generate.backendFallback")}
                    </option>
                    <option value="ortools">
                      {t("generate.backendOrtools")}
                    </option>
                    <option value="native">
                      {t("generate.backendNative")}
                    </option>
                  </select>
                </label>
                <label className="advanced-field">
                  <span>{t("generate.seed")}</span>
                  <input
                    type="number"
                    inputMode="numeric"
                    value={advancedSettings.seed}
                    placeholder={t("generate.seedPlaceholder")}
                    onChange={(event) =>
                      onAdvancedSettingsChange({ seed: event.target.value })
                    }
                  />
                </label>
                <label className="advanced-field advanced-field-wide">
                  <span>{t("generate.customRules")}</span>
                  <textarea
                    rows={5}
                    value={advancedSettings.customRulesJson}
                    placeholder={t("generate.customRulesPlaceholder")}
                    onChange={(event) =>
                      onAdvancedSettingsChange({
                        customRulesJson: event.target.value,
                      })
                    }
                  />
                  <small>{t("generate.customRulesHint")}</small>
                </label>
              </div>
            </details>
            <details className="rotation-settings" open={rotationSettings.enabled}>
              <summary>{t("rotation.title")}</summary>
              <p className="advanced-settings-hint">{t("rotation.hint")}</p>
              <label className="rotation-toggle">
                <input
                  data-testid="rotation-toggle"
                  type="checkbox"
                  checked={rotationSettings.enabled}
                  onChange={(event) =>
                    onRotationSettingsChange({ enabled: event.target.checked })
                  }
                />
                <span>{t("rotation.enabled")}</span>
              </label>
              {rotationSettings.enabled ? (
                <div className="rotation-fields">
                  <label className="advanced-field">
                    <span>{t("rotation.periodCount")}</span>
                    <input
                      data-testid="rotation-period-count"
                      type="number"
                      min={1}
                      max={20}
                      value={rotationSettings.periodCount}
                      onChange={(event) =>
                        onRotationSettingsChange({
                          periodCount: Math.min(
                            20,
                            Math.max(1, Number(event.target.value) || 1),
                          ),
                        })
                      }
                    />
                  </label>
                  <label className="advanced-field advanced-field-wide">
                    <span>{t("rotation.periodLabels")}</span>
                    <input
                      data-testid="rotation-period-labels"
                      type="text"
                      value={rotationSettings.periodLabels}
                      placeholder={t("rotation.periodLabelsPlaceholder")}
                      onChange={(event) =>
                        onRotationSettingsChange({
                          periodLabels: event.target.value,
                        })
                      }
                    />
                    <small>{t("rotation.periodLabelsHint")}</small>
                  </label>
                </div>
              ) : null}
            </details>
            {error ? (
              <p className="inline-error" role="alert">
                {error}
              </p>
            ) : null}
          </div>
        ) : null}

        {step === "adjust" ? (
          <div className="adjust-tools">
            {rotationPlan ? (
              <RotationPlanSummary
                plan={rotationPlan}
                t={t}
                activePeriod={activeRotationPeriod}
                onPeriodSelect={onRotationPeriodSelect}
              />
            ) : null}
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
            disabled={isGenerating || !rosterValid}
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
