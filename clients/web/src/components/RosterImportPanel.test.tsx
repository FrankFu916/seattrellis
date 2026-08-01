import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { previewRosterUpdate, uploadRosterDraft } from "../api/client";
import { createTranslator } from "../i18n/messages";
import { RosterImportPanel } from "./RosterImportPanel";

vi.mock("../api/client", () => ({
  previewRosterUpdate: vi.fn(),
  uploadRosterDraft: vi.fn(),
  RosterApiError: class RosterApiError extends Error {},
}));

describe("RosterImportPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(uploadRosterDraft).mockResolvedValue({
      draft_id: "roster-1",
      source_format: "csv",
      headerless: true,
      row_count: 2,
      column_count: 2,
      columns: [
        { index: 0, header: "Column 1" },
        { index: 1, header: "Column 2" },
      ],
      preview_rows: [
        { row_number: 1, cells: ["小林", "18513806422"] },
        { row_number: 2, cells: ["小周", "18513806423"] },
      ],
      suggested_mapping: [
        { field: "student_id", column_index: 1 },
        { field: "name", column_index: 0 },
      ],
      mapping_issues: [],
    });
    vi.mocked(previewRosterUpdate).mockResolvedValue({
      draft_id: "roster-1",
      base_revision: 0,
      mode: "incremental",
      can_apply: true,
      action_counts: { add: 2, update: 0, unchanged: 0, remove: 0, conflict: 0 },
      changes: [],
      conflicts: [],
      resulting_students: [
        { student_id: "18513806422", name: "小林" },
        { student_id: "18513806423", name: "小周" },
      ],
    });
  });

  it("keeps a headerless file, localizes mapping labels, and exposes confirmation inline", async () => {
    const user = userEvent.setup();
    const onImportConfirmed = vi.fn();
    const { container } = render(
      <RosterImportPanel
        locale="zh-CN"
        t={createTranslator("zh-CN")}
        currentStudents={[]}
        currentRevision={0}
        onImportConfirmed={onImportConfirmed}
      />,
    );

    const file = new File(["小林,18513806422"], "students.csv", {
      type: "text/csv",
    });
    await user.upload(container.querySelector('input[type="file"]') as HTMLInputElement, file);

    expect(await screen.findByText("没有检测到表头")).toBeInTheDocument();
    expect(screen.getAllByRole("option", { name: "姓名" })).toHaveLength(2);
    expect(screen.queryByText("name")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "检查导入变化" }));
    expect(await screen.findByText("可以安全导入")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "确认导入" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "打开导出预览" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "确认导入" }));
    await waitFor(() => {
      expect(onImportConfirmed).toHaveBeenCalledWith([
        { id: "18513806422", name: "小林" },
        { id: "18513806423", name: "小周" },
      ]);
    });
  });
});
