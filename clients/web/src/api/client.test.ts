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
