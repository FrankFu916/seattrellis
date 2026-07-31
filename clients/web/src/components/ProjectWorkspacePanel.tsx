import { useEffect, useMemo, useState } from "react";

import {
  downloadProjectBundle,
  fetchProjectHistory,
  listRecentProjects,
  restoreProjectBundle,
  RosterApiError,
  scanProjectPrivacy,
} from "../api/client";
import type {
  ProjectArtifact,
  ProjectHistoryResponse,
  ProjectPrivacyResponse,
  RecentProject,
} from "../api/types";
import type { Locale, Translate } from "../i18n/messages";

type ProjectWorkspacePanelProps = {
  locale: Locale;
  t: Translate;
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
}: ProjectWorkspacePanelProps) {
  const [root, setRoot] = useState(".");
  const [projects, setProjects] = useState<RecentProject[]>([]);
  const [selectedPath, setSelectedPath] = useState("");
  const [history, setHistory] = useState<ProjectHistoryResponse | null>(null);
  const [privacy, setPrivacy] = useState<ProjectPrivacyResponse | null>(null);
  const [restoreFile, setRestoreFile] = useState<File | null>(null);
  const [restoreTarget, setRestoreTarget] = useState("./restored-project");
  const [busy, setBusy] = useState<"loading" | "scanning" | "backup" | "restore" | null>(null);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  const allArtifacts = useMemo(
    () => [
      ...(history?.history ?? []).map((item) => ({ ...item, group: "history" })),
      ...(history?.outputs ?? []).map((item) => ({ ...item, group: "outputs" })),
    ],
    [history],
  );

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
          </>
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
