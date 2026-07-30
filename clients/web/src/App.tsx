import { useEffect, useMemo, useState } from "react";

import { loadBootstrap } from "./api/client";
import {
  createSeatAssignments,
  demoBootstrap,
  demoStudents,
} from "./api/demo";
import type { BootstrapData, SeatAssignment, Student } from "./api/types";
import { AppHeader } from "./components/AppHeader";
import { DiagnosticsPanel } from "./components/DiagnosticsPanel";
import { ExportPreviewDialog } from "./components/ExportPreviewDialog";
import { RosterImportPanel } from "./components/RosterImportPanel";
import { SeatingCanvas } from "./components/SeatingCanvas";
import { StepNavigation } from "./components/StepNavigation";
import { UnseatedTray } from "./components/UnseatedTray";
import { WorkflowPanel } from "./components/WorkflowPanel";
import {
  deriveDiagnostics,
  getAdjacentStep,
  getUnseatedStudents,
  seatRemainingStudents,
  swapStudents,
  toggleSeatLock,
  type WorkflowStep,
} from "./domain/workflow";
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

function getInitialLocale(): Locale {
  const stored = window.localStorage.getItem(LOCALE_STORAGE_KEY);
  if (stored === "zh-CN" || stored === "en") {
    return stored;
  }
  return window.navigator.language.toLowerCase().startsWith("zh")
    ? "zh-CN"
    : "en";
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
  const [assignments, setAssignments] = useState<SeatAssignment[]>(() =>
    createSeatAssignments(4, 5, demoStudents, 16),
  );
  const [selectedSeatId, setSelectedSeatId] = useState<string | null>(null);
  const [history, setHistory] = useState<SeatAssignment[][]>([]);
  const [isGenerating, setIsGenerating] = useState(false);
  const [previewOpen, setPreviewOpen] = useState(false);
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

    const updated = swapStudents(assignments, selectedSeatId, seatId);
    if (updated !== assignments) {
      setHistory((previous) => [...previous, assignments]);
      setAssignments(updated);
      setSelectedSeatId(null);
    } else {
      setSelectedSeatId(seatId);
    }
  }

  function handleUndo() {
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
    setHistory((previous) => [...previous, assignments]);
    setAssignments((current) => toggleSeatLock(current, selectedSeatId));
  }

  function handleGenerate() {
    setIsGenerating(true);
    window.setTimeout(() => {
      setHistory((previous) => [...previous, assignments]);
      setAssignments((current) =>
        seatRemainingStudents(current, students),
      );
      setSelectedSeatId(null);
      setIsGenerating(false);
      setStep("adjust");
    }, 420);
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
    setStep("room");
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
            selectedFileName={selectedFileName}
            rooms={catalogs.roomTemplates}
            selectedRoomId={selectedRoomId}
            goals={catalogs.teacherGoals}
            selectedGoalId={selectedGoalId}
            exportFormats={catalogs.exportFormats}
            selectedExportFormat={selectedExportFormat}
            orientation={orientation}
            showStudentIds={showStudentIds}
            selectedSeat={selectedSeat}
            canUndo={history.length > 0}
            isGenerating={isGenerating}
            rosterSlot={
              <RosterImportPanel
                locale={locale}
                t={t}
                currentStudents={students}
                currentRevision={revision}
                onImportConfirmed={handleRosterImported}
              />
            }
            onFileSelected={setSelectedFileName}
            onRoomChange={handleRoomChange}
            onGoalChange={setSelectedGoalId}
            onExportFormatChange={setSelectedExportFormat}
            onOrientationChange={setOrientation}
            onShowStudentIdsChange={setShowStudentIds}
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
          </aside>
        </main>
      </div>
      <ExportPreviewDialog
        assignments={assignments}
        orientation={orientation}
        open={previewOpen}
        t={t}
        onClose={() => setPreviewOpen(false)}
      />
    </>
  );
}

