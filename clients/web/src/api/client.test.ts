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

describe("native desktop file bridge", () => {
  afterEach(() => {
    delete window.pywebview;
    vi.resetModules();
  });

  it("saves an export through pywebview when available", async () => {
    const save = vi.fn().mockResolvedValue({ saved: true, name: "plan.html" });
    window.pywebview = { api: { save_export_file: save } };

    const { saveDesktopExport } = await import("./client");
    const blob = {
      arrayBuffer: async () => new TextEncoder().encode("hello").buffer,
    } as Blob;
    const saved = await saveDesktopExport("plan.html", blob);

    expect(saved).toBe(true);
    expect(save).toHaveBeenCalledWith("plan.html", btoa("hello"));
  });

  it("falls back to browser downloads when no native bridge exists", async () => {
    const { saveDesktopExport, hasDesktopBridge } = await import("./client");

    expect(hasDesktopBridge()).toBe(false);
    expect(await saveDesktopExport("plan.html", {} as Blob)).toBe(false);
  });
});
