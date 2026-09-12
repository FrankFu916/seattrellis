# Export and printing guide

SeatTrellis renders saved seating plans locally with its Rust export engine. Creating an export does not require Microsoft Office, LibreOffice, Python, or a headless browser. Opening or printing a file still requires an application that supports its format. Generation time depends on the chart, format, and local fonts; the first font load can take longer.

## Prepare, inspect, then save

The workbench has one export workspace:

1. Choose a format, name policy, and any supported page settings.
2. Generate the preview. Check the names, seat identifiers, aisle gaps, orientation, and warnings.
3. Save the prepared file. Saving uses the same generated file held in memory; it does not silently render a different copy.

SVG and PNG previews display the generated image. The HTML preview extracts its embedded SVG chart and displays it as an inert image; the complete original HTML file is saved unchanged. This avoids executable frames and WebView sandbox incompatibilities. PDF uses an SVG page preview from the same scene, so it does not depend on a browser PDF plugin; saving still writes the actual prepared PDF file. Office formats show a seating-chart preview derived from the same draft and settings, alongside the prepared editable document. This is **not a pixel-for-pixel preview of Word, Excel, or PowerPoint**.

Changing the settings, draft, or draft revision invalidates the prepared file and requires another preview. Preparation can be cancelled; a timeout or stale-draft error requires retrying with the current plan. Browser downloads and desktop save dialogs differ, and the preview is not a promise that a file has already been written to disk.

## Formats and page controls

The workbench shows seven choices. Legacy `html` remains supported by the CLI and API but is not a second, duplicate HTML choice in the interface.

| Format | Intended use | Page controls in the workbench |
| --- | --- | --- |
| Print-ready HTML (`print-html`) | Standalone, script-free page for viewing and browser printing | A4, A3, or Letter; orientation; margins |
| PDF (`pdf`) | A fixed, single-page image-based document for sharing or printing | A4, A3, or Letter; orientation; margins |
| PNG (`png`) | A page image for messaging or presentation software | A4, A3, or Letter; orientation; margins |
| SVG (`svg`) | Scalable seating graphics; outlined text preserves appearance when a supported local font is available | A4, A3, or Letter; orientation; margins |
| Word (`docx`) | Editable seating-table document | A4, A3, or Letter; orientation; margins |
| PowerPoint (`pptx`) | Editable seat shapes and text on a 16:9 slide | Fixed slide geometry; no paper controls |
| Excel (`xlsx`) | Editable seating grid plus an assignment worksheet | Worksheet layout; no paper controls |

The workbench defaults to print-ready HTML, A4 landscape, and 12 mm margins. Narrow and wide margins are 6 mm and 20 mm. Page controls are hidden for formats that do not use them.

HTML, SVG, PNG, and PDF share a point-based scene: seat proportions, identifiers, gaps, and text positions come from the same layout. The chart fits within the selected page rather than stretching each seat independently. PNG and PDF rasterize this scene at 216 dpi; PDF text is part of the page image, not selectable text. SVG/HTML use local-font glyph outlines when available, so another computer does not need the same font to display those outlines. Use Office formats when names need to remain editable.

Office documents use native tables, cells, or shapes rather than flattening everything into an image. Their reader may substitute fonts or lay out editable text differently. Check the file in the intended application before distributing or printing it.

## Names and privacy

The workbench exports real names by default, with student IDs **off unless explicitly enabled**. Choose anonymous labels to replace names with numbered placeholders; this also suppresses student IDs and personal detail lines.

The workspace does not expose score, note, special-needs, height, or vision toggles. Its requests leave those personal details out. It does not offer a separate report template with additional analysis.

Anonymization applies to the student fields, not arbitrary text that you put in a class title or seat identifier. Check those fields yourself before sharing. Saving a file creates a copy under your control: changing settings, cancelling preparation, or leaving the page does not securely erase saved files, downloads, or operating-system copies. See the [privacy guide](privacy.md) for broader data-handling boundaries.

## Warnings and printing checks

- **No usable local font:** PNG/PDF can be generated without text and report a warning. Do not distribute such a file as a complete chart. SVG/HTML may fall back to reader fonts, so appearance is no longer fixed by outlines.
- **Unsupported characters:** the selected font may lack rare name characters. The warning reports the number of distinct unsupported characters without including the names. Check the preview; an editable Office document may display those characters with a different installed font.
- **Small or shortened text:** a dense chart or a long value can require smaller text or an ellipsis. Try a larger sheet or a better-fitting orientation. Use the Excel assignment worksheet when complete values matter more than a one-page chart.

For browser printing, inspect the browser's print preview, use the intended paper size, and disable extra headers and footers. The HTML already includes page margins; avoid adding a second set of browser margins. Printer hardware, scaling settings, and reader behavior can still affect the result—there is no guarantee of identical output on every printer.

## CLI compatibility

Existing `export` and `project-export` commands remain available and validate the solved assignment before rendering. `project-export` reads a saved snapshot rather than solving again.

```bash
seattrellis export \
  --problem problem.json \
  --solution plan.json \
  --format png \
  --output outputs/plan.png

seattrellis project-export \
  --project my-class/seattrellis.project.json \
  --snapshot outputs/candidates.json \
  --candidate candidate_02 \
  --format print-html \
  --template public \
  --orientation landscape \
  --output outputs/class_wall.html
```

`--template teacher` keeps real names; `--template public` anonymizes student fields. Legacy `project-export` teacher exports also include IDs and available height/vision details, unlike the workbench defaults. Its `--orientation auto` leaves the renderer default in effect: landscape for `print-html`, portrait for other paper formats; PPTX remains 16:9. Use `--help` for the flags supported by each command rather than assuming every workbench setting has a CLI flag.

The compatibility API still accepts older export fields, including `html` and `page_scale`. Enlarging a visual chart beyond its fitted page is not a supported cropping feature: values above the fit scale do not deliberately push seats outside the page. New workbench requests use automatic fit without a separate scale slider.

## Related guides

- [Font strategy](font-strategy.md)
- [Web workbench](web.md)
- [Class projects](project.md)
- [Quick start](quickstart.md)
