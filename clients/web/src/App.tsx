import { useEffect, useMemo, useState } from "react";

import {
  EDITOR_PROTOCOL_VERSION,
  RosterApiError,
  dispatchEditorCommand,
  exportDraft,
  fetchEditorState,
  generateClass,
  generateRotationPlan,
  loadBootstrap,
} from "./api/client";
import {
  createSeatAssignments,
  demoBootstrap,
  demoStudents,
} from "./api/demo";
import type {
  BootstrapData,
  CommonConstraint,
  CommonGroupRule,
  CommonPreferenceId,
  CustomRoomSettings,
  DetailedRuleSettings,
  EditorCommand,
  EditorOperation,
  EditorState,
  ProjectRotationLoadResponse,
  AdvancedSolveSettings,
  RotationPlan,
  RotationSettings,
  SeatAssignment,
  Student,
} from "./api/types";
import { AppHeader } from "./components/AppHeader";
import { DiagnosticsPanel } from "./components/DiagnosticsPanel";
import { ExportPreviewDialog } from "./components/ExportPreviewDialog";
import { ProjectWorkspacePanel } from "./components/ProjectWorkspacePanel";
import { RosterImportPanel } from "./components/RosterImportPanel";
import { SeatingCanvas } from "./components/SeatingCanvas";
import {
  rosterIsValid,
  StudentRosterEditor,
} from "./components/StudentRosterEditor";
import { StepNavigation } from "./components/StepNavigation";
import { UnseatedTray } from "./components/UnseatedTray";
import { WorkflowPanel } from "./components/WorkflowPanel";
import {
  deriveDiagnostics,
  getAdjacentStep,
  getUnseatedStudents,
  reconcileStudentAssignments,
  seatRemainingStudents,
  swapStudents,
  toggleSeatLock,
  type WorkflowStep,
} from "./domain/workflow";
import {
  buildGenerateClassRequest,
  buildGenerateRotationPlanRequest,
  InvalidAdvancedSettingError,
} from "./domain/generation";
import {
  createTranslator,
  type Locale,
} from "./i18n/messages";
import {
  applyTheme,
  getInitialTheme,
  type ThemeName,
} from "./theme/theme";

const LOCALE_STORAGE_KEY = "seattrellis-locale";

const DEFAULT_ADVANCED_SETTINGS: AdvancedSolveSettings = {
  candidateCount: 1,
  seed: "",
  timeLimitSeconds: 10,
  backend: "auto",
  customRulesJson: "",
};

const DEFAULT_ROOM_SETTINGS: CustomRoomSettings = {
  enabled: false,
  rows: 5,
  columns: 6,
  aisleColumns: "",
  disabledSeats: "",
  layoutJson: "",
};

const DEFAULT_ROTATION_SETTINGS: RotationSettings = {
  enabled: false,
  periodCount: 4,
  periodLabels: "",
};

const DEFAULT_DETAILED_RULE_SETTINGS: DetailedRuleSettings = {
  enabled: false,
  fairRotation: {
    enabled: true,
    weight: 10,
    lookback: 4,
  },
  avoidRecentNeighbors: {
    enabled: true,
    weight: 10,
    lookback: 4,
    maxRecentCount: 1,
    withinDistance: 2,
    relationTypes: ["desk_mate", "adjacent_any"],
  },
  cooling: {
    enabled: false,
    weight: 12,
    coolingPeriod: 3,
    withinDistance: 2,
    relationTypes: ["desk_mate", "adjacent_any"],
  },
  scorePosition: {
    enabled: true,
    weight: 18,
    direction: "high_front",
  },
  scoreDistribution: {
    enabled: true,
    weight: 18,
    scope: "row",
  },
  mentorPairing: {
    enabled: true,
    weight: 18,
    mentorPercentile: 0.75,
    learnerPercentile: 0.25,
    relation: "desk_mate",
    avoidRecentRepeats: true,
    historyLookback: 4,
  },
};

function getInitialLocale(): Locale {
  const stored = window.localStorage.getItem(LOCALE_STORAGE_KEY);
  if (stored === "zh-CN" || stored === "en") {
    return stored;
  }
  return window.navigator.language.toLowerCase().startsWith("zh")
    ? "zh-CN"
    : "en";
}

function newCommandId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function friendlyError(err: unknown): string {
  if (err instanceof RosterApiError) {
    return err.message;
  }
  if (err instanceof Error) {
    return err.message;
  }
  return String(err);
}

/** Convert the authoritative editor state into the canvas plan model. */
export function editorToPlan(editor: EditorState): {
  students: Student[];
  assignments: SeatAssignment[];
} {
  const students: Student[] = editor.students.map((student) => ({
    id: student.student_key,
    name: student.display_name,
  }));
  const nameById = new Map(
    editor.students.map((student) => [student.student_key, student.display_name]),
  );
  const assignments: SeatAssignment[] = editor.seats
    .filter((seat) => seat.enabled)
    .map((seat) => ({
      seatId: seat.seat_id,
      row: seat.row - 1,
      column: seat.col - 1,
      student: seat.student_key
        ? {
            id: seat.student_key,
            name: nameById.get(seat.student_key) ?? "",
          }
        : undefined,
      locked: seat.locked,
    }));
  return { students, assignments };
}

export function App() {
  const [locale, setLocale] = useState<Locale>(getInitialLocale);
  const [theme, setTheme] = useState<ThemeName>(getInitialTheme);
  const [connection, setConnection] = useState<
    "loading" | BootstrapData["source"]
  >("loading");
  const [catalogs, setCatalogs] = useState(demoBootstrap.catalogs);
  const [students, setStudents] = useState<Student[]>(demoStudents);
  const [revision, setRevision] = useState(0);
  const [step, setStep] = useState<WorkflowStep>("roster");
  const [selectedFileName, setSelectedFileName] = useState<string | null>(null);
  const [selectedRoomId, setSelectedRoomId] = useState("compact");
  const [selectedGoalId, setSelectedGoalId] =
    useState("daily-rotation");
  const [selectedExportFormat, setSelectedExportFormat] = useState("print");
  const [orientation, setOrientation] = useState<
    "portrait" | "landscape"
  >("landscape");
  const [showStudentIds, setShowStudentIds] = useState(false);
  const [advancedSettings, setAdvancedSettings] =
    useState<AdvancedSolveSettings>(DEFAULT_ADVANCED_SETTINGS);
  const [rotationSettings, setRotationSettings] = useState<RotationSettings>(
    DEFAULT_ROTATION_SETTINGS,
  );
  const [detailedRules, setDetailedRules] = useState<DetailedRuleSettings>(
    DEFAULT_DETAILED_RULE_SETTINGS,
  );
  const [rotationPlan, setRotationPlan] = useState<RotationPlan | null>(null);
  const [rotationEditors, setRotationEditors] = useState<EditorState[]>([]);
  const [activeRotationPeriod, setActiveRotationPeriod] = useState(1);
  const [roomSettings, setRoomSettings] =
    useState<CustomRoomSettings>(DEFAULT_ROOM_SETTINGS);
  const [preferences, setPreferences] = useState<CommonPreferenceId[]>([]);
  const [constraints, setConstraints] = useState<CommonConstraint[]>([]);
  const [groups, setGroups] = useState<CommonGroupRule[]>([]);
  const [assignments, setAssignments] = useState<SeatAssignment[]>(() =>
    createSeatAssignments(4, 5, demoStudents, 16),
  );
  const [selectedSeatId, setSelectedSeatId] = useState<string | null>(null);
  const [history, setHistory] = useState<SeatAssignment[][]>([]);
  const [isGenerating, setIsGenerating] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [editorDraftId, setEditorDraftId] = useState<string | null>(null);
  const [editorRevision, setEditorRevision] = useState(0);
  const [editorUndoDepth, setEditorUndoDepth] = useState(0);
  const t = useMemo(() => createTranslator(locale), [locale]);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.lang = locale;
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  }, [locale]);

  useEffect(() => {
    let current = true;
    void loadBootstrap().then((bootstrap) => {
      if (!current) {
        return;
      }
      setCatalogs(bootstrap.catalogs);
      setConnection(bootstrap.source);
      const firstRoom = bootstrap.catalogs.roomTemplates[0];
      const firstGoal = bootstrap.catalogs.teacherGoals[0];
      const firstFormat = bootstrap.catalogs.exportFormats[0];
      if (firstRoom) {
        setSelectedRoomId((value) =>
          bootstrap.catalogs.roomTemplates.some((room) => room.id === value)
            ? value
            : firstRoom.id,
        );
      }
      if (firstGoal) {
        setSelectedGoalId((value) =>
          bootstrap.catalogs.teacherGoals.some((goal) => goal.id === value)
            ? value
            : firstGoal.id,
        );
      }
      if (firstFormat) {
        setSelectedExportFormat((value) =>
          bootstrap.catalogs.exportFormats.some(
            (format) => format.id === value,
          )
            ? value
            : firstFormat.id,
        );
      }
    });
    return () => {
      current = false;
    };
  }, []);

  const unseatedStudents = useMemo(
    () => getUnseatedStudents(students, assignments),
    [students, assignments],
  );
  const diagnostics = useMemo(
    () => deriveDiagnostics(assignments, students, selectedSeatId),
    [assignments, students, selectedSeatId],
  );
  const selectedSeat = assignments.find(
    (seat) => seat.seatId === selectedSeatId,
  );

  function handleRoomChange(roomId: string) {
    const room = catalogs.roomTemplates.find((item) => item.id === roomId);
    if (!room) {
      return;
    }
    setSelectedRoomId(roomId);
    setRoomSettings((current) => ({ ...current, enabled: false }));
    setAssignments(
      createSeatAssignments(
        room.rows,
        room.columns,
        students,
        Math.min(Math.max(0, students.length - 2), room.rows * room.columns),
      ),
    );
    setSelectedSeatId(null);
    setHistory([]);
    setEditorDraftId(null);
    setEditorRevision(0);
    setEditorUndoDepth(0);
    setRotationPlan(null);
    setRotationEditors([]);
    setActiveRotationPeriod(1);
  }

  function handleRoomSettingsChange(changes: Partial<CustomRoomSettings>) {
    setRoomSettings((current) => ({ ...current, ...changes }));
    setSelectedSeatId(null);
    setEditorDraftId(null);
    setEditorRevision(0);
    setEditorUndoDepth(0);
    setRotationPlan(null);
    setRotationEditors([]);
    setActiveRotationPeriod(1);
  }

  function handleConstraintAdd() {
    setConstraints((current) => [
      ...current,
      {
        id: newCommandId(),
        kind: students.length >= 2 ? "avoid_adjacent" : "fixed_seat",
        first: students[0]?.id ?? "",
        second: students[1]?.id ?? students[0]?.id ?? "",
        seatId: "",
        distance: 2,
        metric: "graph",
      },
    ]);
  }

  function handleConstraintChange(
    id: string,
    changes: Partial<CommonConstraint>,
  ) {
    setConstraints((current) =>
      current.map((constraint) =>
        constraint.id === id ? { ...constraint, ...changes } : constraint,
      ),
    );
  }

  function handleConstraintRemove(id: string) {
    setConstraints((current) => current.filter((constraint) => constraint.id !== id));
  }

  function handleConstraintBatchAdd(next: CommonConstraint[]) {
    setConstraints((current) => [...current, ...next]);
  }

  function handleGroupAdd() {
    setGroups((current) => [
      ...current,
      {
        id: newCommandId(),
        name: locale === "zh-CN" ? `小组 ${current.length + 1}` : `Group ${current.length + 1}`,
        mode: "separate",
        students: students.slice(0, 2).map((student) => student.id),
      },
    ]);
  }

  function handleGroupChange(id: string, changes: Partial<CommonGroupRule>) {
    setGroups((current) =>
      current.map((group) => (group.id === id ? { ...group, ...changes } : group)),
    );
  }

  function handleGroupRemove(id: string) {
    setGroups((current) => current.filter((group) => group.id !== id));
  }

  function handlePreferenceToggle(id: CommonPreferenceId) {
    setPreferences((current) =>
      current.includes(id)
        ? current.filter((value) => value !== id)
        : [...current, id],
    );
  }

  function handleSeatActivate(seatId: string) {
    if (!selectedSeatId) {
      setSelectedSeatId(seatId);
      return;
    }
    if (seatId === selectedSeatId) {
      setSelectedSeatId(null);
      return;
    }

    const first = assignments.find((seat) => seat.seatId === selectedSeatId);
    const second = assignments.find((seat) => seat.seatId === seatId);
    if (!first || !second) {
      setSelectedSeatId(seatId);
      return;
    }

    if (editorDraftId) {
      void syncSwap(first, second);
      return;
    }

    const updated = swapStudents(assignments, selectedSeatId, seatId);
    if (updated !== assignments) {
      setHistory((previous) => [...previous, assignments]);
      setAssignments(updated);
      setSelectedSeatId(null);
    } else {
      setSelectedSeatId(seatId);
    }
  }

  async function syncSwap(
    first: SeatAssignment,
    second: SeatAssignment,
  ) {
    if (!editorDraftId || first.locked || second.locked) {
      setSelectedSeatId(null);
      return;
    }
    const operations: EditorOperation[] = [];
    if (first.student && second.student) {
      operations.push({
        kind: "swap_students",
        payload: {
          first_student: first.student.id,
          second_student: second.student.id,
        },
      });
    } else if (first.student && !second.student) {
      operations.push({
        kind: "move_student",
        payload: { student_key: first.student.id, seat_id: second.seatId },
      });
    } else if (!first.student && second.student) {
      operations.push({
        kind: "move_student",
        payload: { student_key: second.student.id, seat_id: first.seatId },
      });
    } else {
      setSelectedSeatId(null);
      return;
    }
    await applyEditorCommand({ action: "apply", operations });
    setSelectedSeatId(null);
  }

  async function applyEditorCommand(
    command: Pick<EditorCommand, "action" | "operations">,
  ) {
    if (!editorDraftId) {
      return;
    }
    setSaveError(null);
    try {
      const editor = await dispatchEditorCommand({
        kind: "seattrellis_editor_command",
        protocol_version: EDITOR_PROTOCOL_VERSION,
        command_id: newCommandId(),
        draft_id: editorDraftId,
        base_revision: editorRevision,
        action: command.action,
        operations: command.operations,
      });
      const plan = editorToPlan(editor);
      setAssignments(plan.assignments);
      setStudents(plan.students);
      setEditorRevision(editor.revision);
      setEditorUndoDepth(editor.undo_depth);
      setSelectedSeatId(null);
    } catch (err) {
      setSaveError(friendlyError(err));
      setSelectedSeatId(null);
    }
  }

  function handleUndo() {
    if (editorDraftId) {
      void applyEditorCommand({ action: "undo", operations: [] });
      return;
    }
    setHistory((previous) => {
      const latest = previous.at(-1);
      if (!latest) {
        return previous;
      }
      setAssignments(latest);
      setSelectedSeatId(null);
      return previous.slice(0, -1);
    });
  }

  function handleToggleLock() {
    if (!selectedSeatId) {
      return;
    }
    if (editorDraftId) {
      const seat = assignments.find((item) => item.seatId === selectedSeatId);
      if (!seat) {
        return;
      }
      void applyEditorCommand({
        action: "apply",
        operations: [
          {
            kind: seat.locked ? "unlock_seat" : "lock_seat",
            payload: { seat_id: seat.seatId },
          },
        ],
      });
      return;
    }
    setHistory((previous) => [...previous, assignments]);
    setAssignments((current) => toggleSeatLock(current, selectedSeatId));
  }

  function applyEditorState(editor: EditorState) {
    const plan = editorToPlan(editor);
    setStudents(plan.students);
    setAssignments(plan.assignments);
    setEditorDraftId(editor.draft_id);
    setEditorRevision(editor.revision);
    setEditorUndoDepth(editor.undo_depth);
    setSelectedSeatId(null);
  }

  async function handleRotationPeriodSelect(period: number) {
    const target = rotationEditors.find(
      (editor) => editor.candidate_id === `period-${period}`,
    );
    if (!target) {
      return;
    }
    if (target.draft_id === editorDraftId) {
      setActiveRotationPeriod(period);
      return;
    }
    setSaveError(null);
    try {
      const editor = await fetchEditorState(target.draft_id);
      applyEditorState(editor);
      setActiveRotationPeriod(period);
    } catch (err) {
      setSaveError(friendlyError(err));
    }
  }

  function handleRotationLoad(result: ProjectRotationLoadResponse) {
    const editors = result.period_editors.length
      ? result.period_editors
      : [result.editor];
    applyEditorState(editors[0]);
    setRotationEditors(editors);
    setRotationPlan(result.rotation_plan);
    setActiveRotationPeriod(1);
    setHistory([]);
    setSelectedSeatId(null);
    setSaveError(null);
    setStep("adjust");
  }

  async function handleGenerate() {
    setIsGenerating(true);
    setSaveError(null);
    const className = selectedFileName
      ? selectedFileName.replace(/\.[^.]+$/, "")
      : locale === "zh-CN"
        ? "我的班级"
        : "My class";
    try {
      const requestArgs = {
        className,
        students,
        selectedRoomId,
        selectedGoalId,
        settings: advancedSettings,
        roomSettings,
        constraints,
        groups,
        preferences,
        detailedRules,
      };
      const response = rotationSettings.enabled
        ? await generateRotationPlan(
            buildGenerateRotationPlanRequest({
              ...requestArgs,
              rotation: rotationSettings,
            }),
          )
        : await generateClass(buildGenerateClassRequest(requestArgs));
      const isRotation = "rotation_plan" in response;
      const periodEditors =
        isRotation && response.period_editors?.length
          ? response.period_editors
          : [response.editor];
      const editor = await fetchEditorState(periodEditors[0].draft_id);
      applyEditorState(editor);
      setRotationEditors(isRotation ? periodEditors : []);
      setActiveRotationPeriod(1);
      setRotationPlan(isRotation ? response.rotation_plan : null);
      setHistory([]);
      setSelectedSeatId(null);
      setStep("adjust");
    } catch (err) {
      if (err instanceof InvalidAdvancedSettingError) {
        setSaveError(
          err.kind === "seed"
            ? t("generate.seedInvalid")
            : err.kind === "rotation"
              ? t("rotation.labelsInvalid")
              : t("generate.jsonInvalid", {
                  field:
                    err.kind === "rules"
                      ? t("generate.customRules")
                      : t("room.invalid"),
                }),
        );
      } else {
        setSaveError(friendlyError(err));
      }
    } finally {
      setIsGenerating(false);
    }
  }

  async function handleSave(format: string) {
    if (!editorDraftId) {
      return;
    }
    setIsSaving(true);
    setSaveError(null);
    try {
      const { blob, filename } = await exportDraft({
        draft_id: editorDraftId,
        format,
        orientation,
        locale: locale === "zh-CN" ? "zh" : "en",
        show_student_ids: showStudentIds,
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = filename;
      document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
      setPreviewOpen(false);
    } catch (err) {
      setSaveError(friendlyError(err));
    } finally {
      setIsSaving(false);
    }
  }

  function handleRosterImported(importedStudents: Student[]) {
    setStudents(importedStudents);
    setRevision((prev) => prev + 1);
    setAssignments(
      createSeatAssignments(4, 5, importedStudents, importedStudents.length),
    );
    setHistory([]);
    setSelectedSeatId(null);
    setSelectedFileName(null);
    setGroups([]);
    setEditorDraftId(null);
    setEditorRevision(0);
    setEditorUndoDepth(0);
    setRotationPlan(null);
    setRotationEditors([]);
    setActiveRotationPeriod(1);
    setStep("room");
  }

  function handleStudentsEdited(editedStudents: Student[]) {
    setStudents(editedStudents);
    setRevision((prev) => prev + 1);
    setAssignments((current) =>
      reconcileStudentAssignments(current, editedStudents),
    );
    setHistory([]);
    setGroups((current) => {
      const validIds = new Set(editedStudents.map((student) => student.id));
      return current
        .map((group) => ({
          ...group,
          students: group.students.filter((student) => validIds.has(student)),
        }))
        .filter((group) => group.students.length > 0);
    });
    setSelectedSeatId(null);
    setEditorDraftId(null);
    setEditorRevision(0);
    setEditorUndoDepth(0);
    setRotationPlan(null);
    setRotationEditors([]);
    setActiveRotationPeriod(1);
  }

  return (
    <>
      <a className="skip-link" href="#main-workspace">
        {t("app.skip")}
      </a>
      <AppHeader
        locale={locale}
        theme={theme}
        studentCount={students.length}
        connection={connection}
        t={t}
        onLocaleChange={setLocale}
        onThemeChange={setTheme}
      />
      <div className="app-shell">
        <StepNavigation
          activeStep={step}
          t={t}
          onStepChange={setStep}
        />
        <main id="main-workspace" className="main-workspace" tabIndex={-1}>
          <WorkflowPanel
            step={step}
            locale={locale}
            t={t}
            studentCount={students.length}
            rosterValid={rosterIsValid(students)}
            students={students}
            seatIds={assignments.map((seat) => seat.seatId)}
            selectedFileName={selectedFileName}
            rooms={catalogs.roomTemplates}
            selectedRoomId={selectedRoomId}
            goals={catalogs.teacherGoals}
            selectedGoalId={selectedGoalId}
            exportFormats={catalogs.exportFormats}
            selectedExportFormat={selectedExportFormat}
            orientation={orientation}
            showStudentIds={showStudentIds}
            advancedSettings={advancedSettings}
            rotationSettings={rotationSettings}
            detailedRules={detailedRules}
            rotationPlan={rotationPlan}
            activeRotationPeriod={activeRotationPeriod}
            roomSettings={roomSettings}
            constraints={constraints}
            groups={groups}
            preferences={preferences}
            error={step === "generate" ? saveError : null}
            selectedSeat={selectedSeat}
            canUndo={
              editorDraftId ? editorUndoDepth > 0 : history.length > 0
            }
            isGenerating={isGenerating}
            rosterSlot={
              <div className="roster-workspace-stack">
                <RosterImportPanel
                  locale={locale}
                  t={t}
                  currentStudents={students}
                  currentRevision={revision}
                  onImportConfirmed={handleRosterImported}
                />
                <StudentRosterEditor
                  students={students}
                  t={t}
                  onChange={handleStudentsEdited}
                />
              </div>
            }
            onFileSelected={setSelectedFileName}
            onRoomChange={handleRoomChange}
            onGoalChange={setSelectedGoalId}
            onExportFormatChange={setSelectedExportFormat}
            onOrientationChange={setOrientation}
            onShowStudentIdsChange={setShowStudentIds}
            onAdvancedSettingsChange={(changes) =>
              setAdvancedSettings((current) => ({ ...current, ...changes }))
            }
            onRotationSettingsChange={(changes) =>
              setRotationSettings((current) => ({ ...current, ...changes }))
            }
            onDetailedRulesChange={(changes) =>
              setDetailedRules((current) => ({ ...current, ...changes }))
            }
            onRotationPeriodSelect={(period) => {
              void handleRotationPeriodSelect(period);
            }}
            onRoomSettingsChange={handleRoomSettingsChange}
            onConstraintAdd={handleConstraintAdd}
            onConstraintBatchAdd={handleConstraintBatchAdd}
            onConstraintChange={handleConstraintChange}
            onConstraintRemove={handleConstraintRemove}
            onGroupAdd={handleGroupAdd}
            onGroupChange={handleGroupChange}
            onGroupRemove={handleGroupRemove}
            onPreferenceToggle={handlePreferenceToggle}
            onBack={() => setStep((current) => getAdjacentStep(current, -1))}
            onNext={() => setStep((current) => getAdjacentStep(current, 1))}
            onGenerate={handleGenerate}
            onUndo={handleUndo}
            onToggleLock={handleToggleLock}
            onPreview={() => setPreviewOpen(true)}
          />

          <section className="canvas-card" aria-labelledby="canvas-card-title">
            <header>
              <div>
                <span className="eyebrow">{t("room.current")}</span>
                <h2 id="canvas-card-title">{t("canvas.title")}</h2>
              </div>
              <span className="seat-count">
                {t("room.seats", { count: assignments.length })}
              </span>
            </header>
            <SeatingCanvas
              assignments={assignments}
              selectedSeatId={selectedSeatId}
              t={t}
              onSeatActivate={handleSeatActivate}
            />
          </section>

          <aside className="workspace-side-rail">
            <UnseatedTray students={unseatedStudents} t={t} />
            <DiagnosticsPanel diagnostics={diagnostics} t={t} />
            <ProjectWorkspacePanel
              locale={locale}
              t={t}
              rotationPlan={rotationPlan}
              rotationDraftIds={rotationEditors.map((editor) => editor.draft_id)}
              onRotationLoad={handleRotationLoad}
            />
          </aside>
        </main>
      </div>
      <ExportPreviewDialog
        assignments={assignments}
        orientation={orientation}
        format={selectedExportFormat}
        open={previewOpen}
        isSaving={isSaving}
        error={saveError}
        t={t}
        onClose={() => setPreviewOpen(false)}
        onSave={handleSave}
      />
    </>
  );
}
