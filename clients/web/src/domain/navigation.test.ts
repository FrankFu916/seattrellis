import { describe, expect, it } from "vitest";

import {
  contextActionFor,
  contentViews,
  isContentView,
  viewToStep,
  type WorkbenchView,
} from "./navigation";

describe("viewToStep", () => {
  it("maps the sidebar views to their legacy panels", () => {
    expect(viewToStep("roster")).toBe("roster");
    expect(viewToStep("room")).toBe("room");
    expect(viewToStep("rules")).toBe("goal");
    expect(viewToStep("generate")).toBe("generate");
    expect(viewToStep("canvas")).toBe("adjust");
    expect(viewToStep("export")).toBe("export");
  });

  it("rejects the history view (own panel)", () => {
    expect(() => viewToStep("history")).toThrow(/no legacy panel step/);
  });
});

describe("isContentView", () => {
  it("accepts exactly the sidebar content views", () => {
    expect(contentViews).toEqual(["roster", "room", "rules", "history"]);
    for (const view of contentViews) {
      expect(isContentView(view)).toBe(true);
    }
    expect(isContentView("generate")).toBe(false);
    expect(isContentView("canvas")).toBe(false);
    expect(isContentView("export")).toBe(false);
  });
});

describe("contextActionFor", () => {
  it("keeps the linear wizard guidance across content views", () => {
    expect(contextActionFor("roster", false)).toMatchObject({
      kind: "navigate",
      target: "room",
    });
    expect(contextActionFor("room", false)).toMatchObject({
      kind: "navigate",
      target: "rules",
    });
    expect(contextActionFor("rules", false)).toMatchObject({
      kind: "navigate",
      target: "generate",
    });
  });

  it("offers generation on the generate view", () => {
    expect(contextActionFor("generate", false)).toEqual({
      kind: "generate",
      label: "action.generate",
    });
  });

  it("offers the export menu on the canvas view", () => {
    expect(contextActionFor("canvas", true)).toEqual({ kind: "exportMenu" });
  });

  it("keeps preview on the export settings view", () => {
    expect(contextActionFor("export", true)).toEqual({
      kind: "preview",
      label: "action.preview",
    });
  });

  it("switches the history view on plan availability", () => {
    expect(contextActionFor("history", false)).toMatchObject({
      kind: "navigate",
      target: "generate",
    });
    expect(contextActionFor("history", true)).toEqual({ kind: "exportMenu" });
  });

  it("covers every workbench view", () => {
    const views: WorkbenchView[] = [
      "roster",
      "room",
      "rules",
      "history",
      "generate",
      "canvas",
      "export",
    ];
    for (const view of views) {
      expect(contextActionFor(view, false)).toBeDefined();
    }
  });
});
