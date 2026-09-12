import { describe, expect, it } from "vitest";
import { createTranslator } from "../i18n/messages";
import { formatExportWarning } from "./exportWarnings";

const zh = createTranslator("zh-CN");
const en = createTranslator("en");

describe("export warning localization", () => {
  it.each([
    [
      "no usable system font found; PNG/PDF text was omitted",
      "未找到可用的系统字体",
    ],
    [
      "Some names are smaller than 8 pt. Use a larger paper size or landscape orientation for a more readable chart.",
      "部分姓名小于 8 磅",
    ],
    [
      "Some text was shortened to fit. Use a larger paper size or the editable spreadsheet to see complete values.",
      "部分文字因空间限制已缩略",
    ],
    [
      "Some spreadsheet cell values were shortened to Excel's 32767-character limit. Check the source roster for complete values.",
      "32767 字符上限",
    ],
    [
      "Some text in the Word document was shortened to fit. Use a larger paper size or the editable spreadsheet to see complete values.",
      "Word 文档中部分文字",
    ],
    [
      "Some student names in the Word document are smaller than 8 pt. Use a larger paper size or landscape orientation for a more readable chart.",
      "Word 文档中部分学生姓名",
    ],
  ])("localizes the known renderer warning: %s", (warning, chinese) => {
    expect(formatExportWarning(warning, zh)).toContain(chinese);
    expect(formatExportWarning(warning, en)).not.toMatch(/[\u4e00-\u9fff]/);
  });

  it("preserves the missing-glyph count without copying roster content", () => {
    const warning =
      "selected system font does not support 12 distinct characters; rendered text may show replacement glyphs";
    expect(formatExportWarning(warning, zh)).toContain("不支持 12 种字符");
    expect(formatExportWarning(warning, en)).toContain(
      "12 distinct characters",
    );
  });

  it("keeps unknown warnings verbatim instead of silently dropping information", () => {
    for (const warning of [
      "Future renderer warning",
      "Some text was shortened to fit. New context matters.",
      "constructor",
      "__proto__",
    ]) {
      expect(formatExportWarning(warning, zh)).toBe(warning);
      expect(formatExportWarning(warning, en)).toBe(warning);
    }
  });
});
