import { describe, expect, it } from "vitest";
import { demoCatalogs } from "../api/demo";
import {
  availableExportFormats,
  buildExportRequest,
  hasPaperLayout,
  needsRenderedPreview,
  type ExportSettings,
} from "./export";

const settings: ExportSettings = {
  format: "pdf",
  anonymize: false,
  showStudentIds: false,
  orientation: "portrait",
  paper: "a3",
  margin: 6,
};

describe("export request normalization", () => {
  it("uses rendered page previews for PDF and Office without relying on plugins", () => {
    for (const format of ["pdf", "docx", "pptx", "xlsx"])
      expect(needsRenderedPreview(format)).toBe(true);
    for (const format of ["print-html", "svg", "png"])
      expect(needsRenderedPreview(format)).toBe(false);
  });

  it("uses explicit privacy-safe defaults and binds the current draft revision", () => {
    expect(buildExportRequest(settings, "draft-1", 7, "Class A", "en")).toEqual(
      {
        draft_id: "draft-1",
        expected_revision: 7,
        title: "Class A",
        format: "pdf",
        template: "teacher",
        privacy: {
          hide_scores: true,
          hide_notes: true,
          hide_special_needs: true,
          show_height: false,
          show_vision: false,
          anonymize: false,
        },
        show_student_ids: false,
        orientation: "portrait",
        paper_size: "a3",
        margin_mm: 6,
        page_scale: 1,
        locale: "en",
      },
    );
  });
  it("cannot leak IDs through a previously selected checkbox in anonymous mode", () => {
    expect(
      buildExportRequest(
        { ...settings, showStudentIds: true, anonymize: true },
        "a",
        0,
        "班级",
        "zh-CN",
      ),
    ).toMatchObject({
      template: "public",
      show_student_ids: false,
      locale: "zh",
      privacy: { anonymize: true },
    });
  });
  it("normalizes irrelevant page options for slides and spreadsheets", () => {
    for (const format of ["pptx", "xlsx"]) {
      expect(hasPaperLayout(format)).toBe(false);
      expect(
        buildExportRequest({ ...settings, format }, "a", 0, "Class", "en"),
      ).toMatchObject({
        orientation: "landscape",
        paper_size: "a4",
        margin_mm: 12,
      });
    }
    for (const format of ["print-html", "pdf", "png", "svg", "docx"])
      expect(hasPaperLayout(format)).toBe(true);
  });
  it("only offers supported formats with one HTML entry", () => {
    const format = demoCatalogs.exportFormats[0];
    expect(
      availableExportFormats([
        ...demoCatalogs.exportFormats,
        { ...format, id: "html" },
        { ...format, id: "unknown" },
      ]).map((entry) => entry.id),
    ).toEqual(["print-html", "svg", "pptx"]);
  });
});
