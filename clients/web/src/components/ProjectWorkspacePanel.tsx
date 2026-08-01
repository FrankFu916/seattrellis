import { useEffect, useMemo, useState } from "react";

import {
  compareProjectArtifacts,
  applyProjectMigration,
  downloadProjectBundle,
  fetchProjectHistory,
  listRecentProjects,
  loadProjectRotationPlan,
  downloadProjectGroupRegister,
  previewProjectMigration,
  restoreProjectMigrationBackup,
  restoreProjectArtifact,
  restoreProjectBundle,
  RosterApiError,
  saveProjectRotationPlan,
  scanProjectPrivacy,
} from "../api/client";
import type {
  ProjectArtifact,
  ProjectArtifactCompareResponse,
  ProjectArtifactOperation,
  ProjectHistoryResponse,
  ProjectMigrationChange,
  ProjectMigrationReferenceCheck,
  ProjectMigrationResponse,
  ProjectPrivacyResponse,
  ProjectRotationLoadResponse,
  RotationPlan,
  RecentProject,
} from "../api/types";
import type { Locale, Translate } from "../i18n/messages";

type ProjectWorkspacePanelProps = {
  locale: Locale;
  t: Translate;
  rotationPlan?: RotationPlan | null;
  rotationDraftIds?: string[];
  onRotationLoad?: (result: ProjectRotationLoadResponse) => void;
};

function errorMessage(error: unknown): string {
  if (error instanceof RosterApiError || error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function formatDate(value: string, locale: Locale): string {
  return new Intl.DateTimeFormat(locale === "zh-CN" ? "zh-CN" : "en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function artifactKindLabel(
  artifact: ProjectArtifact,
  t: Translate,
): string {
  switch (artifact.kind) {
    case "snapshot":
      return t("project.kindSnapshot");
    case "candidate_set":
      return t("project.kindCandidates");
    case "rotation_plan":
      return t("project.kindRotation");
    default:
      return t("project.kindUnknown");
  }
}

function artifactSourceLabel(
  artifact: ProjectArtifact,
  t: Translate,
): string | null {
  switch (artifact.provenance?.source) {
    case "generated":
      return t("project.sourceGenerated");
    case "manual_edit":
      return t("project.sourceManualEdit");
    case "rotation_edit":
      return t("project.sourceRotationEdit");
    case "restored":
      return t("project.sourceRestored");
    case "unknown":
      return t("project.sourceUnknown");
    default:
      return null;
  }
}

function operationActionLabel(
  action: ProjectArtifactOperation["action"],
  t: Translate,
): string {
  switch (action) {
    case "apply":
      return t("project.operationApply");
    case "undo":
      return t("project.operationUndo");
    case "redo":
      return t("project.operationRedo");
    default:
      return t("project.operationUnknown");
  }
}

function operationKindLabel(kind: string, t: Translate): string {
  switch (kind) {
    case "swap_students":
      return t("project.operation.swapStudents");
    case "move_student":
      return t("project.operation.moveStudent");
    case "batch_move":
      return t("project.operation.batchMove");
    case "seat_student":
      return t("project.operation.seatStudent");
    case "unseat_student":
      return t("project.operation.unseatStudent");
    case "lock_student":
      return t("project.operation.lockStudent");
    case "unlock_student":
      return t("project.operation.unlockStudent");
    case "lock_seat":
      return t("project.operation.lockSeat");
    case "unlock_seat":
      return t("project.operation.unlockSeat");
    default:
      return t("project.operationOther");
  }
}

function migrationChangeLabel(
  change: ProjectMigrationChange["change"],
  t: Translate,
): string {
  switch (change) {
    case "added":
      return t("project.migrationAdded");
    case "removed":
      return t("project.migrationRemoved");
    default:
      return t("project.migrationChanged");
  }
}

function migrationReferenceFieldLabel(
  field: ProjectMigrationReferenceCheck["field"],
  t: Translate,
): string {
  return t(`project.migrationReference.${field}` as Parameters<Translate>[0]);
}

function migrationReferenceStatusLabel(
  status: ProjectMigrationReferenceCheck["status"],
  t: Translate,
): string {
  switch (status) {
    case "ok":
      return t("project.migrationReferenceOk");
    case "missing":
      return t("project.migrationReferenceMissing");
    default:
      return t("project.migrationReferenceWrongType");
  }
}

function triggerDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

export function ProjectWorkspacePanel({
  locale,
  t,
  rotationPlan = null,
  rotationDraftIds = [],
  onRotationLoad,
}: ProjectWorkspacePanelProps) {
  const [root, setRoot] = useState(".");
  const [projects, setProjects] = useState<RecentProject[]>([]);
  const [selectedPath, setSelectedPath] = useState("");
  const [history, setHistory] = useState<ProjectHistoryResponse | null>(null);
  const [privacy, setPrivacy] = useState<ProjectPrivacyResponse | null>(null);
  const [restoreFile, setRestoreFile] = useState<File | null>(null);
  const [restoreTarget, setRestoreTarget] = useState("./restored-project");
  const [compareLeftPath, setCompareLeftPath] = useState("");
  const [compareRightPath, setCompareRightPath] = useState("");
  const [restoreArtifactPath, setRestoreArtifactPath] = useState("");
  const [loadRotationPath, setLoadRotationPath] = useState("");
  const [groupRegisterFormat, setGroupRegisterFormat] = useState<"html" | "csv">("html");
  const [migrationArtifactPath, setMigrationArtifactPath] = useState("");
  const [migrationInPlace, setMigrationInPlace] = useState(false);
  const [migrationPreview, setMigrationPreview] = useState<ProjectMigrationResponse | null>(null);
  const [comparison, setComparison] = useState<ProjectArtifactCompareResponse | null>(null);
  const [busy, setBusy] = useState<
    "loading"
    | "scanning"
    | "backup"
    | "restore"
    | "compare"
    | "restore-artifact"
    | "migration-preview"
    | "migration-apply"
    | "migration-restore"
    | "rotation-save"
    | "rotation-load"
    | "group-register"
    | null
  >(null);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  const allArtifacts = useMemo(
    () => [
      ...(history?.history ?? []).map((item) => ({ ...item, group: "history" })),
      ...(history?.outputs ?? []).map((item) => ({ ...item, group: "outputs" })),
    ],
    [history],
  );
  const rotationArtifacts = useMemo(
    () => allArtifacts.filter((artifact) => artifact.kind === "rotation_plan"),
    [allArtifacts],
  );

  useEffect(() => {
    if (!allArtifacts.length) {
      setCompareLeftPath("");
      setCompareRightPath("");
      setRestoreArtifactPath("");
      setLoadRotationPath("");
      setMigrationArtifactPath("");
      setMigrationPreview(null);
      return;
    }
    setCompareLeftPath((current) =>
      allArtifacts.some((artifact) => artifact.path === current)
        ? current
        : allArtifacts[0].path,
    );
    setCompareRightPath((current) =>
      allArtifacts.some((artifact) => artifact.path === current) &&
      current !== allArtifacts[0].path
        ? current
        : allArtifacts[1]?.path ?? "",
    );
    setRestoreArtifactPath((current) =>
      allArtifacts.some((artifact) => artifact.path === current)
        ? current
        : allArtifacts[0].path,
    );
    setMigrationArtifactPath((current) =>
      allArtifacts.some((artifact) => artifact.path === current) ? current : "",
    );
    setLoadRotationPath((current) =>
      rotationArtifacts.some((artifact) => artifact.path === current)
        ? current
        : rotationArtifacts[0]?.path ?? "",
    );
  }, [allArtifacts, rotationArtifacts]);

  async function openProject(path: string): Promise<void> {
    if (!path) {
      setHistory(null);
      setPrivacy(null);
      return;
    }
    setSelectedPath(path);
    setPrivacy(null);
    setError("");
    try {
      const response = await fetchProjectHistory(path);
      setHistory(response);
      setComparison(null);
      setMigrationPreview(null);
      setStatus(t("project.statusLoaded", { name: response.project_name }));
    } catch (caught) {
      setHistory(null);
      setError(t("project.error", { message: errorMessage(caught) }));
    }
  }

  async function refreshProjects(): Promise<void> {
    setBusy("loading");
    setError("");
    try {
      const response = await listRecentProjects(root.trim() || ".");
      setProjects(response.projects);
      const nextPath = response.projects.some((item) => item.path === selectedPath)
        ? selectedPath
        : response.projects[0]?.path ?? "";
      await openProject(nextPath);
    } catch (caught) {
      setProjects([]);
      setHistory(null);
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  useEffect(() => {
    void refreshProjects();
    // Loading once keeps a teacher's selected directory stable until refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function handlePrivacyScan(): Promise<void> {
    if (!selectedPath) {
      return;
    }
    setBusy("scanning");
    setError("");
    try {
      setPrivacy(await scanProjectPrivacy(selectedPath));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleBackup(): Promise<void> {
    if (!selectedPath) {
      return;
    }
    setBusy("backup");
    setError("");
    try {
      const result = await downloadProjectBundle(selectedPath);
      triggerDownload(result.blob, result.filename);
      setStatus(t("project.statusBackup", { name: result.filename }));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleRestore(): Promise<void> {
    if (!restoreFile || !restoreTarget.trim()) {
      return;
    }
    setBusy("restore");
    setError("");
    try {
      const result = await restoreProjectBundle(
        restoreFile,
        restoreTarget.trim(),
      );
      setRestoreFile(null);
      const input = document.getElementById("project-restore-file") as HTMLInputElement | null;
      if (input) {
        input.value = "";
      }
      await refreshProjects();
      setStatus(t("project.statusRestore", { path: result.project_path }));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleCompare(): Promise<void> {
    if (!selectedPath || !compareLeftPath || !compareRightPath) {
      return;
    }
    setBusy("compare");
    setError("");
    try {
      setComparison(
        await compareProjectArtifacts(selectedPath, compareLeftPath, compareRightPath),
      );
      setStatus(t("project.statusCompared"));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleRestoreArtifact(): Promise<void> {
    if (!selectedPath || !restoreArtifactPath) {
      return;
    }
    setBusy("restore-artifact");
    setError("");
    try {
      const result = await restoreProjectArtifact(selectedPath, restoreArtifactPath);
      await refreshProjects();
      setStatus(t("project.statusArtifactRestored", { name: result.restored_artifact }));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleMigrationPreview(): Promise<void> {
    if (!selectedPath) {
      return;
    }
    setBusy("migration-preview");
    setError("");
    try {
      const result = await previewProjectMigration(
        selectedPath,
        migrationArtifactPath || undefined,
        migrationInPlace,
      );
      setMigrationPreview(result);
      setStatus(t("project.statusMigrationPreview"));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleMigrationApply(): Promise<void> {
    if (!selectedPath || !migrationPreview) {
      return;
    }
    setBusy("migration-apply");
    setError("");
    try {
      const result = await applyProjectMigration(
        selectedPath,
        migrationArtifactPath || undefined,
        migrationInPlace,
      );
      await refreshProjects();
      // Refreshing the project list resets transient selection state. Keep the
      // apply result so a backup created by an in-place migration remains
      // available for the next explicit recovery action.
      setMigrationPreview(result);
      setStatus(t("project.statusMigrationApplied", { path: result.output_path ?? result.source_path }));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleMigrationRestore(): Promise<void> {
    if (!selectedPath || !migrationPreview?.backup_path) {
      return;
    }
    setBusy("migration-restore");
    setError("");
    try {
      const result = await restoreProjectMigrationBackup(
        selectedPath,
        migrationPreview.source_path,
        migrationPreview.backup_path,
      );
      await refreshProjects();
      setStatus(t("project.statusMigrationRestored", { path: result.source_path }));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleRotationSave(): Promise<void> {
    if (
      !selectedPath ||
      !rotationPlan ||
      rotationDraftIds.length !== rotationPlan.periods.length
    ) {
      return;
    }
    setBusy("rotation-save");
    setError("");
    try {
      const result = await saveProjectRotationPlan(
        selectedPath,
        rotationPlan,
        rotationDraftIds,
      );
      await refreshProjects();
      setStatus(t("project.statusRotationSaved", { path: result.output_path }));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleRotationLoad(): Promise<void> {
    if (!selectedPath || !loadRotationPath || !onRotationLoad) {
      return;
    }
    setBusy("rotation-load");
    setError("");
    try {
      const result = await loadProjectRotationPlan(selectedPath, loadRotationPath);
      onRotationLoad(result);
      setStatus(t("project.statusRotationLoaded", { name: result.rotation_plan.name }));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  async function handleGroupRegister(): Promise<void> {
    if (!selectedPath || !loadRotationPath) {
      return;
    }
    setBusy("group-register");
    setError("");
    try {
      const result = await downloadProjectGroupRegister(
        selectedPath,
        loadRotationPath,
        groupRegisterFormat,
        locale === "zh-CN" ? "zh" : "en",
      );
      triggerDownload(result.blob, result.filename);
      setStatus(t("project.statusGroupRegister", { name: result.filename }));
    } catch (caught) {
      setError(t("project.error", { message: errorMessage(caught) }));
    } finally {
      setBusy(null);
    }
  }

  return (
    <section
      className="side-card project-workspace-card"
      aria-labelledby="project-workspace-title"
      data-testid="project-workspace"
    >
      <header className="side-card-heading">
        <div>
          <span className="eyebrow">Project</span>
          <h2 id="project-workspace-title">{t("project.title")}</h2>
          <p>{t("project.subtitle")}</p>
        </div>
      </header>

      <div className="project-workspace-content">
        <label className="project-field" htmlFor="project-root-input">
          <span>{t("project.root")}</span>
          <input
            id="project-root-input"
            data-testid="project-root-input"
            value={root}
            onChange={(event) => setRoot(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                void refreshProjects();
              }
            }}
          />
          <small>{t("project.rootHint")}</small>
        </label>
        <button
          className="secondary-button project-refresh-button"
          type="button"
          data-testid="project-refresh"
          onClick={() => void refreshProjects()}
          disabled={busy !== null}
        >
          {t("project.refresh")}
        </button>

        <label className="project-field" htmlFor="project-select">
          <span>{t("project.select")}</span>
          <select
            id="project-select"
            data-testid="project-select"
            value={selectedPath}
            onChange={(event) => void openProject(event.target.value)}
            disabled={!projects.length || busy === "loading"}
          >
            {!projects.length && <option value="">{t("project.noProjects")}</option>}
            {projects.map((project) => (
              <option key={project.path} value={project.path}>
                {project.name} · {formatDate(project.modified_at, locale)}
              </option>
            ))}
          </select>
        </label>

        {history && (
          <>
            <div className="project-artifact-groups">
              <div className="project-artifact-group" data-testid="project-history">
                <h3>{t("project.history")}</h3>
                {!history.history.length && <p className="project-empty">{t("project.noArtifacts")}</p>}
                {history.history.map((artifact) => (
                  <ArtifactRow key={artifact.path} artifact={artifact} t={t} locale={locale} />
                ))}
              </div>
              <div className="project-artifact-group" data-testid="project-outputs">
                <h3>{t("project.outputs")}</h3>
                {!history.outputs.length && <p className="project-empty">{t("project.noArtifacts")}</p>}
                {history.outputs.map((artifact) => (
                  <ArtifactRow key={artifact.path} artifact={artifact} t={t} locale={locale} />
                ))}
              </div>
            </div>
            {history.warnings.length > 0 && (
              <p className="project-warning" role="status">
                {t("project.warning")}: {history.warnings.length}
              </p>
            )}
            {allArtifacts.length > 0 && (
              <div className="project-history-tools" data-testid="project-history-tools">
                <h3>{t("project.compareTitle")}</h3>
                <div className="project-compare-fields">
                  <label className="project-field">
                    <span>{t("project.compareLeft")}</span>
                    <select
                      data-testid="project-compare-left"
                      value={compareLeftPath}
                      onChange={(event) => setCompareLeftPath(event.target.value)}
                    >
                      {allArtifacts.map((artifact) => (
                        <option key={`left-${artifact.path}`} value={artifact.path}>
                          {artifact.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="project-field">
                    <span>{t("project.compareRight")}</span>
                    <select
                      data-testid="project-compare-right"
                      value={compareRightPath}
                      onChange={(event) => setCompareRightPath(event.target.value)}
                    >
                      {allArtifacts.map((artifact) => (
                        <option key={`right-${artifact.path}`} value={artifact.path}>
                          {artifact.name}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <button
                  className="secondary-button"
                  type="button"
                  data-testid="project-compare-button"
                  onClick={() => void handleCompare()}
                  disabled={
                    !compareLeftPath ||
                    !compareRightPath ||
                    compareLeftPath === compareRightPath ||
                    busy !== null
                  }
                >
                  {busy === "compare" ? t("project.comparing") : t("project.compareAction")}
                </button>
                {rotationArtifacts.length > 0 && (
                  <>
                    {onRotationLoad && (
                      <>
                        <label className="project-field">
                          <span>{t("project.openRotation")}</span>
                          <select
                            data-testid="project-open-rotation-select"
                            value={loadRotationPath}
                            onChange={(event) => setLoadRotationPath(event.target.value)}
                          >
                            {rotationArtifacts.map((artifact) => (
                              <option key={`open-rotation-${artifact.path}`} value={artifact.path}>
                                {artifact.name}
                              </option>
                            ))}
                          </select>
                        </label>
                        <button
                          className="secondary-button"
                          type="button"
                          data-testid="project-open-rotation-button"
                          onClick={() => void handleRotationLoad()}
                          disabled={!loadRotationPath || busy !== null}
                        >
                          {busy === "rotation-load"
                            ? t("project.openingRotation")
                            : t("project.openRotationAction")}
                        </button>
                      </>
                    )}
                    <label className="project-field">
                      <span>{t("project.groupRegisterFormat")}</span>
                      <select
                        data-testid="project-group-register-format"
                        value={groupRegisterFormat}
                        onChange={(event) =>
                          setGroupRegisterFormat(event.target.value as "html" | "csv")
                        }
                      >
                        <option value="html">HTML · {t("project.groupRegisterPrint")}</option>
                        <option value="csv">CSV · {t("project.groupRegisterData")}</option>
                      </select>
                    </label>
                    <button
                      className="secondary-button"
                      type="button"
                      data-testid="project-group-register-button"
                      onClick={() => void handleGroupRegister()}
                      disabled={!loadRotationPath || busy !== null}
                    >
                      {busy === "group-register"
                        ? t("project.groupRegistering")
                        : t("project.groupRegisterAction")}
                    </button>
                  </>
                )}
                <label className="project-field">
                  <span>{t("project.restoreArtifact")}</span>
                  <select
                    data-testid="project-restore-artifact-select"
                    value={restoreArtifactPath}
                    onChange={(event) => setRestoreArtifactPath(event.target.value)}
                  >
                    {allArtifacts.map((artifact) => (
                      <option key={`restore-${artifact.path}`} value={artifact.path}>
                        {artifact.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  className="secondary-button"
                  type="button"
                  data-testid="project-restore-artifact-button"
                  onClick={() => void handleRestoreArtifact()}
                  disabled={!restoreArtifactPath || busy !== null}
                >
                  {busy === "restore-artifact"
                    ? t("project.restoringArtifact")
                    : t("project.restoreArtifactAction")}
                </button>
                {comparison && (
                  <div
                    className="project-compare-result"
                    data-testid="project-compare-result"
                    role="status"
                  >
                    <strong>{t("project.compareResult")}</strong>
                    <span>
                      {t("project.assignmentChanges", {
                        count: comparison.diff.assignment_changes,
                      })}
                    </span>
                    <span>
                      {t("project.rosterAdded", { count: comparison.diff.roster_added })}
                    </span>
                    <span>
                      {t("project.rosterRemoved", {
                        count: comparison.diff.roster_removed,
                      })}
                    </span>
                    <span>
                      {t("project.layoutChanged", {
                        value: comparison.diff.layout_changed
                          ? t("project.yes")
                          : t("project.no"),
                      })}
                    </span>
                    <span>
                      {t("project.rulesChanged", {
                        value: comparison.diff.rules_changed
                          ? t("project.yes")
                          : t("project.no"),
                      })}
                    </span>
                    {comparison.diff.assignment_details.length > 0 ? (
                      <details
                        className="project-compare-details"
                        data-testid="project-assignment-details"
                      >
                        <summary>{t("project.assignmentDetails")}</summary>
                        <ul>
                          {comparison.diff.assignment_details.map((change) => (
                            <li
                              key={`${change.student_ref}-${change.before_seat_id ?? "none"}-${change.after_seat_id ?? "none"}`}
                            >
                              <strong>{change.student_ref}</strong>
                              <span>
                                {change.change === "moved"
                                  ? t("project.assignmentMoved")
                                  : change.change === "seated"
                                    ? t("project.assignmentSeated")
                                    : t("project.assignmentUnseated")}
                              </span>
                              <small>
                                {t("project.assignmentBefore")}: {change.before_seat_id ?? "—"}
                                {" → "}
                                {t("project.assignmentAfter")}: {change.after_seat_id ?? "—"}
                              </small>
                            </li>
                          ))}
                        </ul>
                      </details>
                    ) : null}
                  </div>
                )}
              </div>
            )}
          </>
        )}

        {selectedPath && (
          <div className="project-migration" data-testid="project-migration">
            <h3>{t("project.migrationTitle")}</h3>
            <p className="muted">{t("project.migrationHint")}</p>
            <label className="project-field" htmlFor="project-migration-artifact">
              <span>{t("project.migrationArtifact")}</span>
              <select
                id="project-migration-artifact"
                data-testid="project-migration-artifact"
                value={migrationArtifactPath}
                onChange={(event) => {
                  setMigrationArtifactPath(event.target.value);
                  setMigrationPreview(null);
                }}
              >
                <option value="">{t("project.migrationProjectFile")}</option>
                {allArtifacts.map((artifact) => (
                  <option key={`migration-${artifact.path}`} value={artifact.path}>
                    {artifact.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="rule-toggle">
              <input
                type="checkbox"
                data-testid="project-migration-in-place"
                checked={migrationInPlace}
                onChange={(event) => {
                  setMigrationInPlace(event.target.checked);
                  setMigrationPreview(null);
                }}
              />
              <span>{t("project.migrationInPlace")}</span>
            </label>
            <div className="project-actions">
              <button
                className="secondary-button"
                type="button"
                data-testid="project-migration-preview"
                onClick={() => void handleMigrationPreview()}
                disabled={busy !== null}
              >
                {busy === "migration-preview"
                  ? t("project.migrationPreviewing")
                  : t("project.migrationPreview")}
              </button>
              {migrationPreview && (
                <button
                  className="primary-button"
                  type="button"
                  data-testid="project-migration-apply"
                  onClick={() => void handleMigrationApply()}
                  disabled={busy !== null}
                >
                  {busy === "migration-apply"
                    ? t("project.migrationApplying")
                    : t("project.migrationApply")}
                </button>
              )}
              {migrationPreview?.backup_path && (
                <button
                  className="secondary-button"
                  type="button"
                  data-testid="project-migration-restore"
                  onClick={() => void handleMigrationRestore()}
                  disabled={busy !== null}
                >
                  {busy === "migration-restore"
                    ? t("project.migrationRestoring")
                    : t("project.migrationRestore")}
                </button>
              )}
            </div>
            {migrationPreview && (
              <div className="project-migration-result" data-testid="project-migration-result" role="status">
                <strong>{t("project.migrationReady")}</strong>
                <span>{t("project.migrationSchema", { version: migrationPreview.schema_version })}</span>
                <span>
                  {migrationPreview.before_valid
                    ? t("project.migrationBeforeValid")
                    : t("project.migrationAfterPending")}
                </span>
                <span>
                  {migrationPreview.after_valid === null
                    ? t("project.migrationAfterPending")
                    : migrationPreview.after_valid
                      ? t("project.migrationAfterValid")
                      : t("project.migrationAfterPending")}
                </span>
                <span>
                  {migrationPreview.rollback_available
                    ? t("project.migrationRollback")
                    : t("project.migrationSourcePreserved")}
                </span>
                <span>
                  {migrationPreview.change_count
                    ? t("project.migrationChanges", { count: migrationPreview.change_count })
                    : t("project.migrationChangesNone")}
                </span>
                <small>{migrationPreview.output_path ?? t("project.migrationInPlaceTarget")}</small>
                {migrationPreview.backup_path && (
                  <small>{t("project.migrationBackup", { path: migrationPreview.backup_path })}</small>
                )}
                {migrationPreview.changes.length > 0 && (
                  <details className="project-migration-details" data-testid="project-migration-details">
                    <summary>{t("project.migrationShowChanges")}</summary>
                    <ul>
                      {migrationPreview.changes.map((change) => (
                        <li key={`${change.change}-${change.path}`}>
                          <code>{change.path}</code>
                          <span>{migrationChangeLabel(change.change, t)}</span>
                          <small>
                            {change.before_type ?? "—"} → {change.after_type ?? "—"}
                          </small>
                        </li>
                      ))}
                    </ul>
                  </details>
                )}
                {migrationPreview.reference_checks && migrationPreview.reference_checks.length > 0 && (
                  <details className="project-migration-details" open>
                    <summary>{t("project.migrationReferenceTitle")}</summary>
                    <ul className="project-migration-references">
                      {migrationPreview.reference_checks.map((reference) => (
                        <li key={reference.field}>
                          <strong>{migrationReferenceFieldLabel(reference.field, t)}</strong>
                          <span>{reference.path}</span>
                          <small className={`migration-reference-${reference.status}`}>
                            {migrationReferenceStatusLabel(reference.status, t)}
                          </small>
                        </li>
                      ))}
                    </ul>
                  </details>
                )}
              </div>
            )}
          </div>
        )}

        {selectedPath &&
          rotationPlan &&
          rotationDraftIds.length === rotationPlan.periods.length && (
            <div className="project-migration" data-testid="project-rotation-save">
              <h3>{t("project.rotationSaveTitle")}</h3>
              <p className="muted">{t("project.rotationSaveHint")}</p>
              <button
                className="primary-button"
                type="button"
                data-testid="project-rotation-save-button"
                onClick={() => void handleRotationSave()}
                disabled={busy !== null}
              >
                {busy === "rotation-save"
                  ? t("project.rotationSaving")
                  : t("project.rotationSaveAction")}
              </button>
            </div>
          )}

        <div className="project-actions">
          <button
            className="secondary-button"
            type="button"
            data-testid="project-privacy-button"
            onClick={() => void handlePrivacyScan()}
            disabled={!selectedPath || busy !== null}
          >
            {busy === "scanning" ? t("project.scanning") : t("project.scan")}
          </button>
          <button
            className="primary-button"
            type="button"
            data-testid="project-backup-button"
            onClick={() => void handleBackup()}
            disabled={!selectedPath || busy !== null}
          >
            {busy === "backup" ? t("project.backingUp") : t("project.backup")}
          </button>
        </div>

        {privacy && (
          <div
            className={`project-privacy ${privacy.safe_for_public_sharing ? "is-safe" : "needs-review"}`}
            data-testid="project-privacy-status"
            role="status"
          >
            <strong>
              {privacy.safe_for_public_sharing
                ? t("project.privacySafe")
                : t("project.privacyReview")}
            </strong>
            <small>{t("project.filesScanned", { count: privacy.files_scanned })}</small>
            {!privacy.safe_for_public_sharing && (
              <ul>
                {privacy.findings.slice(0, 4).map((finding) => (
                  <li key={finding.file}>
                    {finding.file}: {finding.fields.join(", ")}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        <div className="project-restore">
          <h3>{t("project.restore")}</h3>
          <label className="file-picker project-file-picker" htmlFor="project-restore-file">
            <strong>{restoreFile?.name ?? t("project.restoreFile")}</strong>
            <input
              id="project-restore-file"
              data-testid="project-restore-file"
              type="file"
              accept=".zip,.seattrellis.zip,application/zip"
              onChange={(event) => setRestoreFile(event.target.files?.[0] ?? null)}
            />
          </label>
          <label className="project-field" htmlFor="project-restore-target">
            <span>{t("project.restoreTarget")}</span>
            <input
              id="project-restore-target"
              data-testid="project-restore-target"
              value={restoreTarget}
              onChange={(event) => setRestoreTarget(event.target.value)}
            />
            <small>{t("project.restoreTargetHint")}</small>
          </label>
          <button
            className="secondary-button"
            type="button"
            data-testid="project-restore-button"
            onClick={() => void handleRestore()}
            disabled={!restoreFile || !restoreTarget.trim() || busy !== null}
          >
            {busy === "restore" ? t("project.restoring") : t("project.restoreAction")}
          </button>
        </div>

        {status && <p className="project-status" data-testid="project-status" role="status">{status}</p>}
        {error && <p className="project-error" data-testid="project-error" role="alert">{error}</p>}
        <span className="sr-only">{allArtifacts.length} artifacts</span>
      </div>
    </section>
  );
}

function ArtifactRow({
  artifact,
  locale,
  t,
}: {
  artifact: ProjectArtifact;
  locale: Locale;
  t: Translate;
}) {
  const sourceLabel = artifactSourceLabel(artifact, t);
  const operationHistory = artifact.operation_history ?? [];
  return (
    <article className="project-artifact-row">
      <strong>{artifactKindLabel(artifact, t)}</strong>
      <span>{artifact.name}</span>
      <small>
        {formatDate(artifact.modified_at, locale)}
        {artifact.period_count
          ? ` · ${artifact.period_count} ${t("project.periods")}`
          : ""}
      </small>
      {sourceLabel && (
        <small className="project-artifact-provenance" data-testid="project-artifact-provenance">
          {sourceLabel}
          {artifact.provenance?.parent_name
            ? ` · ${t("project.sourceParent", {
                name: artifact.provenance.parent_name,
              })}`
            : ""}
          {artifact.provenance?.operation_count != null
            ? ` · ${t("project.sourceOperations", {
                count: artifact.provenance.operation_count,
              })}`
            : ""}
        </small>
      )}
      {operationHistory.length > 0 && (
        <details
          className="project-artifact-history"
          data-testid="project-artifact-operation-history"
        >
          <summary>{t("project.operationHistory")}</summary>
          <ol>
            {operationHistory.map((operation) => {
              const kinds = operation.operation_kinds
                .map((kind) => operationKindLabel(kind, t))
                .join(locale === "zh-CN" ? "、" : ", ");
              return (
                <li key={`${artifact.path}-${operation.sequence}`}>
                  <strong>
                    {t("project.operationStep", { sequence: operation.sequence })}
                  </strong>
                  <span>{operationActionLabel(operation.action, t)}</span>
                  <small>
                    {operation.operation_count > 0
                      ? t("project.operationSummary", {
                          count: operation.operation_count,
                          kinds,
                        })
                      : t("project.operationNoChanges")}
                  </small>
                </li>
              );
            })}
          </ol>
          {artifact.operation_history_truncated && (
            <small>{t("project.operationHistoryTruncated")}</small>
          )}
        </details>
      )}
    </article>
  );
}
