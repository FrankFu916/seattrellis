import { useEffect, useMemo, useState } from "react";

import {
  compareProjectArtifacts,
  applyProjectMigration,
  downloadProjectBundle,
  fetchProjectHistory,
  listRecentProjects,
  previewProjectMigration,
  restoreProjectArtifact,
  restoreProjectBundle,
  RosterApiError,
  saveProjectRotationPlan,
  scanProjectPrivacy,
} from "../api/client";
import type {
  ProjectArtifact,
  ProjectArtifactCompareResponse,
  ProjectHistoryResponse,
  ProjectMigrationResponse,
  ProjectPrivacyResponse,
  RotationPlan,
  RecentProject,
} from "../api/types";
import type { Locale, Translate } from "../i18n/messages";

type ProjectWorkspacePanelProps = {
  locale: Locale;
  t: Translate;
  rotationPlan?: RotationPlan | null;
  rotationDraftIds?: string[];
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
    | "rotation-save"
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

  useEffect(() => {
    if (!allArtifacts.length) {
      setCompareLeftPath("");
      setCompareRightPath("");
      setRestoreArtifactPath("");
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
  }, [allArtifacts]);

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
      setMigrationPreview(result);
      await refreshProjects();
      setStatus(t("project.statusMigrationApplied", { path: result.output_path ?? result.source_path }));
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
            </div>
            {migrationPreview && (
              <div className="project-migration-result" data-testid="project-migration-result" role="status">
                <strong>{t("project.migrationReady")}</strong>
                <span>{t("project.migrationSchema", { version: migrationPreview.schema_version })}</span>
                <small>{migrationPreview.output_path ?? t("project.migrationInPlaceTarget")}</small>
                {migrationPreview.backup_path && (
                  <small>{t("project.migrationBackup", { path: migrationPreview.backup_path })}</small>
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
    </article>
  );
}
