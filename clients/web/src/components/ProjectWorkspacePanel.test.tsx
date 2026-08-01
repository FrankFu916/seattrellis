import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  applyProjectMigration,
  compareProjectArtifacts,
  fetchProjectHistory,
  listRecentProjects,
  loadProjectRotationPlan,
  downloadProjectGroupRegister,
  downloadProjectBundle,
  previewProjectMigration,
  restoreProjectBundle,
  restoreProjectArtifact,
  saveProjectRotationPlan,
  scanProjectPrivacy,
} from "../api/client";
import { createTranslator } from "../i18n/messages";
import { ProjectWorkspacePanel } from "./ProjectWorkspacePanel";

vi.mock("../api/client", () => ({
  applyProjectMigration: vi.fn(),
  compareProjectArtifacts: vi.fn(),
  downloadProjectBundle: vi.fn(),
  fetchProjectHistory: vi.fn(),
  listRecentProjects: vi.fn(),
  previewProjectMigration: vi.fn(),
  restoreProjectBundle: vi.fn(),
  restoreProjectArtifact: vi.fn(),
  loadProjectRotationPlan: vi.fn(),
  downloadProjectGroupRegister: vi.fn(),
  saveProjectRotationPlan: vi.fn(),
  RosterApiError: class RosterApiError extends Error {},
  scanProjectPrivacy: vi.fn(),
}));

const recentResponse = {
  api_version: "1" as const,
  root: "/classes",
  projects: [
    {
      name: "Demo Class",
      path: "/classes/demo.seattrellis.json",
      modified_at: "2026-08-01T00:00:00Z",
    },
  ],
};

const historyResponse = {
  api_version: "1" as const,
  project_name: "Demo Class",
  project_path: "/classes/demo.seattrellis.json",
  history: [
    {
      name: "week1.snapshot.json",
      path: "/classes/history/week1.snapshot.json",
      kind: "snapshot" as const,
      modified_at: "2026-08-01T00:00:00Z",
      created_at: "2026-08-01T00:00:00Z",
      size_bytes: 100,
      student_count: 30,
      period_count: null,
    },
    {
      name: "week2.snapshot.json",
      path: "/classes/history/week2.snapshot.json",
      kind: "snapshot" as const,
      modified_at: "2026-08-02T00:00:00Z",
      created_at: "2026-08-02T00:00:00Z",
      size_bytes: 110,
      student_count: 30,
      period_count: null,
    },
  ],
  outputs: [
    {
      name: "rotation-plan.json",
      path: "/classes/outputs/rotation-plan.json",
      kind: "rotation_plan" as const,
      modified_at: "2026-08-03T00:00:00Z",
      created_at: "2026-08-03T00:00:00Z",
      size_bytes: 420,
      student_count: 30,
      period_count: 2,
    },
  ],
  warnings: [],
};

const privacyResponse = {
  api_version: "1" as const,
  project_path: "/classes/demo.seattrellis.json",
  files_scanned: 2,
  safe_for_public_sharing: true,
  findings: [],
};

describe("ProjectWorkspacePanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listRecentProjects).mockResolvedValue(recentResponse);
    vi.mocked(fetchProjectHistory).mockResolvedValue(historyResponse);
    vi.mocked(downloadProjectBundle).mockResolvedValue({
      blob: new Blob(["backup"]),
      filename: "demo.seattrellis.zip",
    });
    vi.mocked(scanProjectPrivacy).mockResolvedValue(privacyResponse);
    vi.mocked(restoreProjectBundle).mockResolvedValue({
      api_version: "1",
      project_path: "/classes/restored/demo.seattrellis.json",
      output_dir: "/classes/restored",
    });
    vi.mocked(compareProjectArtifacts).mockResolvedValue({
      api_version: "1",
      left: {
        name: "week1.snapshot.json",
        path: "/classes/history/week1.snapshot.json",
        kind: "snapshot",
        created_at: "2026-08-01T00:00:00Z",
        student_count: 30,
        assignment_count: 30,
        enabled_seat_count: 30,
        solver_status: "FEASIBLE",
      },
      right: {
        name: "week1.snapshot.json",
        path: "/classes/history/week1.snapshot.json",
        kind: "snapshot",
        created_at: "2026-08-01T00:00:00Z",
        student_count: 30,
        assignment_count: 30,
        enabled_seat_count: 30,
        solver_status: "FEASIBLE",
      },
      diff: {
        assignment_changes: 4,
        roster_added: 1,
        roster_removed: 0,
        layout_changed: false,
        rules_changed: true,
        solver_status_changed: false,
        assignment_details: [
          {
            student_ref: "student-1",
            change: "moved",
            before_seat_id: "R1C1",
            after_seat_id: "R2C2",
          },
        ],
      },
    });
    vi.mocked(restoreProjectArtifact).mockResolvedValue({
      api_version: "1",
      project_path: "/classes/demo.seattrellis.json",
      source_artifact: "/classes/history/week1.snapshot.json",
      restored_artifact: "/classes/outputs/restored-week1.snapshot.json",
    });
    vi.mocked(previewProjectMigration).mockResolvedValue({
      api_version: "1",
      project_path: "/classes/demo.seattrellis.json",
      source_path: "/classes/demo.seattrellis.json",
      artifact: "project",
      schema_version: "1",
      output_path: "/classes/demo.seattrellis.migrated.json",
      backup_path: null,
      dry_run: true,
      before_valid: true,
      after_valid: null,
      rollback_available: true,
      change_count: 1,
      changes: [
        {
          path: "schema_version",
          change: "changed",
          before_type: "string",
          after_type: "string",
        },
      ],
    });
    vi.mocked(applyProjectMigration).mockResolvedValue({
      api_version: "1",
      project_path: "/classes/demo.seattrellis.json",
      source_path: "/classes/demo.seattrellis.json",
      artifact: "project",
      schema_version: "1",
      output_path: "/classes/demo.seattrellis.migrated.json",
      backup_path: null,
      dry_run: false,
      before_valid: true,
      after_valid: true,
      rollback_available: true,
      change_count: 1,
      changes: [
        {
          path: "schema_version",
          change: "changed",
          before_type: "string",
          after_type: "string",
        },
      ],
    });
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:backup");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("loads a recent project and shows its history without student data", async () => {
    render(<ProjectWorkspacePanel locale="en" t={createTranslator("en")} />);

    await waitFor(() => {
      expect(screen.getByTestId("project-select")).toHaveValue(
        "/classes/demo.seattrellis.json",
      );
      expect(screen.getByTestId("project-compare-right")).toHaveValue(
        "/classes/history/week2.snapshot.json",
      );
    });
    expect(screen.getAllByText("Seating plan")).toHaveLength(2);
    expect(screen.getAllByText("week1.snapshot.json").length).toBeGreaterThan(0);
    expect(screen.queryByText("Alice")).not.toBeInTheDocument();
  });

  it("downloads a selected project backup", async () => {
    const user = userEvent.setup();
    render(<ProjectWorkspacePanel locale="en" t={createTranslator("en")} />);

    await waitFor(() => {
      expect(screen.getByTestId("project-select")).toHaveValue(
        "/classes/demo.seattrellis.json",
      );
    });
    await user.click(screen.getByTestId("project-backup-button"));

    await waitFor(() => {
      expect(downloadProjectBundle).toHaveBeenCalledWith(
        "/classes/demo.seattrellis.json",
      );
    });
    expect(screen.getByTestId("project-status")).toHaveTextContent(
      "Backup downloaded: demo.seattrellis.zip",
    );
  });

  it("runs a privacy check and restores an uploaded bundle", async () => {
    const user = userEvent.setup();
    render(<ProjectWorkspacePanel locale="en" t={createTranslator("en")} />);

    await waitFor(() => {
      expect(screen.getByTestId("project-select")).toHaveValue(
        "/classes/demo.seattrellis.json",
      );
    });
    await user.click(screen.getByTestId("project-privacy-button"));
    expect(await screen.findByTestId("project-privacy-status")).toHaveTextContent(
      "No sensitive fields found",
    );

    const file = new File(["bundle"], "demo.seattrellis.zip", { type: "application/zip" });
    await user.upload(screen.getByTestId("project-restore-file"), file);
    await user.click(screen.getByTestId("project-restore-button"));
    await waitFor(() => {
      expect(restoreProjectBundle).toHaveBeenCalledWith(
        file,
        "./restored-project",
      );
    });
    expect(screen.getByTestId("project-status")).toHaveTextContent(
      "Project restored to /classes/restored/demo.seattrellis.json",
    );
  });

  it("compares history and creates a safe restored artifact", async () => {
    const user = userEvent.setup();
    render(<ProjectWorkspacePanel locale="en" t={createTranslator("en")} />);

    await waitFor(() => {
      expect(screen.getByTestId("project-select")).toHaveValue(
        "/classes/demo.seattrellis.json",
      );
    });
    await user.click(screen.getByTestId("project-compare-button"));
    await waitFor(() => {
      expect(compareProjectArtifacts).toHaveBeenCalledWith(
        "/classes/demo.seattrellis.json",
        "/classes/history/week1.snapshot.json",
        "/classes/history/week2.snapshot.json",
      );
    });
    expect(screen.getByTestId("project-compare-result")).toHaveTextContent(
      "Seat changes: 4",
    );
    await user.click(
      screen.getByTestId("project-assignment-details").querySelector("summary")!,
    );
    expect(screen.getByTestId("project-assignment-details")).toHaveTextContent(
      "student-1",
    );
    await user.click(screen.getByTestId("project-restore-artifact-button"));
    await waitFor(() => {
      expect(restoreProjectArtifact).toHaveBeenCalledWith(
        "/classes/demo.seattrellis.json",
        "/classes/history/week1.snapshot.json",
      );
    });
  });

  it("previews and writes a project schema migration", async () => {
    const user = userEvent.setup();
    render(<ProjectWorkspacePanel locale="en" t={createTranslator("en")} />);

    await waitFor(() => {
      expect(screen.getByTestId("project-select")).toHaveValue(
        "/classes/demo.seattrellis.json",
      );
    });
    await user.click(screen.getByTestId("project-migration-preview"));
    await waitFor(() => {
      expect(previewProjectMigration).toHaveBeenCalledWith(
        "/classes/demo.seattrellis.json",
        undefined,
        false,
      );
    });
    expect(screen.getByTestId("project-migration-result")).toHaveTextContent(
      "Migration check passed",
    );
    expect(screen.getByTestId("project-migration-result")).toHaveTextContent(
      "1 field changes detected",
    );
    expect(screen.getByTestId("project-migration-details")).toHaveTextContent(
      "schema_version",
    );
    await user.click(screen.getByTestId("project-migration-apply"));
    await waitFor(() => {
      expect(applyProjectMigration).toHaveBeenCalledWith(
        "/classes/demo.seattrellis.json",
        undefined,
        false,
      );
    });
    expect(screen.getByTestId("project-status")).toHaveTextContent(
      "Migration written to: /classes/demo.seattrellis.migrated.json",
    );
  });

  it("saves the current rotation drafts to the selected project", async () => {
    const user = userEvent.setup();
    const rotationPlan = {
      kind: "rotation_plan" as const,
      name: "Weekly rotation",
      periods: [
        {
          period: 1,
          label: "Monday",
          snapshot: { assignments: [], solver_status: "feasible" },
        },
        {
          period: 2,
          label: "Friday",
          snapshot: { assignments: [], solver_status: "feasible" },
        },
      ],
      base_history_count: 0,
      fairness_summary: {},
      pair_repeat_summary: {},
      warnings: [],
    };
    vi.mocked(saveProjectRotationPlan).mockResolvedValue({
      api_version: "1",
      project_path: "/classes/demo.seattrellis.json",
      output_path: "/classes/outputs/rotation-plan.json",
      period_count: 2,
      saved_at: "2026-08-01T00:00:00Z",
    });
    render(
      <ProjectWorkspacePanel
        locale="en"
        t={createTranslator("en")}
        rotationPlan={rotationPlan}
        rotationDraftIds={["period-one", "period-two"]}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("project-select")).toHaveValue(
        "/classes/demo.seattrellis.json",
      );
    });
    await user.click(screen.getByTestId("project-rotation-save-button"));
    await waitFor(() => {
      expect(saveProjectRotationPlan).toHaveBeenCalledWith(
        "/classes/demo.seattrellis.json",
        rotationPlan,
        ["period-one", "period-two"],
      );
    });
    expect(screen.getByTestId("project-status")).toHaveTextContent(
      "Rotation plan saved to: /classes/outputs/rotation-plan.json",
    );
  });

  it("opens a saved rotation in the editing workflow", async () => {
    const user = userEvent.setup();
    const onRotationLoad = vi.fn();
    vi.mocked(loadProjectRotationPlan).mockResolvedValue({
      api_version: "1",
      project_path: "/classes/demo.seattrellis.json",
      artifact_path: "/classes/outputs/rotation-plan.json",
      rotation_plan: {
        kind: "rotation_plan",
        name: "Weekly rotation",
        periods: [],
        base_history_count: 0,
        fairness_summary: {},
        pair_repeat_summary: {},
        warnings: [],
      },
      editor: {} as never,
      period_editors: [{} as never],
    });
    render(
      <ProjectWorkspacePanel
        locale="en"
        t={createTranslator("en")}
        onRotationLoad={onRotationLoad}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("project-open-rotation-select")).toHaveValue(
        "/classes/outputs/rotation-plan.json",
      );
    });
    await user.click(screen.getByTestId("project-open-rotation-button"));
    await waitFor(() => {
      expect(loadProjectRotationPlan).toHaveBeenCalledWith(
        "/classes/demo.seattrellis.json",
        "/classes/outputs/rotation-plan.json",
      );
      expect(onRotationLoad).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByTestId("project-status")).toHaveTextContent(
      "Rotation loaded: Weekly rotation",
    );
  });

  it("downloads a group register for a saved rotation", async () => {
    const user = userEvent.setup();
    vi.mocked(downloadProjectGroupRegister).mockResolvedValue({
      blob: new Blob(["Period,Group"]),
      filename: "group-register.html",
    });
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:register");
    render(<ProjectWorkspacePanel locale="en" t={createTranslator("en")} />);

    await waitFor(() => {
      expect(screen.getByTestId("project-group-register-button")).toBeEnabled();
    });
    await user.click(screen.getByTestId("project-group-register-button"));
    await waitFor(() => {
      expect(downloadProjectGroupRegister).toHaveBeenCalledWith(
        "/classes/demo.seattrellis.json",
        "/classes/outputs/rotation-plan.json",
        "html",
        "en",
      );
    });
    expect(screen.getByTestId("project-status")).toHaveTextContent(
      "Register downloaded: group-register.html",
    );
  });
});
