import { demoBootstrap } from "./demo";
import type {
  BootstrapData,
  CatalogResponse,
  HealthResponse,
  RosterDraftResponse,
  RosterUpdatePreviewRequest,
  RosterUpdatePreviewResponse,
} from "./types";

const API_ROOT = "/api/v1";
const REQUEST_TIMEOUT_MS = 1800;
const ROSTER_TIMEOUT_MS = 30_000;

async function fetchJson<T>(
  path: string,
  init?: RequestInit,
  timeoutMs = REQUEST_TIMEOUT_MS,
): Promise<T> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(`${API_ROOT}${path}`, {
      headers: { Accept: "application/json" },
      signal: controller.signal,
      ...init,
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

