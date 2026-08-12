import type { MessageKey } from "../i18n/messages";
import type { WorkflowStep } from "./workflow";

/**
 * Navigation model for the fused class shell (M4 PD-D1-NAVIGATION).
 *
 * The sidebar owns four class-content views; "generate", "canvas" and
 * "export" are transient views reached from the context action bar (the
 * wizard's guidance survives there, not as a second navigation system).
 */
export const contentViews = ["roster", "room", "rules", "history"] as const;
export type ContentView = (typeof contentViews)[number];

export const transientViews = ["generate", "canvas", "export"] as const;
export type TransientView = (typeof transientViews)[number];

export type WorkbenchView = ContentView | TransientView;

/** A class context is a named workspace; the scratch workspace has none. */
export type ClassContext =
  | { kind: "class"; id: string; name: string }
  | { kind: "temp" };

/** Client-side class entry created by "save as class" (G-5, session-scoped). */
export type SessionClass = {
  id: string;
  name: string;
};

/** The "next step" affordance offered by the context action bar (D1). */
export type ContextAction =
  | { kind: "navigate"; target: WorkbenchView; label: MessageKey }
  | { kind: "generate"; label: MessageKey }
  | { kind: "preview"; label: MessageKey }
  | { kind: "exportMenu" };

/** Map a workbench view to the legacy panel step it renders. */
export function viewToStep(view: WorkbenchView): WorkflowStep {
  switch (view) {
    case "roster":
      return "roster";
    case "room":
      return "room";
    case "rules":
      return "goal";
    case "generate":
      return "generate";
    case "canvas":
      return "adjust";
    case "export":
      return "export";
    case "history":
      throw new Error("history has no legacy panel step");
  }
}

export function isContentView(view: WorkbenchView): view is ContentView {
  return (contentViews as readonly string[]).includes(view);
}

/**
 * The primary action for the current view (D1 context bar). The wizard's
 * linear guidance is preserved: roster → room → rules → generate; after a
 * plan exists the bar offers export instead of navigation.
 */
export function contextActionFor(
  view: WorkbenchView,
  hasPlan: boolean,
): ContextAction {
  switch (view) {
    case "roster":
      return { kind: "navigate", target: "room", label: "ctx.nextRoom" };
    case "room":
      return { kind: "navigate", target: "rules", label: "ctx.nextRules" };
    case "rules":
      return { kind: "navigate", target: "generate", label: "ctx.nextGenerate" };
    case "generate":
      return { kind: "generate", label: "action.generate" };
    case "canvas":
      return { kind: "exportMenu" };
    case "export":
      return { kind: "preview", label: "action.preview" };
    case "history":
      return hasPlan
        ? { kind: "exportMenu" }
        : { kind: "navigate", target: "generate", label: "ctx.nextGenerate" };
  }
}
