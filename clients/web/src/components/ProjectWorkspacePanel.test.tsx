import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  fetchProjectHistory,
  listRecentProjects,
  downloadProjectBundle,
  restoreProjectBundle,
  scanProjectPrivacy,
} from "../api/client";
import { createTranslator } from "../i18n/messages";
import { ProjectWorkspacePanel } from "./ProjectWorkspacePanel";

vi.mock("../api/client", () => ({
  downloadProjectBundle: vi.fn(),
  fetchProjectHistory: vi.fn(),
  listRecentProjects: vi.fn(),
  restoreProjectBundle: vi.fn(),
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
  ],
  outputs: [],
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
    });
    expect(screen.getByText("Seating plan")).toBeInTheDocument();
    expect(screen.getByText("week1.snapshot.json")).toBeInTheDocument();
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
});
