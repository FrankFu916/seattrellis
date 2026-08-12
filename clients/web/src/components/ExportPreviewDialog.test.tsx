import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ExportPrivacyOptions, ExportTemplate } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { ExportPreviewDialog } from "./ExportPreviewDialog";

const privacy: ExportPrivacyOptions = {
  hide_scores: true,
  hide_notes: true,
  hide_special_needs: true,
  anonymize: false,
  show_height: false,
  show_vision: false,
};

function renderPreview(template: ExportTemplate) {
  render(
    <ExportPreviewDialog
      assignments={[
        {
          seatId: "R1C1",
          row: 0,
          column: 0,
          student: { id: "S1", name: "林晓雨" },
          locked: false,
        },
      ]}
      orientation="landscape"
      format="print-html"
      template={template}
      privacy={privacy}
      open
      isSaving={false}
      error={null}
      t={createTranslator("zh-CN")}
      onClose={vi.fn()}
      onSave={vi.fn()}
    />,
  );
}

describe("ExportPreviewDialog privacy preview", () => {
  it("shows real names for the default teacher template", () => {
    renderPreview("teacher");
    expect(screen.getByText("林晓雨")).toBeInTheDocument();
  });

  it("matches the Rust public-template anonymization before saving", () => {
    renderPreview("public");
    expect(screen.queryByText("林晓雨")).not.toBeInTheDocument();
    expect(screen.getByText(/学生/)).toBeInTheDocument();
  });
});
