import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildExportRequest } from "../domain/export";

const sessionStorageDescriptor = Object.getOwnPropertyDescriptor(window, "sessionStorage");

describe("export transport", () => {
  const request = buildExportRequest({ format: "pdf", anonymize: false, showStudentIds: false, paper: "a4", orientation: "landscape", margin: 12 }, "draft-1", 3, "Class", "en");
  beforeEach(() => {
    vi.resetModules();
    window.history.replaceState({}, "", "/?session=export-session");
    window.sessionStorage.clear();
  });
  afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

  it("refreshes a stale session once and returns decoded quality warnings", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response("{}", { status: 401 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ session_token: "fresh" }), { status: 200 }))
      .mockResolvedValueOnce(new Response("%PDF-result", { status: 200, headers: { "Content-Type": "application/pdf", "Content-Disposition": 'attachment; filename="seat-plan.pdf"', "X-Export-Warnings": encodeURIComponent(JSON.stringify(["字体提醒", 1])) } }));
    vi.stubGlobal("fetch", fetchMock);
    const { exportDraft } = await import("./client");
    const result = await exportDraft(request);
    expect(result.filename).toBe("seat-plan.pdf");
    expect(result.warnings).toEqual(["字体提醒"]);
    expect(await result.blob.text()).toBe("%PDF-result");
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(new Headers(fetchMock.mock.calls[2][1].headers).get("Authorization")).toBe("Bearer fresh");
  });

  it("passes cancellation through both export endpoints and removes its timeout", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn((_url: string, init: RequestInit) => new Promise<Response>((_resolve, reject) => init.signal?.addEventListener("abort", () => reject(new DOMException("cancelled", "AbortError")))));
    vi.stubGlobal("fetch", fetchMock);
    const { exportDraft, previewExportDraft } = await import("./client");
    for (const call of [exportDraft, previewExportDraft]) {
      const controller = new AbortController();
      const result = call(request, controller.signal);
      const expectation = expect(result).rejects.toMatchObject({ name: "AbortError" });
      await vi.advanceTimersByTimeAsync(0);
      controller.abort();
      await expectation;
      expect(vi.getTimerCount()).toBe(0);
    }
    expect(fetchMock.mock.calls[1][0]).toBe("/api/v1/exports/preview");
  });

  it("times out a stalled export with a recoverable error", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("fetch", vi.fn((_url: string, init: RequestInit) => new Promise<Response>((_resolve, reject) => init.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError"))))));
    const { exportDraft } = await import("./client");
    const expectation = expect(exportDraft(request)).rejects.toMatchObject({ name: "TimeoutError" });
    await vi.advanceTimersByTimeAsync(60_000);
    await expectation;
    expect(vi.getTimerCount()).toBe(0);
  });

  it("tolerates malformed optional warning headers", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("ok", { headers: { "X-Export-Warnings": "%invalid" } })));
    const { exportDraft } = await import("./client");
    expect((await exportDraft(request)).warnings).toEqual([]);
  });

  it("preserves document-preview warnings alongside the preview blob", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("<svg />", { headers: { "Content-Type": "image/svg+xml", "X-Export-Warnings": encodeURIComponent(JSON.stringify(["Preview font missing"])) } })));
    const { previewExportDraft } = await import("./client");
    const result = await previewExportDraft(request);
    expect(result.blob.type).toBe("image/svg+xml");
    expect(result.warnings).toEqual(["Preview font missing"]);
  });
});

describe("desktop session handoff", () => {
  beforeEach(() => {
    vi.resetModules();
    window.history.replaceState({}, "", "/?session=desktop-session");
    Object.defineProperty(window, "sessionStorage", {
      configurable: true,
      get() {
        throw new Error("sessionStorage is unavailable in this embedded window");
      },
    });
  });

  afterEach(() => {
    if (sessionStorageDescriptor) {
      Object.defineProperty(window, "sessionStorage", sessionStorageDescriptor);
    }
    vi.unstubAllGlobals();
  });

  it("keeps the URL token in memory when sessionStorage is unavailable", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({}), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const { fetchEditorState } = await import("./client");
    await fetchEditorState("draft-1");

    const request = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(new Headers(request.headers).get("Authorization")).toBe(
      "Bearer desktop-session",
    );
  });

  it("surfaces the server ErrorEnvelope error field", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ error: "Artifact is outside the project." }), {
          status: 422,
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );

    const { fetchEditorState } = await import("./client");
    await expect(fetchEditorState("draft-1")).rejects.toMatchObject({
      status: 422,
      message: "Artifact is outside the project.",
    });
  });
});

describe("session token re-bootstrap", () => {
  beforeEach(() => {
    vi.resetModules();
    window.history.replaceState({}, "", "/");
    window.sessionStorage.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("bounds a stalled session bootstrap and recovers on the next request", async () => {
    vi.useFakeTimers();
    let stall = true;
    const fetchMock = vi.fn((input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      if (String(input).endsWith("/session") && stall) {
        return new Promise((_, reject) => {
          init?.signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")));
        });
      }
      if (init?.signal?.aborted) return Promise.reject(new DOMException("Aborted", "AbortError"));
      return Promise.resolve(new Response(JSON.stringify(String(input).endsWith("/session")
        ? { session_token: "recovered" } : { draft_id: "draft-1" }), { status: 200 }));
    });
    vi.stubGlobal("fetch", fetchMock);
    const { fetchEditorState } = await import("./client");
    const failed = expect(fetchEditorState("draft-1")).rejects.toMatchObject({ name: "AbortError" });
    await vi.advanceTimersByTimeAsync(1800);
    await failed;
    expect(vi.getTimerCount()).toBe(0);
    stall = false;
    await expect(fetchEditorState("draft-1")).resolves.toMatchObject({ draft_id: "draft-1" });
    expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/session"))).toHaveLength(2);
    expect(new Headers(fetchMock.mock.calls.at(-1)?.[1]?.headers).get("Authorization")).toBe("Bearer recovered");
    expect(vi.getTimerCount()).toBe(0);
  });

  it("shares session bootstrap across concurrent editor requests", async () => {
    let resolveSession!: (response: Response) => void;
    const pending = new Promise<Response>((resolve) => { resolveSession = resolve; });
    const fetchMock = vi.fn((input: RequestInfo | URL) => String(input).endsWith("/session")
      ? pending : Promise.resolve(new Response("{}", { status: 200 })));
    vi.stubGlobal("fetch", fetchMock);
    const { fetchEditorState } = await import("./client");
    const requests = [fetchEditorState("a"), fetchEditorState("b")];
    expect(fetchMock).toHaveBeenCalledTimes(1);
    resolveSession(new Response(JSON.stringify({ session_token: "shared" }), { status: 200 }));
    await Promise.all(requests);
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it("re-bootstraps once after a 401 even when no token was known", async () => {
    // The service was down during bootstrap (call 0), so the first API call
    // (call 1) goes out unauthenticated and is rejected; a fresh session
    // (call 2) is then bootstrapped and the call retried with it (call 3).
    const responses = [
      new Response(JSON.stringify({}), { status: 503 }),
      new Response(JSON.stringify({}), { status: 401 }),
      new Response(JSON.stringify({ session_token: "fresh-token" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
      new Response(JSON.stringify({}), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    ];
    // Snapshot the Authorization header per call: fetchJson reuses and
    // mutates the same Headers object when it retries, so reading the init
    // afterwards would show the final state for every call.
    const seen: Array<string | null> = [];
    const fetchMock = vi.fn(
      (input: RequestInfo | URL, init?: RequestInit) => {
        seen.push(new Headers(init?.headers).get("Authorization"));
        return Promise.resolve(responses.shift() ?? new Response(null, { status: 500 }));
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    const { fetchEditorState } = await import("./client");
    await expect(fetchEditorState("draft-1")).resolves.toBeDefined();

    expect(seen).toEqual([null, null, null, "Bearer fresh-token"]);
  });
});
