import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api/client";
import type { CatalogOption } from "../api/types";
import * as desktop from "../domain/desktop";
import { createTranslator } from "../i18n/messages";
import { ExportWorkspace } from "./ExportWorkspace";

vi.mock("../api/client", async (original) => ({
  ...(await original<typeof api>()),
  exportDraft: vi.fn(),
  previewExportDraft: vi.fn(),
}));
vi.mock("../domain/desktop", () => ({ saveBlobWithDialog: vi.fn() }));

const formats: CatalogOption[] = [
  "print-html",
  "pdf",
  "svg",
  "docx",
  "pptx",
  "xlsx",
].map((id) => ({
  id,
  name: { "zh-CN": id, en: id },
  description: { "zh-CN": `${id} description`, en: `${id} description` },
}));
const t = createTranslator("en");
const props = {
  draftId: "draft-a",
  revision: 3,
  title: "Class A",
  formats,
  locale: "en" as const,
  t,
  onExported: vi.fn(),
};
const file = () => ({
  blob: new Blob(
    [
      '<!doctype html><title>Class A</title><main><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 70"><text x="10" y="20">Class A</text></svg></main>',
    ],
    {
      type: "text/html",
    },
  ),
  filename: "seat-plan.print.html",
  warnings: [] as string[],
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL = vi.fn(() => "blob:prepared-file");
      static revokeObjectURL = vi.fn();
    },
  );
  vi.mocked(api.exportDraft).mockResolvedValue(file());
  vi.mocked(api.previewExportDraft).mockResolvedValue({
    blob: new Blob(["<svg />"], { type: "image/svg+xml" }),
    warnings: [],
  });
  vi.mocked(desktop.saveBlobWithDialog).mockResolvedValue("saved");
});
afterEach(() => vi.unstubAllGlobals());

describe("ExportWorkspace", () => {
  it.each(["print-html", "pdf"])(
    "invalidates a %s preview that cannot decode and allows a fresh attempt",
    async (format) => {
      const user = userEvent.setup();
      render(<ExportWorkspace {...props} />);
      await user.selectOptions(
        screen.getByRole("combobox", { name: "File format" }),
        format,
      );
      await user.click(
        screen.getByRole("button", { name: "Generate preview" }),
      );
      const image = await screen.findByRole("img", {
        name: "Seating chart in the generated file",
      });
      fireEvent.error(image);
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "preview image could not be displayed",
      );
      expect(screen.queryByRole("img")).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Save file" })).toBeDisabled();
      expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:prepared-file");
      await user.click(screen.getByRole("button", { name: "Save file" }));
      expect(desktop.saveBlobWithDialog).not.toHaveBeenCalled();
      await user.click(
        screen.getByRole("button", { name: "Retry generation" }),
      );
      await screen.findByRole("img", {
        name: "Seating chart in the generated file",
      });
      expect(screen.getByRole("button", { name: "Save file" })).toBeEnabled();
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      expect(api.exportDraft).toHaveBeenCalledTimes(2);
    },
  );

  it("ignores an old image error after a new revision has a prepared preview", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ExportWorkspace {...props} />);
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    const oldImage = await screen.findByRole("img", {
      name: "Seating chart in the generated file",
    });
    rerender(<ExportWorkspace {...props} revision={4} />);
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    const newImage = await screen.findByRole("img", {
      name: "Seating chart in the generated file",
    });
    expect(newImage).not.toBe(oldImage);
    fireEvent.error(oldImage);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save file" })).toBeEnabled();
    expect(newImage).toBeInTheDocument();
  });

  it("reports malformed HTML previews instead of showing a blank embedded document", async () => {
    vi.mocked(api.exportDraft).mockResolvedValueOnce({
      ...file(),
      blob: new Blob(["<html><body>No diagram</body></html>"], {
        type: "text/html",
      }),
    });
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "cannot be displayed safely",
    );
    expect(screen.getByRole("button", { name: "Save file" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Retry generation" }),
    ).toBeEnabled();
  });

  it("keeps preview-only font warnings visible and deduplicates shared file warnings", async () => {
    vi.mocked(api.exportDraft).mockResolvedValueOnce({
      ...file(),
      warnings: ["Shared issue", "File issue", "File issue"],
    });
    vi.mocked(api.previewExportDraft).mockResolvedValueOnce({
      blob: new Blob(["<svg />"], { type: "image/svg+xml" }),
      warnings: [
        "Shared issue",
        "Preview missing glyph",
        "Preview missing glyph",
      ],
    });
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "File format" }),
      "docx",
    );
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await screen.findByRole("img", {
      name: "Seating chart in the generated file",
    });
    expect(screen.getByText("File quality notes")).toBeInTheDocument();
    expect(screen.getByText("Preview rendering notes")).toBeInTheDocument();
    for (const warning of [
      "Shared issue",
      "File issue",
      "Preview missing glyph",
    ])
      expect(screen.getAllByText(warning)).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Save file" })).toBeEnabled();
  });

  it("discards the earlier file when regeneration discovers a stale draft revision", async () => {
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await screen.findByTitle("Seating chart in the generated file");
    vi.mocked(api.exportDraft).mockRejectedValueOnce(
      new api.RosterApiError(409, "editor_revision_conflict", "stale"),
    );
    await user.click(
      screen.getByRole("button", { name: "Regenerate preview" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "seating plan has changed",
    );
    expect(screen.getByRole("button", { name: "Save file" })).toBeDisabled();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:prepared-file");
  });

  it("previews PDF through a stable page image but saves the prepared PDF bytes", async () => {
    const pdf = new Blob(["%PDF-prepared"], { type: "application/pdf" });
    vi.mocked(api.exportDraft).mockResolvedValueOnce({
      blob: pdf,
      filename: "seat-plan.pdf",
      warnings: [],
    });
    const user = userEvent.setup();
    const { container } = render(<ExportWorkspace {...props} />);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "File format" }),
      "pdf",
    );
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await screen.findByRole("img", {
      name: "Seating chart in the generated file",
    });
    expect(container.querySelector("object")).toBeNull();
    expect(
      screen.getByText(/without requiring a browser PDF plugin/),
    ).toBeInTheDocument();
    expect(api.previewExportDraft).toHaveBeenCalledWith(
      expect.objectContaining({ format: "pdf", expected_revision: 3 }),
      expect.any(AbortSignal),
    );
    await user.click(screen.getByRole("button", { name: "Save file" }));
    expect(desktop.saveBlobWithDialog).toHaveBeenCalledWith(
      "seat-plan.pdf",
      pdf,
    );
    expect(api.exportDraft).toHaveBeenCalledOnce();
  });

  it("shows a recoverable error when the PDF page image fails instead of a blank viewer", async () => {
    vi.mocked(api.previewExportDraft).mockRejectedValueOnce(
      new Error("preview unavailable"),
    );
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "File format" }),
      "pdf",
    );
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "could not be generated",
    );
    expect(screen.getByRole("button", { name: "Save file" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Retry generation" }),
    ).toBeEnabled();
    expect(vi.mocked(api.exportDraft).mock.calls[0][1]?.aborted).toBe(true);
    expect(URL.createObjectURL).not.toHaveBeenCalled();
  });

  it("shows coherent safe defaults and saves the exact prepared file without rendering twice", async () => {
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    expect(screen.getByRole("button", { name: "Save file" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: /Show names/ })).toBeChecked();
    expect(screen.queryByText("Plan report")).not.toBeInTheDocument();
    expect(screen.queryByText("Hide scores")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    const frame = await screen.findByTitle(
      "Seating chart in the generated file",
    );
    expect(frame).toHaveAttribute("src", "blob:prepared-file");
    expect(frame.tagName).toBe("IMG");
    expect(document.querySelector("iframe, object")).toBeNull();
    expect(api.exportDraft).toHaveBeenCalledWith(
      expect.objectContaining({
        draft_id: "draft-a",
        expected_revision: 3,
        title: "Class A",
        page_scale: 1,
        privacy: expect.objectContaining({
          show_height: false,
          hide_notes: true,
        }),
      }),
      expect.any(AbortSignal),
    );
    await user.click(screen.getByRole("button", { name: "Save file" }));
    expect(desktop.saveBlobWithDialog).toHaveBeenCalledWith(
      "seat-plan.print.html",
      (await vi.mocked(api.exportDraft).mock.results[0].value).blob,
    );
    expect(api.exportDraft).toHaveBeenCalledOnce();
    expect(props.onExported).toHaveBeenCalledOnce();
  });

  it("revokes the old preview on settings changes and strips IDs when anonymizing", async () => {
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.click(
      screen.getByRole("checkbox", { name: "Also show student IDs" }),
    );
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await screen.findByTitle("Seating chart in the generated file");
    await user.click(screen.getByRole("radio", { name: /Anonymous copy/ }));
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:prepared-file");
    expect(
      screen.queryByTitle("Seating chart in the generated file"),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save file" })).toBeDisabled();
    expect(
      screen.queryByRole("checkbox", { name: "Also show student IDs" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    expect(api.exportDraft).toHaveBeenLastCalledWith(
      expect.objectContaining({ template: "public", show_student_ids: false }),
      expect.any(AbortSignal),
    );
  });

  it("cancels preparation immediately and ignores a late result", async () => {
    const pending = deferred<ReturnType<typeof file>>();
    vi.mocked(api.exportDraft).mockReturnValueOnce(pending.promise);
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.dblClick(
      screen.getByRole("button", { name: "Generate preview" }),
    );
    expect(api.exportDraft).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "Cancel generation" }));
    expect(vi.mocked(api.exportDraft).mock.calls[0][1]?.aborted).toBe(true);
    await act(async () => pending.resolve(file()));
    expect(URL.createObjectURL).not.toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: "Generate preview" }),
    ).toBeEnabled();
    expect(
      screen.getByText("Cancelled. No file was saved."),
    ).toBeInTheDocument();
  });

  it("invalidates asynchronous results after a revision change and on unmount", async () => {
    const pending = deferred<ReturnType<typeof file>>();
    vi.mocked(api.exportDraft).mockReturnValue(pending.promise);
    const user = userEvent.setup();
    const { rerender, unmount } = render(<ExportWorkspace {...props} />);
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    rerender(<ExportWorkspace {...props} revision={4} />);
    expect(vi.mocked(api.exportDraft).mock.calls[0][1]?.aborted).toBe(true);
    await act(async () => pending.resolve(file()));
    expect(screen.getByRole("button", { name: "Save file" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await screen.findByTitle("Seating chart in the generated file");
    unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:prepared-file");
  });

  it("offers retry after timeout without keeping a permanently busy button", async () => {
    vi.mocked(api.exportDraft).mockRejectedValueOnce(
      new DOMException("timed out", "TimeoutError"),
    );
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "exceeded 60 seconds",
    );
    await user.click(screen.getByRole("button", { name: "Retry generation" }));
    expect(
      await screen.findByTitle("Seating chart in the generated file"),
    ).toBeInTheDocument();
    expect(api.exportDraft).toHaveBeenCalledTimes(2);
  });

  it("retains a prepared file after native save cancellation and prevents repeated saves", async () => {
    const pending = deferred<"cancelled">();
    vi.mocked(desktop.saveBlobWithDialog).mockReturnValueOnce(pending.promise);
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await screen.findByTitle("Seating chart in the generated file");
    await user.dblClick(screen.getByRole("button", { name: "Save file" }));
    expect(desktop.saveBlobWithDialog).toHaveBeenCalledOnce();
    expect(
      screen.getByRole("combobox", { name: "File format" }),
    ).toBeDisabled();
    await act(async () => pending.resolve("cancelled"));
    expect(screen.getByRole("button", { name: "Save file" })).toBeEnabled();
    expect(props.onExported).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Save file" }));
    expect(api.exportDraft).toHaveBeenCalledOnce();
    expect(props.onExported).toHaveBeenCalledOnce();
  });

  it("only shows meaningful page options and labels Office preview limitations", async () => {
    const user = userEvent.setup();
    render(<ExportWorkspace {...props} />);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "File format" }),
      "pptx",
    );
    expect(
      screen.queryByRole("combobox", { name: "Paper" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText(/pagination and fonts may vary/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    expect(
      await screen.findByRole("img", {
        name: "Seating chart in the generated file",
      }),
    ).toBeInTheDocument();
    expect(api.previewExportDraft).toHaveBeenCalledWith(
      expect.objectContaining({ format: "pptx", orientation: "landscape" }),
      expect.any(AbortSignal),
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "File format" }),
      "pdf",
    );
    expect(screen.getByRole("combobox", { name: "Paper" })).toBeInTheDocument();
  });

  it("shows quality warnings without blocking the file and never exports without a draft", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ExportWorkspace {...props} draftId={null} />);
    expect(
      screen.getByRole("button", { name: "Generate preview" }),
    ).toBeDisabled();
    rerender(<ExportWorkspace {...props} />);
    vi.mocked(api.exportDraft).mockResolvedValueOnce({
      ...file(),
      warnings: ["A missing glyph needs attention."],
    });
    await user.click(screen.getByRole("button", { name: "Generate preview" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Save file" })).toBeEnabled(),
    );
    expect(
      screen.getByText("A missing glyph needs attention."),
    ).toBeInTheDocument();
  });
});
