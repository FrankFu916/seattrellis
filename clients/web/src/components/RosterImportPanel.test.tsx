import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  fetchTrustedRoot,
  previewRosterUpdate,
  readTrustedFile,
  uploadRosterDraft,
} from "../api/client";
import {
  isTauriDesktop,
  isTrustedRelativePath,
  pickFileWithDialog,
} from "../domain/desktop";
import { createTranslator } from "../i18n/messages";
import { RosterImportPanel } from "./RosterImportPanel";

vi.mock("../api/client", () => ({
  fetchTrustedRoot: vi.fn(),
  previewRosterUpdate: vi.fn(),
  readTrustedFile: vi.fn(),
  uploadRosterDraft: vi.fn(),
  RosterApiError: class RosterApiError extends Error {},
}));

vi.mock("../domain/desktop", () => ({
  isTauriDesktop: vi.fn(() => false),
  isTrustedRelativePath: vi.fn(),
  pickFileWithDialog: vi.fn(),
}));

describe("RosterImportPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(isTauriDesktop).mockReturnValue(false);
    vi.mocked(isTrustedRelativePath).mockImplementation(
      (raw) => raw !== "" && !raw.startsWith("/") && !raw.includes(".."),
    );
    vi.mocked(fetchTrustedRoot).mockResolvedValue("/Users/teacher/SeatTrellis");
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
    expect(screen.getByRole("button", { name: "确认导入" })).toBeDisabled();

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

  it("uses the Tauri native dialog when the desktop bridge is available (PD-D14 entry ①)", async () => {
    const user = userEvent.setup();
    vi.mocked(isTauriDesktop).mockReturnValue(true);
    vi.mocked(pickFileWithDialog).mockResolvedValue(
      new File(["student_id,name\nS01,Alice\n"], "students.csv", {
        type: "text/csv",
      }),
    );
    const { container } = render(
      <RosterImportPanel
        locale="zh-CN"
        t={createTranslator("zh-CN")}
        currentStudents={[]}
        currentRevision={0}
        onImportConfirmed={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: "打开本机文件" }));

    await waitFor(() => {
      expect(pickFileWithDialog).toHaveBeenCalledWith(
        ["csv", "xlsx", "xls"],
        "表格文件",
      );
      expect(uploadRosterDraft).toHaveBeenCalledWith(expect.any(File));
    });
    const uploaded = vi.mocked(uploadRosterDraft).mock.calls[0]?.[0];
    expect(uploaded?.name).toBe("students.csv");
    expect(container.querySelector(".roster-mapping-section")).toBeInTheDocument();
  });

  it("reads a typed trusted-root path and uploads the result (PD-D14 entry ③)", async () => {
    const user = userEvent.setup();
    vi.mocked(readTrustedFile).mockResolvedValue(
      new File(["student_id,name\nS01,Alice\n"], "class-8-3.csv", {
        type: "text/csv",
      }),
    );
    const { container } = render(
      <RosterImportPanel
        locale="zh-CN"
        t={createTranslator("zh-CN")}
        currentStudents={[]}
        currentRevision={0}
        onImportConfirmed={vi.fn()}
      />,
    );

    await user.type(screen.getByLabelText("或输入相对路径"), "rosters/class-8-3.csv");
    await user.click(screen.getByRole("button", { name: "读取" }));

    await waitFor(() => {
      expect(readTrustedFile).toHaveBeenCalledWith("rosters/class-8-3.csv");
      expect(uploadRosterDraft).toHaveBeenCalledWith(expect.any(File));
    });
    expect(container.querySelector(".roster-mapping-section")).toBeInTheDocument();
  });

  it("rejects a typed path that escapes the trusted root before reading", async () => {
    const user = userEvent.setup();
    render(
      <RosterImportPanel
        locale="zh-CN"
        t={createTranslator("zh-CN")}
        currentStudents={[]}
        currentRevision={0}
        onImportConfirmed={vi.fn()}
      />,
    );

    await user.type(screen.getByLabelText("或输入相对路径"), "../outside.csv");
    await user.click(screen.getByRole("button", { name: "读取" }));

    expect(readTrustedFile).not.toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toHaveTextContent("相对路径");
  });

  it("accepts a dropped file (PD-D14 entry ②)", async () => {
    const file = new File(["student_id,name\nS01,Alice\n"], "dropped.csv", {
      type: "text/csv",
    });
    const { container } = render(
      <RosterImportPanel
        locale="zh-CN"
        t={createTranslator("zh-CN")}
        currentStudents={[]}
        currentRevision={0}
        onImportConfirmed={vi.fn()}
      />,
    );

    fireEvent.drop(container.querySelector(".file-picker") as HTMLElement, {
      dataTransfer: { files: [file] },
    });

    await waitFor(() => {
      expect(uploadRosterDraft).toHaveBeenCalledWith(file);
    });
  });

  it("keeps the mapping form open when preview fails", async () => {
    const user = userEvent.setup();
    vi.mocked(previewRosterUpdate).mockRejectedValueOnce(
      new Error("HTTP 500: internal solver details"),
    );
    const { container } = render(
      <RosterImportPanel
        locale="zh-CN"
        t={createTranslator("zh-CN")}
        currentStudents={[]}
        currentRevision={0}
        onImportConfirmed={vi.fn()}
      />,
    );

    const file = new File(["小林,18513806422"], "students.csv", {
      type: "text/csv",
    });
    await user.upload(container.querySelector('input[type="file"]') as HTMLInputElement, file);
    await user.click(await screen.findByRole("button", { name: "检查导入变化" }));

    expect(await screen.findByText("预览失败：名单操作没有完成，请检查文件后重试。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "检查导入变化" })).toBeInTheDocument();
    expect(screen.queryByText("HTTP 500: internal solver details")).not.toBeInTheDocument();
  });

  it("clears confirmation when the mapping changes after a preview", async () => {
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
    const file = new File(["小林,18513806422"], "students.csv", { type: "text/csv" });
    await user.upload(container.querySelector('input[type="file"]') as HTMLInputElement, file);
    await user.click(await screen.findByRole("button", { name: "检查导入变化" }));
    expect(screen.getByRole("button", { name: "确认导入" })).toBeInTheDocument();

    await user.selectOptions(screen.getAllByRole("combobox")[0], "student_id");
    expect(screen.getByRole("button", { name: "确认导入" })).toBeDisabled();
    expect(onImportConfirmed).not.toHaveBeenCalled();
  });

  it("does not apply a preview without resulting students", async () => {
    const user = userEvent.setup();
    const onImportConfirmed = vi.fn();
    vi.mocked(previewRosterUpdate).mockResolvedValueOnce({
      draft_id: "roster-1",
      base_revision: 0,
      mode: "incremental",
      can_apply: true,
      action_counts: { add: 0, update: 0, unchanged: 0, remove: 0, conflict: 0 },
      changes: [],
      conflicts: [],
      resulting_students: null,
    });
    const { container } = render(
      <RosterImportPanel
        locale="zh-CN"
        t={createTranslator("zh-CN")}
        currentStudents={[]}
        currentRevision={0}
        onImportConfirmed={onImportConfirmed}
      />,
    );
    const file = new File(["小林,18513806422"], "students.csv", { type: "text/csv" });
    await user.upload(container.querySelector('input[type="file"]') as HTMLInputElement, file);
    await user.click(await screen.findByRole("button", { name: "检查导入变化" }));
    expect(screen.getByRole("button", { name: "确认导入" })).toBeDisabled();
    expect(onImportConfirmed).not.toHaveBeenCalled();
  });
});
