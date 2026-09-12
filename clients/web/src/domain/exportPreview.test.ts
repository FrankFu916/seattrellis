import { describe, expect, it } from "vitest";
import {
  extractHtmlChartPreview,
  InvalidExportPreviewError,
} from "./exportPreview";

const svg = (contents: string) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 200">${contents}</svg>`;
const html = (contents: string) =>
  new Blob([`<!doctype html><html><body>${contents}</body></html>`], {
    type: "text/html",
  });

describe("HTML export chart extraction", () => {
  it("extracts actual geometry and accessible names without a browsing context", async () => {
    const output = await extractHtmlChartPreview(
      html(
        `<main>${svg('<title>班级</title><rect width="30" height="20"/><text>林晓雨</text>')}</main><script>throw new Error('must stay inert')</script>`,
      ),
    );
    expect(output.type).toBe("image/svg+xml");
    const source = await output.text();
    expect(source).toContain("林晓雨");
    expect(source).toContain('viewBox="0 0 300 200"');
    expect(source).toContain('<rect width="30" height="20"');
    expect(source).not.toContain("<script");
    expect(source).not.toContain("<main");
  });

  it("allows renderer-owned local clipping paths", async () => {
    const source = svg(
      '<defs><clipPath id="text-1"><rect width="10" height="10"/></clipPath></defs><path clip-path="url(#text-1)" d="M0 0L5 5"/>',
    );
    expect(
      await (await extractHtmlChartPreview(html(source))).text(),
    ).toContain('clip-path="url(#text-1)"');
  });

  it("does not confuse literal accessible labels with resource URLs", async () => {
    const source = svg(
      '<g aria-label="URL(student)"><text>URL(student)</text></g>',
    );
    expect(
      await (await extractHtmlChartPreview(html(source))).text(),
    ).toContain('aria-label="URL(student)"');
  });

  it.each([
    "<p>No diagram</p>",
    svg("") + svg(""),
    '<svg xmlns="https://untrusted.example" viewBox="0 0 30 20"/>',
    '<svg xmlns="http://www.w3.org/2000/svg"/>',
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 -1 20"/>',
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 Infinity 20"/>',
  ])(
    "rejects missing, ambiguous or invalid chart geometry: %s",
    async (source) => {
      await expect(
        extractHtmlChartPreview(html(source)),
      ).rejects.toBeInstanceOf(InvalidExportPreviewError);
    },
  );

  it.each([
    "<script>alert(1)</script>",
    "<foreignObject><div>HTML</div></foreignObject>",
    '<use href="https://untrusted.example/remote.svg"/>',
    '<text onclick="alert(1)">Student</text>',
    '<path style="fill: red" d="M0 0L5 5"/>',
    '<path clip-path="url(https://untrusted.example/clip)" d="M0 0L5 5"/>',
    '<g xml:base="https://untrusted.example"><path clip-path="url(#local)" d="M0 0L5 5"/></g>',
  ])(
    "rejects active or externally referenced SVG markup: %s",
    async (contents) => {
      await expect(
        extractHtmlChartPreview(html(svg(contents))),
      ).rejects.toBeInstanceOf(InvalidExportPreviewError);
    },
  );
});
