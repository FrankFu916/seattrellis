import { describe, expect, it, vi } from "vitest";

import { isTrustedRelativePath, isTauriDesktop } from "./desktop";

describe("isTrustedRelativePath (PD-D14)", () => {
  it("accepts plain relative paths", () => {
    expect(isTrustedRelativePath("rosters/class-8-3.csv")).toBe(true);
    expect(isTrustedRelativePath("class-8-3.csv")).toBe(true);
    expect(isTrustedRelativePath("a/b/c.csv")).toBe(true);
    expect(isTrustedRelativePath("rosters/./class.csv")).toBe(true);
  });

  it("rejects absolute paths and drive prefixes", () => {
    expect(isTrustedRelativePath("/etc/passwd")).toBe(false);
    expect(isTrustedRelativePath("/rosters/a.csv")).toBe(false);
    expect(isTrustedRelativePath("C:/windows/win.ini")).toBe(false);
    expect(isTrustedRelativePath("C:rosters/a.csv")).toBe(false);
    expect(isTrustedRelativePath("c:\\rosters\\a.csv")).toBe(false);
  });

  it("rejects traversal and empty input", () => {
    expect(isTrustedRelativePath("../secrets.csv")).toBe(false);
    expect(isTrustedRelativePath("rosters/../../secrets.csv")).toBe(false);
    expect(isTrustedRelativePath("")).toBe(false);
    expect(isTrustedRelativePath(".")).toBe(false);
    expect(isTrustedRelativePath("a\0b.csv")).toBe(false);
    expect(isTrustedRelativePath("rosters\\a.csv")).toBe(false);
  });
});

describe("isTauriDesktop", () => {
  it("is false in a plain browser", () => {
    expect(isTauriDesktop()).toBe(false);
  });

  it("is true when the Tauri shell injected its internals", () => {
    vi.stubGlobal("__TAURI_INTERNALS__", { invoke: vi.fn() });
    expect(isTauriDesktop()).toBe(true);
    vi.unstubAllGlobals();
  });
});
