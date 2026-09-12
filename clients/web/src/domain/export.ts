import type { CatalogOption, ExportDraftRequest } from "../api/types";

export const DEFAULT_EXPORT_FORMAT = "print-html";
export const DEFAULT_EXPORT_TEMPLATE = "teacher";

export const SUPPORTED_EXPORT_FORMATS = [
  "print-html",
  "pdf",
  "png",
  "svg",
  "docx",
  "pptx",
  "xlsx",
] as const;

/** A single HTML entry; unknown server formats must not acquire fake controls. */
export function availableExportFormats(
  formats: CatalogOption[],
): CatalogOption[] {
  return SUPPORTED_EXPORT_FORMATS.flatMap((id) => {
    const option = formats.find((format) => format.id === id);
    return option ? [option] : [];
  });
}

export function hasPaperLayout(format: string): boolean {
  return format !== "pptx" && format !== "xlsx";
}

export function isOfficeFormat(format: string): boolean {
  return format === "docx" || format === "pptx" || format === "xlsx";
}

/** PDF plugins are absent from many WebViews and headless browsers. */
export function needsRenderedPreview(format: string): boolean {
  return format === "pdf" || isOfficeFormat(format);
}

export type ExportSettings = {
  format: string;
  anonymize: boolean;
  showStudentIds: boolean;
  orientation: "portrait" | "landscape";
  paper: "a4" | "a3" | "letter";
  margin: number;
};

export function buildExportRequest(
  settings: ExportSettings,
  draftId: string,
  revision: number,
  title: string,
  locale: "zh-CN" | "en",
): ExportDraftRequest {
  const paperLayout = hasPaperLayout(settings.format);
  return {
    draft_id: draftId,
    expected_revision: revision,
    title,
    format: settings.format,
    template: settings.anonymize ? "public" : "teacher",
    privacy: {
      hide_scores: true,
      hide_notes: true,
      hide_special_needs: true,
      anonymize: settings.anonymize,
      show_height: false,
      show_vision: false,
    },
    show_student_ids: !settings.anonymize && settings.showStudentIds,
    orientation: paperLayout ? settings.orientation : "landscape",
    paper_size: paperLayout ? settings.paper : "a4",
    margin_mm: paperLayout ? settings.margin : 12,
    page_scale: 1,
    locale: locale === "zh-CN" ? "zh" : "en",
  };
}
