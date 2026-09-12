import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const sessionStorageDescriptor = Object.getOwnPropertyDescriptor(window, "sessionStorage");

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
