import { demoBootstrap } from "./demo";
import type {
  BootstrapData,
  CatalogResponse,
  CompiledLayoutResponse,
  CreateLayoutDraftRequest,
  EditorCommand,
  EditorState,
  ExportDraftRequest,
  GenerateClassRequest,
  GenerateClassResponse,
  GenerateRotationPlanRequest,
  GenerateRotationPlanResponse,
  HealthResponse,
  ProjectHistoryResponse,
  ProjectArtifactCompareResponse,
  ProjectArtifactRestoreResponse,
  ProjectGroupRegisterPreviewResponse,
  ProjectMigrationResponse,
  ProjectMigrationBatchResponse,
  ProjectMigrationRestoreResponse,
  ProjectRotationLoadResponse,
  ProjectRotationSaveResponse,
  ProjectListResponse,
  ProjectPrivacyResponse,
  ProjectRestoreResponse,
  RotationPlan,
  LayoutCommand,
  LayoutStateResponse,
  RosterDraftResponse,
  RosterUpdatePreviewRequest,
  RosterUpdatePreviewResponse,
  RuleTemplatesResponse,
  CompiledRule,
  DraftAuditReport,
} from "./types";

const API_ROOT = "/api/v1";
const REQUEST_TIMEOUT_MS = 1800;
const ROSTER_TIMEOUT_MS = 30_000;
const GENERATE_TIMEOUT_MS = 30_000;
let cachedDesktopSessionToken: string | null | undefined;
let sessionBootstrapPromise: Promise<string | null> | null = null;

export const EDITOR_PROTOCOL_VERSION = "1.0";

/** Bootstrap (or re-bootstrap after a 401) the loopback session token. */
async function bootstrapSessionToken(): Promise<string | null> {
  try {
    const response = await fetch(`${API_ROOT}/session`, {
      headers: { Accept: "application/json" },
    });
    if (!response.ok) {
      return null;
    }
    const data = (await response.json()) as { session_token?: string };
    const token = data.session_token ?? null;
    if (token) {
      cachedDesktopSessionToken = token;
      try {
        window.sessionStorage.setItem("seattrellis.desktop.session", token);
      } catch {
        // The in-memory copy suffices for this window.
      }
    }
    return token;
  } catch {
    return null;
  }
}

async function ensureSessionToken(): Promise<string | null> {
  const known = readDesktopSessionToken();
  if (known) {
    return known;
  }
  if (!sessionBootstrapPromise) {
    sessionBootstrapPromise = bootstrapSessionToken();
  }
  return sessionBootstrapPromise;
}

/** Drop a stale token (server restarted) and bootstrap a fresh one once. */
async function refreshSessionToken(): Promise<string | null> {
  cachedDesktopSessionToken = null;
  sessionBootstrapPromise = null;
  try {
    window.sessionStorage.removeItem("seattrellis.desktop.session");
  } catch {
    // Best effort; the in-memory copy is cleared above.
  }
  return bootstrapSessionToken();
}

async function fetchJson<T>(
  path: string,
  init?: RequestInit,
  timeoutMs = REQUEST_TIMEOUT_MS,
): Promise<T> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);

  const headers = new Headers(init?.headers);
  headers.set("Accept", "application/json");
  let sessionToken = await ensureSessionToken();
  if (sessionToken) {
    headers.set("Authorization", `Bearer ${sessionToken}`);
  }

  try {
    let response = await fetch(`${API_ROOT}${path}`, {
      ...init,
      headers,
      signal: controller.signal,
    });
    if (response.status === 401) {
      // The local service restarted and rotated its token — or the token was
      // never obtained because the service was down when the page bootstrapped.
      // Either way, re-bootstrap once and retry (refreshSessionToken also
      // clears the cached bootstrap promise so a later 401 can retry again).
      sessionToken = await refreshSessionToken();
      if (sessionToken) {
        headers.set("Authorization", `Bearer ${sessionToken}`);
        response = await fetch(`${API_ROOT}${path}`, {
          ...init,
          headers,
          signal: controller.signal,
        });
      }
    }
    if (!response.ok) {
      const detail = await safeErrorDetail(response);
      throw new RosterApiError(response.status, detail.code, detail.message);
    }
    return (await response.json()) as T;
  } finally {
    window.clearTimeout(timeout);
  }
}

async function safeErrorDetail(
  response: Response,
): Promise<{ code: string; message: string }> {
  try {
    const body = (await response.json()) as {
      code?: string;
      message?: string;
      detail?: { code?: string; message?: string };
    };
    const code = body.code ?? body.detail?.code ?? "request_failed";
    const message =
      body.message ?? body.detail?.message ?? "The request could not be completed.";
    return { code, message };
  } catch {
    return { code: "request_failed", message: "The request could not be completed." };
  }
}

export class RosterApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "RosterApiError";
    this.status = status;
    this.code = code;
  }
}

async function getJson<T>(path: string): Promise<T> {
  return fetchJson<T>(path);
}

export async function loadBootstrap(): Promise<BootstrapData> {
  try {
    const [health, catalogs] = await Promise.all([
      getJson<HealthResponse>("/health"),
      getJson<CatalogResponse>("/catalogs"),
    ]);

    if (health.status !== "ok") {
      return demoBootstrap;
    }

    return { health, catalogs, source: "local" };
  } catch {
    // A bundled demo keeps design reviews and the installed static workbench
    // usable before the local SeatTrellis service has started.
    return demoBootstrap;
  }
}

export async function listRecentProjects(
  root = ".",
  limit = 20,
): Promise<ProjectListResponse> {
  const params = new URLSearchParams({ root, limit: String(limit) });
  return fetchJson<ProjectListResponse>(`/projects/recent?${params.toString()}`);
}

export async function fetchProjectHistory(
  projectPath: string,
  includeOutputs = true,
): Promise<ProjectHistoryResponse> {
  return fetchJson<ProjectHistoryResponse>("/projects/history", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_path: projectPath,
      include_outputs: includeOutputs,
    }),
  });
}

export async function compareProjectArtifacts(
  projectPath: string,
  artifactPath: string,
  compareToPath: string,
): Promise<ProjectArtifactCompareResponse> {
  return fetchJson<ProjectArtifactCompareResponse>("/projects/artifacts/compare", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_path: projectPath,
      artifact_path: artifactPath,
      compare_to_path: compareToPath,
    }),
  }, 30_000);
}

export async function restoreProjectArtifact(
  projectPath: string,
  artifactPath: string,
): Promise<ProjectArtifactRestoreResponse> {
  return fetchJson<ProjectArtifactRestoreResponse>("/projects/artifacts/restore", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_path: projectPath,
      artifact_path: artifactPath,
    }),
  }, 30_000);
}

export async function previewProjectMigration(
  projectPath: string,
  artifactPath?: string,
  inPlace = false,
): Promise<ProjectMigrationResponse> {
  return fetchJson<ProjectMigrationResponse>("/projects/migration/preview", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_path: projectPath,
      ...(artifactPath ? { artifact_path: artifactPath } : {}),
      in_place: inPlace,
    }),
  }, 30_000);
}

export async function applyProjectMigration(
  projectPath: string,
  artifactPath?: string,
  inPlace = false,
): Promise<ProjectMigrationResponse> {
  return fetchJson<ProjectMigrationResponse>("/projects/migration/apply", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_path: projectPath,
      ...(artifactPath ? { artifact_path: artifactPath } : {}),
      in_place: inPlace,
    }),
  }, 30_000);
}

export async function previewProjectMigrationBatch(
  projectPaths: string[],
  inPlace = false,
): Promise<ProjectMigrationBatchResponse> {
  return fetchJson<ProjectMigrationBatchResponse>("/projects/migration/batch/preview", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ project_paths: projectPaths, in_place: inPlace }),
  }, 30_000);
}

export async function applyProjectMigrationBatch(
  projectPaths: string[],
  inPlace = false,
): Promise<ProjectMigrationBatchResponse> {
  return fetchJson<ProjectMigrationBatchResponse>("/projects/migration/batch/apply", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ project_paths: projectPaths, in_place: inPlace }),
  }, 30_000);
}

export async function restoreProjectMigrationBackup(
  projectPath: string,
  sourcePath: string,
  backupPath: string,
): Promise<ProjectMigrationRestoreResponse> {
  return fetchJson<ProjectMigrationRestoreResponse>(
    "/projects/migration/restore",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        project_path: projectPath,
        source_path: sourcePath,
        backup_path: backupPath,
      }),
    },
    30_000,
  );
}

export async function saveProjectRotationPlan(
  projectPath: string,
  rotationPlan: RotationPlan,
  draftIds: string[],
  outputName?: string,
): Promise<ProjectRotationSaveResponse> {
  return fetchJson<ProjectRotationSaveResponse>(
    "/projects/rotation/save",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        project_path: projectPath,
        rotation_plan: rotationPlan,
        draft_ids: draftIds,
        ...(outputName ? { output_name: outputName } : {}),
      }),
    },
    30_000,
  );
}

export async function loadProjectRotationPlan(
  projectPath: string,
  artifactPath: string,
): Promise<ProjectRotationLoadResponse> {
  return fetchJson<ProjectRotationLoadResponse>(
    "/projects/rotation/load",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        project_path: projectPath,
        artifact_path: artifactPath,
      }),
    },
    30_000,
  );
}

export async function downloadProjectGroupRegister(
  projectPath: string,
  artifactPath: string,
  format: "html" | "csv" = "html",
  locale: "zh" | "en" = "zh",
): Promise<{ blob: Blob; filename: string }> {
  const headers = new Headers({ "Content-Type": "application/json" });
  const sessionToken = await ensureSessionToken();
  if (sessionToken) {
    headers.set("Authorization", `Bearer ${sessionToken}`);
  }
  const response = await fetch(`${API_ROOT}/projects/rotation/group-register`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      project_path: projectPath,
      artifact_path: artifactPath,
      format,
      locale,
    }),
  });
  if (!response.ok) {
    const detail = await safeErrorDetail(response);
    throw new RosterApiError(response.status, detail.code, detail.message);
  }
  const disposition = response.headers.get("Content-Disposition") ?? "";
  const match = /filename="([^"]+)"/.exec(disposition);
  return {
    blob: await response.blob(),
    filename: match ? match[1] : `group-register.${format}`,
  };
}

export async function previewProjectGroupRegister(
  projectPath: string,
  artifactPath: string,
): Promise<ProjectGroupRegisterPreviewResponse> {
  return fetchJson<ProjectGroupRegisterPreviewResponse>(
    "/projects/rotation/group-register/preview",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        project_path: projectPath,
        artifact_path: artifactPath,
      }),
    },
    30_000,
  );
}

export async function scanProjectPrivacy(
  projectPath: string,
  includeOutputs = true,
): Promise<ProjectPrivacyResponse> {
  return fetchJson<ProjectPrivacyResponse>("/projects/privacy", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_path: projectPath,
      include_outputs: includeOutputs,
    }),
  });
}

export async function downloadProjectBundle(
  projectPath: string,
  includeOutputs = true,
): Promise<{ blob: Blob; filename: string }> {
  const headers = new Headers({ "Content-Type": "application/json" });
  const sessionToken = await ensureSessionToken();
  if (sessionToken) {
    headers.set("Authorization", `Bearer ${sessionToken}`);
  }
  const response = await fetch(`${API_ROOT}/projects/bundle`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      project_path: projectPath,
      include_outputs: includeOutputs,
    }),
  });
  if (!response.ok) {
    const detail = await safeErrorDetail(response);
    throw new RosterApiError(response.status, detail.code, detail.message);
  }
  const disposition = response.headers.get("Content-Disposition") ?? "";
  const match = /filename="([^"]+)"/.exec(disposition);
  return {
    blob: await response.blob(),
    filename: match ? match[1] : "project.seattrellis.zip",
  };
}

export async function restoreProjectBundle(
  bundle: File,
  outputDir: string,
  overwrite = false,
): Promise<ProjectRestoreResponse> {
  const form = new FormData();
  form.append("bundle", bundle);
  form.append("output_dir", outputDir);
  form.append("overwrite", String(overwrite));
  return fetchJson<ProjectRestoreResponse>("/projects/restore", {
    method: "POST",
    body: form,
  }, 30_000);
}

export async function fetchRuleTemplates(): Promise<RuleTemplatesResponse> {
  return getJson<RuleTemplatesResponse>("/rules/templates");
}

/** Compile a filled sentence template into the canonical rule entry (D3). */
export async function compileRuleSentence(
  templateId: string,
  slots: Record<string, string | number>,
): Promise<CompiledRule> {
  return fetchJson<CompiledRule>("/rules/compile", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ template_id: templateId, slots }),
  });
}

export async function uploadRosterDraft(
  file: File,
): Promise<RosterDraftResponse> {
  const form = new FormData();
  form.append("file", file);
  return fetchJson<RosterDraftResponse>(
    "/rosters/drafts",
    { method: "POST", body: form },
    ROSTER_TIMEOUT_MS,
  );
}

/**
 * The canonical trusted root for typed paths (PD-D14). Manual path input is
 * relative to this directory; the backend refuses anything outside it.
 */
export async function fetchTrustedRoot(): Promise<string> {
  const data = await fetchJson<{ root: string }>("/files/root");
  return data.root;
}

/**
 * Read a file through the backend's trusted-root endpoint (PD-D14 entry ③).
 * The path must be relative; absolute paths and traversal are rejected both
 * client-side and by the server. Returns the bytes as a `File` so the rest
 * of the import flow stays on the multipart upload contract.
 */
export async function readTrustedFile(relPath: string): Promise<File> {
  const data = await fetchJson<{
    name: string;
    size: number;
    content_base64: string;
  }>("/files/read", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path: relPath }),
  });
  const binary = atob(data.content_base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return new File([bytes], data.name);
}

export async function fetchRosterDraft(
  draftId: string,
): Promise<RosterDraftResponse> {
  return fetchJson<RosterDraftResponse>(`/rosters/drafts/${draftId}`);
}

export async function previewRosterUpdate(
  draftId: string,
  request: RosterUpdatePreviewRequest,
): Promise<RosterUpdatePreviewResponse> {
  return fetchJson<RosterUpdatePreviewResponse>(
    `/rosters/drafts/${draftId}/preview`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    },
    ROSTER_TIMEOUT_MS,
  );
}

export async function deleteRosterDraft(draftId: string): Promise<void> {
  await fetchJson<void>(`/rosters/drafts/${draftId}`, { method: "DELETE" });
}

export async function createLayoutDraft(
  request: CreateLayoutDraftRequest,
): Promise<LayoutStateResponse> {
  return fetchJson<LayoutStateResponse>("/layouts/drafts", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  });
}

export async function dispatchLayoutCommand(
  draftId: string,
  command: LayoutCommand,
): Promise<LayoutStateResponse> {
  return fetchJson<LayoutStateResponse>(
    `/layouts/drafts/${encodeURIComponent(draftId)}/commands`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(command),
    },
  );
}

export async function compileLayoutDraft(
  draftId: string,
): Promise<CompiledLayoutResponse> {
  return fetchJson<CompiledLayoutResponse>(
    `/layouts/drafts/${encodeURIComponent(draftId)}/compiled`,
  );
}

export async function deleteLayoutDraft(draftId: string): Promise<void> {
  await fetchJson<void>(`/layouts/drafts/${encodeURIComponent(draftId)}`, {
    method: "DELETE",
  });
}

export async function generateClass(
  request: GenerateClassRequest,
): Promise<GenerateClassResponse> {
  return fetchJson<GenerateClassResponse>(
    "/classes/generate",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    },
    GENERATE_TIMEOUT_MS,
  );
}

export async function generateRotationPlan(
  request: GenerateRotationPlanRequest,
): Promise<GenerateRotationPlanResponse> {
  return fetchJson<GenerateRotationPlanResponse>(
    "/classes/rotation",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    },
    GENERATE_TIMEOUT_MS,
  );
}

export async function fetchEditorState(draftId: string): Promise<EditorState> {
  return fetchJson<EditorState>(`/editing/drafts/${draftId}`);
}

/** Recompute the score + hard-constraint audit for a draft (B5/D5, D6). */
export async function fetchDraftAudit(
  draftId: string,
): Promise<DraftAuditReport> {
  return getJson<DraftAuditReport>(`/editing/drafts/${draftId}/audit`);
}

export async function dispatchEditorCommand(
  command: EditorCommand,
): Promise<EditorState> {
  return fetchJson<EditorState>(
    `/editing/drafts/${command.draft_id}/commands`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(command),
    },
    10_000,
  );
}

export async function exportDraft(
  request: ExportDraftRequest,
): Promise<{ blob: Blob; filename: string }> {
  const headers = new Headers({ "Content-Type": "application/json" });
  const sessionToken = await ensureSessionToken();
  if (sessionToken) {
    headers.set("Authorization", `Bearer ${sessionToken}`);
  }
  const response = await fetch(`${API_ROOT}/exports`, {
    method: "POST",
    headers,
    body: JSON.stringify(request),
  });
  if (!response.ok) {
    const detail = await safeErrorDetail(response);
    throw new RosterApiError(response.status, detail.code, detail.message);
  }
  const blob = await response.blob();
  const disposition = response.headers.get("Content-Disposition") ?? "";
  const match = /filename="([^"]+)"/.exec(disposition);
  const filename = match ? match[1] : `seating.${request.format}`;
  return { blob, filename };
}

function readDesktopSessionToken(): string | null {
  if (typeof window === "undefined") {
    return null;
  }
  if (cachedDesktopSessionToken !== undefined) {
    return cachedDesktopSessionToken;
  }

  // The Tauri shell injects the token into the JS context at page load
  // (window.__SEATTRELLIS_SESSION__), so it never touches the URL or disk.
  const injected = (window as { __SEATTRELLIS_SESSION__?: unknown })
    .__SEATTRELLIS_SESSION__;
  if (typeof injected === "string" && injected) {
    cachedDesktopSessionToken = injected;
    return cachedDesktopSessionToken;
  }

  const fromUrl = new URLSearchParams(window.location.search).get("session");
  if (fromUrl) {
    // Some embedded WebView environments make sessionStorage unavailable. Keep
    // an in-memory copy so the initial authenticated API calls still work.
    cachedDesktopSessionToken = fromUrl;
    try {
      window.sessionStorage.setItem("seattrellis.desktop.session", fromUrl);
    } catch {
      // The in-memory token above is sufficient for this desktop window.
    }
    try {
      // The credential is needed only for the first page load. Removing it
      // from the visible URL avoids leaking it through copied links or logs.
      window.history.replaceState({}, document.title, window.location.pathname);
    } catch {
      // History APIs may be restricted by an embedded WebView; keep the URL.
    }
    return cachedDesktopSessionToken;
  }

  try {
    cachedDesktopSessionToken = window.sessionStorage.getItem(
      "seattrellis.desktop.session",
    );
  } catch {
    cachedDesktopSessionToken = null;
  }
  return cachedDesktopSessionToken;
}
