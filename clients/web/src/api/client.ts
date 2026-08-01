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
  ProjectMigrationResponse,
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
} from "./types";

const API_ROOT = "/api/v1";
const REQUEST_TIMEOUT_MS = 1800;
const ROSTER_TIMEOUT_MS = 30_000;
const GENERATE_TIMEOUT_MS = 30_000;
let cachedDesktopSessionToken: string | null | undefined;

export const EDITOR_PROTOCOL_VERSION = "1.0";

async function fetchJson<T>(
  path: string,
  init?: RequestInit,
  timeoutMs = REQUEST_TIMEOUT_MS,
): Promise<T> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);

  const headers = new Headers(init?.headers);
  headers.set("Accept", "application/json");
  const sessionToken = readDesktopSessionToken();
  if (sessionToken) {
    headers.set("Authorization", `Bearer ${sessionToken}`);
  }

  try {
    const response = await fetch(`${API_ROOT}${path}`, {
      ...init,
      headers,
      signal: controller.signal,
    });
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
  const sessionToken = readDesktopSessionToken();
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
  const sessionToken = readDesktopSessionToken();
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
