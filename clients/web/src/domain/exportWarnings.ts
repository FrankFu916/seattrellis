import type { MessageKey, Translate } from "../i18n/messages";

// Keep the wire messages intact in artifacts/logs. Known renderer diagnostics
// get localized display text; an unfamiliar message must remain visible.
const KNOWN_WARNINGS: Record<string, MessageKey> = {
  "no usable system font found; PNG/PDF text was omitted":
    "export.warningFontUnavailable",
  "Some names are smaller than 8 pt. Use a larger paper size or landscape orientation for a more readable chart.":
    "export.warningSmallNames",
  "Some text was shortened to fit. Use a larger paper size or the editable spreadsheet to see complete values.":
    "export.warningShortened",
  "Some spreadsheet cell values were shortened to Excel's 32767-character limit. Check the source roster for complete values.":
    "export.warningSpreadsheetLimit",
  "Some text in the Word document was shortened to fit. Use a larger paper size or the editable spreadsheet to see complete values.":
    "export.warningWordShortened",
  "Some student names in the Word document are smaller than 8 pt. Use a larger paper size or landscape orientation for a more readable chart.":
    "export.warningWordSmallNames",
};

export function formatExportWarning(warning: string, t: Translate): string {
  const key = Object.hasOwn(KNOWN_WARNINGS, warning)
    ? KNOWN_WARNINGS[warning]
    : undefined;
  if (key) return t(key);
  const missingGlyphs =
    /^selected system font does not support (\d+) distinct characters; rendered text may show replacement glyphs$/.exec(
      warning,
    );
  if (missingGlyphs)
    return t("export.warningMissingGlyphs", { count: missingGlyphs[1] });
  return warning;
}
