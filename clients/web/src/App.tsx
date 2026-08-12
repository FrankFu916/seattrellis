import { useEffect, useMemo, useState } from "react";

import {
  EDITOR_PROTOCOL_VERSION,
  RosterApiError,
  dispatchEditorCommand,
  exportDraft,
  fetchDraftAudit,
  fetchEditorState,
  generateClass,
  generateRotationPlan,
  listRecentProjects,
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
  HistorySnapshotPayload,
  ProjectRotationLoadResponse,
  AdvancedSolveSettings,
  RecentProject,
  DraftAuditReport,
  RotationPlan,
  RotationSettings,
  SeatAssignment,
  Student,
  ExportPrivacyOptions,
  ExportTemplate,
} from "./api/types";
import { AppHeader } from "./components/AppHeader";
import { ContextBar } from "./components/ContextBar";
import { DiagnosticsPanel } from "./components/DiagnosticsPanel";
import { ExportPreviewDialog } from "./components/ExportPreviewDialog";
import {
  FirstRunChecklist,
  type FirstRunProgress,
} from "./components/FirstRunChecklist";
import { HistoryRotationPanel } from "./components/HistoryRotationPanel";
import { RosterImportPanel } from "./components/RosterImportPanel";
import { SaveAsClassDialog } from "./components/SaveAsClassDialog";
import { CandidatesPanel, type CandidateMeta, type ReproInfo } from "./components/CandidatesPanel";
import { SeatingCanvasEditor } from "./components/SeatingCanvasEditor";
import { Sidebar } from "./components/Sidebar";
import { saveBlobWithDialog } from "./domain/desktop";
import {
  rosterIsValid,
  StudentRosterEditor,
} from "./components/StudentRosterEditor";
import { UnseatedTray } from "./components/UnseatedTray";
import { WorkflowPanel } from "./components/WorkflowPanel";
import {
  planBatchLock,
  planBatchMove,
} from "./domain/canvasEdit";
import {
  restoreSnapshotPlan,
} from "./components/HistoryRotationPanel";
import {
  contextActionFor,
  isContentView,
  viewToStep,
  type ClassContext,
  type ContextAction,
  type SessionClass,
  type WorkbenchView,
} from "./domain/navigation";
import {
  getUnseatedStudents,
  reconcileStudentAssignments,
  seatRemainingStudents,
  swapStudents,
  toggleSeatLock,
} from "./domain/workflow";
import {
  buildGenerateClassRequest,
  buildGenerateRotationPlanRequest,
  InvalidAdvancedSettingError,
} from "./domain/generation";
import {
  createTranslator,
  type Locale,
  type Translate,
} from "./i18n/messages";

const LOCALE_STORAGE_KEY = "seattrellis-locale";
/** First-run checklist dismissal ("用过即收", D1). */
const FIRST_RUN_KEY = "seattrellis-first-run:v1";

const DEFAULT_ADVANCED_SETTINGS: AdvancedSolveSettings = {
  // D4: the quick panel asks for the candidate count; 5 is the frozen
  // default pending G-4 dogfood evidence.
  candidateCount: 5,
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

const DEFAULT_EXPORT_PRIVACY: Record<ExportTemplate, ExportPrivacyOptions> = {
  public: {
    hide_scores: true,
    hide_notes: true,
    hide_special_needs: true,
    anonymize: false,
    show_height: false,
    show_vision: false,
  },
  teacher: {
    hide_scores: false,
    hide_notes: false,
    hide_special_needs: false,
    anonymize: false,
    show_height: true,
    show_vision: true,
  },
  report: {
    hide_scores: false,
    hide_notes: true,
    hide_special_needs: true,
    anonymize: false,
    show_height: false,
    show_vision: false,
  },
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

function friendlyError(err: unknown, t?: Translate): string {
  if (err instanceof RosterApiError) {
    const localizedKey: Record<string, Parameters<Translate>[0]> = {
      class_not_ready: "app.classNotReady",
      invalid_class_draft: "app.classNotReady",
      plan_not_found: "app.planNotFound",
      feature_unavailable: "app.featureUnavailable",
      session_required: "app.sessionExpired",
      editor_revision_conflict: "app.operationFailed",
      layout_revision_conflict: "app.operationFailed",
    };
    const key = localizedKey[err.code];
    return t && key ? t(key) : t ? t("app.operationFailed") : err.message;
  }
  if (err instanceof Error) {
    console.error("Workbench operation failed", err);
    return t ? t("app.operationFailed") : err.message;
  }
  return t ? t("app.operationFailed") : "The operation could not be completed.";
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

/**
 * Whether a keyboard event target is a form control. Global shortcuts
 * (Cmd/Ctrl+Z undo) must not swallow the platform's native text editing
 * while the teacher is typing in an input, textarea or select (C2).
 */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  return (
    target.contentEditable === "true" ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  );
}

export function App() {
  const [locale, setLocale] = useState<Locale>(getInitialLocale);
  const [connection, setConnection] = useState<
    "loading" | BootstrapData["source"]
  >("loading");
  const [catalogs, setCatalogs] = useState(demoBootstrap.catalogs);
  const [students, setStudents] = useState<Student[]>(demoStudents);
  const [revision, setRevision] = useState(0);
  const [view, setView] = useState<WorkbenchView>("roster");
  const [classContext, setClassContext] = useState<ClassContext>({
    kind: "temp",
  });
  const [sessionClasses, setSessionClasses] = useState<SessionClass[]>([]);
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [visitedViews, setVisitedViews] = useState<string[]>([]);
  const [generationDone, setGenerationDone] = useState(false);
  const [exportedOnce, setExportedOnce] = useState(false);
  const [firstRunDismissed, setFirstRunDismissed] = useState(
    () => window.localStorage.getItem(FIRST_RUN_KEY) === "done",
  );
  const [saveAsOpen, setSaveAsOpen] = useState(false);
  const [selectedFileName, setSelectedFileName] = useState<string | null>(null);
  const [selectedRoomId, setSelectedRoomId] = useState("compact");
  const [selectedGoalId, setSelectedGoalId] =
    useState("daily-rotation");
  const [selectedExportFormat, setSelectedExportFormat] = useState("print");
  const [orientation, setOrientation] = useState<
    "portrait" | "landscape"
  >("landscape");
  const [exportTemplate, setExportTemplate] =
    useState<ExportTemplate>("public");
  const [exportPrivacy, setExportPrivacy] = useState<ExportPrivacyOptions>(
    DEFAULT_EXPORT_PRIVACY.public,
  );
  const [pageScale, setPageScale] = useState(1);
  const [advancedSettings, setAdvancedSettings] =
    useState<AdvancedSolveSettings>(DEFAULT_ADVANCED_SETTINGS);
  const [historySnapshots, setHistorySnapshots] = useState<HistorySnapshotPayload[]>([]);
  const [historyFileNames, setHistoryFileNames] = useState<string[]>([]);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [rotationSettings, setRotationSettings] = useState<RotationSettings>(
    DEFAULT_ROTATION_SETTINGS,
  );
  const [detailedRules, setDetailedRules] = useState<DetailedRuleSettings>(
    DEFAULT_DETAILED_RULE_SETTINGS,
  );
  const [rotationPlan, setRotationPlan] = useState<RotationPlan | null>(null);
  const [candidateMetas, setCandidateMetas] = useState<CandidateMeta[]>([]);
  const [draftAudit, setDraftAudit] = useState<DraftAuditReport | null>(null);
  const [diagnosticFocus, setDiagnosticFocus] = useState<string | null>(null);
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
  const [isDirty, setIsDirty] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [editorDraftId, setEditorDraftId] = useState<string | null>(null);
  const [editorRevision, setEditorRevision] = useState(0);
  const [editorUndoDepth, setEditorUndoDepth] = useState(0);
  const [editorRedoDepth, setEditorRedoDepth] = useState(0);
  const t = useMemo(() => createTranslator(locale), [locale]);

  useEffect(() => {
    document.documentElement.lang = locale;
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  }, [locale]);

  useEffect(() => {
    function warnBeforeUnload(event: BeforeUnloadEvent): void {
      if (!isDirty) {
        return;
      }
      event.preventDefault();
      event.returnValue = t("app.unsavedChanges");
    }

    window.addEventListener("beforeunload", warnBeforeUnload);
    return () => window.removeEventListener("beforeunload", warnBeforeUnload);
  }, [isDirty, t]);

  // Re-audit the current draft whenever it changes (D6): the diagnostics
  // panel and the canvas badges consume the same Rust report.
  useEffect(() => {
    let current = true;
    if (!editorDraftId) {
      setDraftAudit(null);
      setDiagnosticFocus(null);
      return;
    }
    void fetchDraftAudit(editorDraftId)
      .then((report) => {
        if (current) {
          setDraftAudit(report);
        }
      })
      .catch(() => {
        if (current) {
          setDraftAudit(null);
        }
      });
    return () => {
      current = false;
    };
  }, [editorDraftId, editorRevision]);

  // Cmd/Ctrl+Z undo, Cmd/Ctrl+Shift+Z and Cmd/Ctrl+Y redo (design §6).
  // The combo is ignored while typing in a form control so the platform's
  // native text undo keeps working inside editors (C2 platform adaptation).
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      if (isEditableTarget(event.target)) {
        return;
      }
      if (!(event.metaKey || event.ctrlKey) || event.altKey) {
        return;
      }
      const key = event.key.toLowerCase();
      if (key === "z") {
        event.preventDefault();
        if (event.shiftKey) {
          handleRedo();
        } else {
          handleUndo();
        }
      } else if (key === "y") {
        event.preventDefault();
        handleRedo();
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  useEffect(() => {
    let current = true;
    void loadBootstrap().then((bootstrap) => {
      if (!current) {
        return;
      }
      setCatalogs(bootstrap.catalogs);
      setConnection(bootstrap.source);
      if (bootstrap.source === "local") {
        void listRecentProjects()
          .then((result) => {
            if (current) {
              setRecentProjects(result.projects);
            }
          })
          .catch(() => {
            // The class list is best-effort; the workbench stays usable.
          });
      }
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
  const selectedSeat = assignments.find(
    (seat) => seat.seatId === selectedSeatId,
  );
  const hasPlan = editorDraftId !== null || rotationPlan !== null;
  const canRedo = editorDraftId ? editorRedoDepth > 0 : false;
  const viewLabel =
    view === "history" ? t("nav.history") : t(`step.${viewToStep(view)}`);
  const viewMeta = useMemo(() => {
    switch (view) {
      case "roster":
        return t("app.students", { count: students.length });
      case "rules":
        return t("ctx.rulesCount", {
          count: constraints.length + groups.length + preferences.length,
        });
      case "history":
        return historySnapshots.length > 0
          ? t("ctx.historyCount", { count: historySnapshots.length })
          : null;
      case "canvas":
      case "export":
        return isDirty
          ? t("ctx.planDirty")
          : hasPlan
            ? t("ctx.planReady")
            : t("ctx.noPlan");
      default:
        return null;
    }
  }, [
    view,
    t,
    students.length,
    constraints.length,
    groups.length,
    preferences.length,
    historySnapshots.length,
    isDirty,
    hasPlan,
  ]);
  const diagnosticBadges = useMemo(() => {
    const badges: Record<string, "error" | "warning"> = {};
    for (const witness of draftAudit?.audit.hard_constraint_summary
      .witnesses ?? []) {
      const seatIds = (witness as { seat_ids?: unknown }).seat_ids;
      if (Array.isArray(seatIds)) {
        for (const seatId of seatIds) {
          badges[String(seatId)] = "error";
        }
      }
    }
    return badges;
  }, [draftAudit]);

  const firstRunProgress: FirstRunProgress = {
    roster: visitedViews.includes("roster"),
    room: visitedViews.includes("room"),
    rules: visitedViews.includes("rules"),
    generate: generationDone,
    export: exportedOnce,
  };
  const showFirstRun =
    !firstRunDismissed && !generationDone && historySnapshots.length === 0;

  function switchView(next: WorkbenchView) {
    if (isContentView(next) && !visitedViews.includes(next)) {
      setVisitedViews((current) => [...current, next]);
    }
    setView(next);
  }

  /** Restore the initial workbench draft (context switch, D1). */
  function resetWorkbench() {
    setStudents(demoStudents);
    setRevision(0);
    setSelectedFileName(null);
    setSelectedRoomId("compact");
    setRoomSettings(DEFAULT_ROOM_SETTINGS);
    setSelectedGoalId("daily-rotation");
    setAdvancedSettings(DEFAULT_ADVANCED_SETTINGS);
    setDetailedRules(DEFAULT_DETAILED_RULE_SETTINGS);
    setConstraints([]);
    setGroups([]);
    setPreferences([]);
    setAssignments(createSeatAssignments(4, 5, demoStudents, 16));
    setSelectedSeatId(null);
    setHistory([]);
    setEditorDraftId(null);
    setEditorRevision(0);
    setEditorUndoDepth(0);
      setEditorRedoDepth(0);
    setRotationPlan(null);
    setRotationEditors([]);
    setActiveRotationPeriod(1);
    setCandidateMetas([]);
    setHistorySnapshots([]);
    setHistoryFileNames([]);
    setHistoryError(null);
    setIsDirty(false);
    setView("roster");
  }

  function switchContext(next: ClassContext) {
    const same =
      classContext.kind === next.kind &&
      (next.kind !== "class" ||
        (classContext.kind === "class" && classContext.id === next.id));
    if (same) {
      return;
    }
    if (isDirty && !window.confirm(t("app.discardDraft"))) {
      return;
    }
    setClassContext(next);
    resetWorkbench();
  }

  function handleRestoreSnapshot(snapshot: HistorySnapshotPayload) {
    if (isDirty && !window.confirm(t("app.discardDraft"))) {
      return;
    }
    const { students: restoredStudents, assignments: restoredAssignments } =
      restoreSnapshotPlan(snapshot, assignments);
    if (restoredStudents.length === 0) {
      setSaveError(t("history.notRestorable"));
      return;
    }
    setStudents(restoredStudents);
    setAssignments(restoredAssignments);
    setEditorDraftId(null);
    setEditorRevision(0);
    setEditorUndoDepth(0);
    setEditorRedoDepth(0);
    setRotationPlan(null);
    setRotationEditors([]);
    setActiveRotationPeriod(1);
    setSelectedSeatId(null);
    setSaveError(null);
    setIsDirty(true);
    switchView("canvas");
  }

  function handleSaveAsClass(name: string) {
    const entry: SessionClass = { id: newCommandId(), name };
    setSessionClasses((current) => [...current, entry]);
    setSaveAsOpen(false);
    // G-5: the scratch draft graduates into the new class context as-is.
    setClassContext({ kind: "class", id: entry.id, name });
  }

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
      setEditorRedoDepth(0);
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
      setEditorRedoDepth(0);
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

  function handleGroupBatchAdd(next: CommonGroupRule[]) {
    setGroups((current) => [...current, ...next]);
  }

  function handlePreferenceToggle(id: CommonPreferenceId) {
    setPreferences((current) =>
      current.includes(id)
        ? current.filter((value) => value !== id)
        : [...current, id],
    );
  }

  async function handleHistoryFiles(files: File[]) {
    const MAX_HISTORY_FILES = 40;
    const MAX_HISTORY_FILE_BYTES = 20 * 1024 * 1024;
    if (files.length > MAX_HISTORY_FILES) {
      setHistoryError(t("generate.historyTooMany"));
      return;
    }

    const parsed: HistorySnapshotPayload[] = [];
    try {
      for (const file of files) {
        if (file.size > MAX_HISTORY_FILE_BYTES) {
          throw new Error("history_file_too_large");
        }
        const value: unknown = JSON.parse(await file.text());
        const entries = Array.isArray(value) ? value : [value];
        if (
          entries.length === 0 ||
          entries.some(
            (entry) =>
              entry === null ||
              typeof entry !== "object" ||
              Array.isArray(entry) ||
              !("students" in entry) ||
              !("layout" in entry) ||
              !("rules" in entry) ||
              !("assignments" in entry),
          )
        ) {
          throw new Error("history_file_invalid");
        }
        parsed.push(...(entries as HistorySnapshotPayload[]));
      }
    } catch (error) {
      console.error("History import failed", error);
      setHistoryError(
        error instanceof Error && error.message === "history_file_too_large"
          ? t("generate.historyTooLarge")
          : t("generate.historyInvalid"),
      );
      return;
    }

    setHistorySnapshots(parsed);
    setHistoryFileNames(files.map((file) => file.name));
    setHistoryError(null);
  }

  function clearHistoryFiles() {
    setHistorySnapshots([]);
    setHistoryFileNames([]);
    setHistoryError(null);
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
      setIsDirty(true);
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
      setEditorRedoDepth(editor.redo_depth);
      setSelectedSeatId(null);
      setIsDirty(true);
    } catch (err) {
      setSaveError(friendlyError(err, t));
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
      setIsDirty(true);
      return previous.slice(0, -1);
    });
  }

  function handleRedo() {
    if (editorDraftId) {
      void applyEditorCommand({ action: "redo", operations: [] });
    }
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
    setIsDirty(true);
  }

  function applyEditorState(editor: EditorState) {
    const plan = editorToPlan(editor);
    setStudents(plan.students);
    setAssignments(plan.assignments);
    setEditorDraftId(editor.draft_id);
    setEditorRevision(editor.revision);
    setEditorUndoDepth(editor.undo_depth);
    setEditorRedoDepth(editor.redo_depth);
    setSelectedSeatId(null);
    setIsDirty(false);
  }

  /** Batch lock/unlock from the canvas box selection (one atomic command). */
  function handleLockSelection(seatIds: string[], lock: boolean) {
    if (seatIds.length === 0) {
      return;
    }
    if (editorDraftId) {
      void applyEditorCommand({
        action: "apply",
        operations: planBatchLock(seatIds, lock),
      });
      return;
    }
    setHistory((previous) => [...previous, assignments]);
    setAssignments((current) => {
      const locked = new Set(seatIds);
      return current.map((seat) =>
        locked.has(seat.seatId) ? { ...seat, locked: lock } : seat,
      );
    });
    setIsDirty(true);
  }

  /** Batch move of a canvas multi-selection onto a drop seat. */
  function handleBatchMove(selectedIds: string[], dropSeatId: string) {
    const ops = planBatchMove(assignments, selectedIds, dropSeatId);
    if (ops.length === 0) {
      return;
    }
    if (editorDraftId) {
      void applyEditorCommand({ action: "apply", operations: ops });
      return;
    }
    setHistory((previous) => [...previous, assignments]);
    setAssignments((current) => {
      let next = current;
      for (const move of (ops[0].payload as { moves: Array<{ student_key: string; seat_id: string }> }).moves) {
        const student = next.find(
          (seat) => seat.student?.id === move.student_key,
        )?.student;
        if (!student) {
          continue;
        }
        next = next.map((seat) =>
          seat.seatId === move.seat_id
            ? { ...seat, student }
            : seat.student?.id === move.student_key
              ? { ...seat, student: undefined }
              : seat,
        );
      }
      return next;
    });
    setIsDirty(true);
  }

  /**
   * Table view edit (D2): place or remove a student on one seat. A student
   * already seated elsewhere is rejected up front — the Rust editor applies
   * the same rule.
   */
  function handleTableAssign(seatId: string, studentId: string | null) {
    const seat = assignments.find((item) => item.seatId === seatId);
    if (!seat || seat.locked) {
      return;
    }
    if (studentId) {
      const occupant = assignments.find(
        (item) => item.seatId === seatId && item.student,
      );
      // Reject only when the target seat holds a *different* student (the
      // Rust editor applies the same rule). Moving a student from another
      // seat into an empty seat is a plain move; occupying a seat that
      // already holds this student is a no-op.
      if (occupant?.student && occupant.student.id !== studentId) {
        setSaveError(t("canvas.tableDuplicate", { seat: seatId }));
        return;
      }
    }
    const operations: EditorOperation[] = studentId
      ? [
          {
            kind: "move_student",
            payload: { student_key: studentId, seat_id: seatId },
          },
        ]
      : seat.student
        ? [
            {
              kind: "unseat_student",
              payload: { student_key: seat.student.id },
            },
          ]
        : [];
    if (operations.length === 0) {
      return;
    }
    if (editorDraftId) {
      void applyEditorCommand({ action: "apply", operations });
      return;
    }
    setHistory((previous) => [...previous, assignments]);
    setAssignments((current) =>
      current.map((item) =>
        item.seatId === seatId
          ? {
              ...item,
              student: studentId
                ? current.find((candidate) => candidate.student?.id === studentId)
                    ?.student
                : undefined,
            }
          : item,
      ),
    );
    setIsDirty(true);
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
      setSaveError(friendlyError(err, t));
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
    setView("canvas");
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
        historySnapshots,
      };
      const response = rotationSettings.enabled
        ? await generateRotationPlan(
            buildGenerateRotationPlanRequest({
              ...requestArgs,
              rotation: rotationSettings,
            }),
          )
        : await generateClass(buildGenerateClassRequest(requestArgs));
      if (!response.feasible) {
        // ProvenInfeasible/Timeout/Unknown/Cancelled are successful transport
        // responses with no editable assignment, not HTTP failures.
        setSaveError(t("app.planNotFound"));
        return;
      }
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
      // Candidate comparison (D5): fetch every candidate's draft and keep
      // its seat plan so the panel can diff and switch without re-solving.
      const metas: CandidateMeta[] = [];
      if ("candidates" in response && Array.isArray(response.candidates)) {
        for (const candidate of response.candidates) {
          try {
            const state = await fetchEditorState(candidate.candidate_id);
            const plan = editorToPlan(state);
            metas.push({
              draft_id: candidate.candidate_id,
              total_score: candidate.total_score,
              recommended: candidate.recommended,
              assignments: plan.assignments,
            });
          } catch {
            // A candidate draft may already be evicted; skip it.
          }
        }
      }
      setCandidateMetas(metas);
      setHistory([]);
      setSelectedSeatId(null);
      setIsDirty(false);
      setGenerationDone(true);
      setFirstRunDismissed(true);
      window.localStorage.setItem(FIRST_RUN_KEY, "done");
      setView("canvas");
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
        setSaveError(friendlyError(err, t));
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
        template: exportTemplate,
        privacy: exportPrivacy,
        orientation,
        page_scale: pageScale,
        locale: locale === "zh-CN" ? "zh" : "en",
      });
      const desktopSave = await saveBlobWithDialog(filename, blob);
      if (desktopSave === "cancelled") {
        return;
      }
      if (desktopSave === "unavailable") {
        const url = URL.createObjectURL(blob);
        const link = document.createElement("a");
        link.href = url;
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        link.remove();
        URL.revokeObjectURL(url);
      }
      setPreviewOpen(false);
      setIsDirty(false);
      setExportedOnce(true);
    } catch (err) {
      setSaveError(friendlyError(err, t));
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
    clearHistoryFiles();
    setEditorDraftId(null);
    setEditorRevision(0);
    setEditorUndoDepth(0);
      setEditorRedoDepth(0);
    setRotationPlan(null);
    setRotationEditors([]);
    setActiveRotationPeriod(1);
    setCandidateMetas([]);
    switchView("room");
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
      setEditorRedoDepth(0);
    setRotationPlan(null);
    setRotationEditors([]);
    setActiveRotationPeriod(1);
  }

  const contextAction = contextActionFor(view, hasPlan);

  function handleContextAction(action: ContextAction) {
    switch (action.kind) {
      case "navigate":
        switchView(action.target);
        break;
      case "generate":
        void handleGenerate();
        break;
      case "preview":
        setPreviewOpen(true);
        break;
      case "exportMenu":
        break;
    }
  }

  return (
    <>
      <a className="skip-link" href="#main-workspace">
        {t("app.skip")}
      </a>
      <AppHeader
        locale={locale}
        studentCount={students.length}
        connection={connection}
        t={t}
        onLocaleChange={setLocale}
      />
      <div className="app-shell">
        <Sidebar
          activeView={isContentView(view) ? view : null}
          context={classContext}
          connection={connection}
          projects={recentProjects}
          sessionClasses={sessionClasses}
          t={t}
          onSelectView={switchView}
          onSelectClass={(id, name) =>
            switchContext({ kind: "class", id, name })
          }
          onSelectTemp={() => switchContext({ kind: "temp" })}
        />
        <div className="workbench-column">
          <ContextBar
            context={classContext}
            viewLabel={viewLabel}
            meta={viewMeta}
            action={contextAction}
            exportFormats={catalogs.exportFormats}
            locale={locale}
            isGenerating={isGenerating}
            canGenerate={rosterIsValid(students)}
            t={t}
            onAction={handleContextAction}
            onQuickExport={(formatId) => {
              void handleSave(formatId);
            }}
            onExportSettings={() => switchView("export")}
            onSaveAsClass={() => setSaveAsOpen(true)}
          />
          {showFirstRun ? (
            <FirstRunChecklist
              progress={firstRunProgress}
              t={t}
              onDismiss={() => {
                setFirstRunDismissed(true);
                window.localStorage.setItem(FIRST_RUN_KEY, "done");
              }}
            />
          ) : null}
          <main
            id="main-workspace"
            className={`main-workspace view-${view}`}
            tabIndex={-1}
          >
            {view === "history" ? (
              <HistoryRotationPanel
                locale={locale}
                t={t}
                rotationPlan={rotationPlan}
                rotationDraftIds={rotationEditors.map(
                  (editor) => editor.draft_id,
                )}
                historyFileNames={historyFileNames}
                historySnapshots={historySnapshots}
                historyError={historyError}
                assignments={assignments}
                students={students}
                isDirty={isDirty}
                activeRotationPeriod={activeRotationPeriod}
                onRotationLoad={handleRotationLoad}
                onRotationPeriodSelect={(period) => {
                  void handleRotationPeriodSelect(period);
                }}
                onHistoryFilesChange={(files) => {
                  void handleHistoryFiles(files);
                }}
                onHistoryClear={clearHistoryFiles}
                onRestoreSnapshot={handleRestoreSnapshot}
              />
            ) : (
              <WorkflowPanel
                step={viewToStep(view)}
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
                exportTemplate={exportTemplate}
                exportPrivacy={exportPrivacy}
                orientation={orientation}
                pageScale={pageScale}
                advancedSettings={advancedSettings}
                rotationSettings={rotationSettings}
                detailedRules={detailedRules}
                historyFileNames={historyFileNames}
                historySnapshotCount={historySnapshots.length}
                historyError={historyError}
                rotationPlan={rotationPlan}
                activeRotationPeriod={activeRotationPeriod}
                roomSettings={roomSettings}
                constraints={constraints}
                groups={groups}
                preferences={preferences}
                error={view === "generate" ? saveError : null}
                selectedSeat={selectedSeat}
                canUndo={
                  editorDraftId ? editorUndoDepth > 0 : history.length > 0
                }
                isGenerating={isGenerating}
                hideActions
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
                onExportTemplateChange={(template) => {
                  setExportTemplate(template);
                  setExportPrivacy({ ...DEFAULT_EXPORT_PRIVACY[template] });
                }}
                onExportPrivacyChange={(changes) =>
                  setExportPrivacy((current) => ({ ...current, ...changes }))
                }
                onOrientationChange={setOrientation}
                onPageScaleChange={setPageScale}
                onAdvancedSettingsChange={(changes) =>
                  setAdvancedSettings((current) => ({ ...current, ...changes }))
                }
                onRotationSettingsChange={(changes) =>
                  setRotationSettings((current) => ({ ...current, ...changes }))
                }
                onDetailedRulesChange={(changes) =>
                  setDetailedRules((current) => ({ ...current, ...changes }))
                }
                onHistoryFilesChange={(files) => {
                  void handleHistoryFiles(files);
                }}
                onHistoryClear={clearHistoryFiles}
                onRotationPeriodSelect={(period) => {
                  void handleRotationPeriodSelect(period);
                }}
                onRoomSettingsChange={handleRoomSettingsChange}
                onConstraintAdd={handleConstraintAdd}
                onConstraintBatchAdd={handleConstraintBatchAdd}
                onConstraintChange={handleConstraintChange}
                onConstraintRemove={handleConstraintRemove}
                onGroupAdd={handleGroupAdd}
                onGroupBatchAdd={handleGroupBatchAdd}
                onGroupChange={handleGroupChange}
                onGroupRemove={handleGroupRemove}
                onPreferenceToggle={handlePreferenceToggle}
                onBack={() => undefined}
                onNext={() => undefined}
                onGenerate={handleGenerate}
                onUndo={handleUndo}
                onToggleLock={handleToggleLock}
                onPreview={() => setPreviewOpen(true)}
                onOpenRules={() => switchView("rules")}
              />
            )}

            {view === "canvas" && candidateMetas.length > 1 ? (
              <CandidatesPanel
                candidates={candidateMetas}
                repro={{
                  seed: advancedSettings.seed.trim(),
                  solver: advancedSettings.backend,
                  timeLimitSeconds: advancedSettings.timeLimitSeconds,
                  historyCount: historySnapshots.length,
                }}
                locale={locale}
                t={t}
                onChoose={(draftId) => {
                  void fetchEditorState(draftId)
                    .then(applyEditorState)
                    .catch((error: unknown) =>
                      setSaveError(friendlyError(error, t)),
                    );
                }}
              />
            ) : null}

            {view === "canvas" ? (
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
                {saveError ? (
                  <p className="inline-error" role="alert">
                    {saveError}
                  </p>
                ) : null}
                <SeatingCanvasEditor
                  assignments={assignments}
                  students={students}
                  canUndo={
                    editorDraftId ? editorUndoDepth > 0 : history.length > 0
                  }
                  canRedo={canRedo}
                  diagnosticBadges={diagnosticBadges}
                  focusSeatId={diagnosticFocus}
                  onDiagnosticClick={setDiagnosticFocus}
                  t={t}
                  onSeatActivate={handleSeatActivate}
                  onSwap={(from, to) => {
                    const first = assignments.find(
                      (seat) => seat.seatId === from,
                    );
                    const second = assignments.find(
                      (seat) => seat.seatId === to,
                    );
                    if (first && second) {
                      void syncSwap(first, second);
                    }
                  }}
                  onBatchMove={handleBatchMove}
                  onLockSelection={(ids) => handleLockSelection(ids, true)}
                  onUnlockSelection={(ids) => handleLockSelection(ids, false)}
                  onAssign={handleTableAssign}
                  onUndo={handleUndo}
                  onRedo={handleRedo}
                />
              </section>
            ) : null}

            {view === "canvas" ? (
              <aside className="workspace-side-rail">
                <UnseatedTray students={unseatedStudents} t={t} />
                <DiagnosticsPanel
                  report={draftAudit}
                  students={students}
                  focusSeatId={diagnosticFocus}
                  t={t}
                  onFocusChange={setDiagnosticFocus}
                  onFix={(operations) => {
                    if (editorDraftId) {
                      void applyEditorCommand({
                        action: "apply",
                        operations: operations as EditorOperation[],
                      });
                    }
                  }}
                />
              </aside>
            ) : null}
          </main>
        </div>
      </div>
      <ExportPreviewDialog
        assignments={assignments}
        orientation={orientation}
        format={selectedExportFormat}
        template={exportTemplate}
        privacy={exportPrivacy}
        open={previewOpen}
        isSaving={isSaving}
        error={saveError}
        t={t}
        onClose={() => setPreviewOpen(false)}
        onSave={handleSave}
      />
      <SaveAsClassDialog
        open={saveAsOpen}
        t={t}
        onClose={() => setSaveAsOpen(false)}
        onConfirm={handleSaveAsClass}
      />
    </>
  );
}
