const SVG_NAMESPACE = "http://www.w3.org/2000/svg";
const PASSIVE_SVG_ELEMENTS = new Set([
  "svg",
  "g",
  "path",
  "rect",
  "circle",
  "ellipse",
  "line",
  "polyline",
  "polygon",
  "text",
  "tspan",
  "title",
  "desc",
  "defs",
  "clipPath",
]);
const URL_PRESENTATION_ATTRIBUTES = new Set([
  "fill",
  "stroke",
  "filter",
  "clip-path",
  "mask",
  "cursor",
  "marker-start",
  "marker-mid",
  "marker-end",
]);

export class InvalidExportPreviewError extends Error {
  constructor() {
    super("The HTML export did not contain a single passive SVG chart.");
    this.name = "InvalidExportPreviewError";
  }
}

/**
 * Display the actual chart in an HTML artifact without creating a document
 * browsing context. Detached parsing never inserts the HTML into the app;
 * the extracted chart is additionally limited to our renderer's passive SVG
 * vocabulary before it is loaded as an image (where scripts cannot execute).
 */
export async function extractHtmlChartPreview(file: Blob): Promise<Blob> {
  const html = await file.text();
  const document = new DOMParser().parseFromString(html, "text/html");
  const charts = document.body.querySelectorAll("svg");
  if (charts.length !== 1) throw new InvalidExportPreviewError();
  const chart = charts[0];
  if (
    chart.namespaceURI !== SVG_NAMESPACE ||
    chart.getAttribute("xmlns") !== SVG_NAMESPACE
  ) {
    throw new InvalidExportPreviewError();
  }
  const viewBox = chart
    .getAttribute("viewBox")
    ?.trim()
    .split(/[\s,]+/)
    .map(Number);
  if (
    !viewBox ||
    viewBox.length !== 4 ||
    viewBox.some((value) => !Number.isFinite(value)) ||
    viewBox[2] <= 0 ||
    viewBox[3] <= 0
  ) {
    throw new InvalidExportPreviewError();
  }
  for (const element of [chart, ...chart.querySelectorAll("*")]) {
    if (
      element.namespaceURI !== SVG_NAMESPACE ||
      !PASSIVE_SVG_ELEMENTS.has(element.localName)
    )
      throw new InvalidExportPreviewError();
    for (const attribute of element.attributes) {
      const name = attribute.name.toLowerCase();
      const safeClip =
        name === "clip-path" && /^url\(#[\w.-]+\)$/.test(attribute.value);
      if (
        name.startsWith("on") ||
        name === "href" ||
        name.endsWith(":href") ||
        name === "src" ||
        name === "style" ||
        name === "xml:base" ||
        (URL_PRESENTATION_ATTRIBUTES.has(name) &&
          /url\s*\(/i.test(attribute.value) &&
          !safeClip)
      ) {
        throw new InvalidExportPreviewError();
      }
    }
  }
  return new Blob([new XMLSerializer().serializeToString(chart)], {
    type: "image/svg+xml",
  });
}
