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
});
