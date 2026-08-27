# Export Formats

[English](export.md) / [简体中文](export.zh.md)

SeatTrellis v2.0.0 renders exports locally through the Rust export layer. No
optional Python, browser, or office installation is required to generate the
files.

## The eight formats

| Format | Description |
| --- | --- |
| HTML | Self-contained browser-viewable seating page |
| `print-html` | Dedicated print sheet; A4 landscape by default |
| SVG | Self-contained vector seating map |
| PNG | Raster seating image |
| PDF | Single-page rasterized document |
| XLSX | Excel workbook with seating and assignment sheets |
| DOCX | Editable Word document with a seating table |
| PPTX | One editable 16:9 PowerPoint slide |

The standalone `export` command accepts seven formats and intentionally does not
accept `print-html`. The `project-export` command and the web/API export flow
accept all eight:

```text
svg | html | print-html | png | pdf | xlsx | docx | pptx
```

## Standalone CLI export

`--solution` must be the JSON response written by `solve --output`. SeatTrellis
revalidates the complete assignment, including hard constraints, before
rendering; an invalid or non-solved response is refused.

```bash
seattrellis_cli solve \
  --problem problem.json \
  --output outputs/plan.json

seattrellis_cli export \
  --problem problem.json \
  --solution outputs/plan.json \
  --format png \
  --template teacher \
  --output outputs/plan.png
```

The standalone CLI supports `teacher` and `public` templates. `teacher` is the
default. `public` anonymizes student labels and hides student IDs and sensitive
details. The privacy layer is fail-closed; a caller cannot use export options
to loosen public safety defaults.

## Project export

`project-export` renders a plan already saved by `project-solve`; it **never
re-solves**. For a candidate set, it selects the project's recommended
candidate unless `--candidate <id>` is supplied.

```bash
seattrellis_cli project-solve \
  --project my-class/seattrellis.project.json \
  --candidates 3 \
  --output outputs/candidates.json

seattrellis_cli project-export \
  --project my-class/seattrellis.project.json \
  --snapshot outputs/candidates.json \
  --candidate candidate_02 \
  --format print-html \
  --template public \
  --orientation landscape \
  --output outputs/wall-copy.html
```

`--template teacher` retains real names, IDs, and any enabled detail fields.
`--template public` forces anonymization and suppresses names, IDs, height, and
vision details. `--orientation` accepts `portrait`, `landscape`, or `auto`.
With `auto` (the default), `print-html` uses A4 landscape and other document
formats use portrait. An explicit orientation overrides that default.

## Print HTML and page layout

`print-html` is a dedicated, standalone HTML document intended for a wall or
classroom handout. Its default page is A4 landscape, with a front-of-room marker,
aisle structure, a deterministic fit-to-page layout, and a consistent font size
based on the longest visible name. It can be opened in a browser or printed to a
PDF by the user.

The HTTP export request additionally supports `paper_size` (`a4`, `a3`, or
`letter`), `margin_mm`, `page_scale`, `orientation`, `locale`, and
`show_student_ids`. Public templates still enforce their privacy boundary.

## Templates and privacy

The API supports three templates:

| Template | Use |
| --- | --- |
| `public` | Anonymized class handout with no student IDs or sensitive detail |
| `teacher` | Internal copy with real labels and permitted detail fields |
| `report` | Candidate explanation with scores and hard-constraint summary |

The CLI accepts `public` and `teacher`; the `report` template is an API/workbench
template. Exporters do not render scores, notes, or special needs in the basic
seating map, and the public path additionally suppresses identifying labels and
detail fields.

## Font behavior

HTML, print HTML, SVG, and Office documents use their format's normal font
fallback behavior. **PNG and PDF are different:** both are rasterized locally
with a discovered system font at export time. They do not embed a font, write
font metadata, or rely on a viewer's font selection. See [Font strategy](font-strategy.md).

If no usable system font can be loaded, PNG/PDF files are still produced but
text is omitted and the exporter emits a warning. Install a suitable CJK font
before exporting non-ASCII names.

## Release integrity

v2.0.0 desktop bundles are unsigned by the owner's release policy. Verify
downloaded installers against `SHA256SUMS` and desktop bundles against
`DESKTOP-SHA256SUMS`. On macOS, Gatekeeper may require **Open** from the
context menu; Windows may display a SmartScreen warning. Unsigned release status
does not change the local-only behavior of generated exports.

## Known limitation

RTL text such as Arabic or Hebrew is drawn in logical order in PNG/PDF and does
not yet receive bidirectional layout.

## Related documents

- [Quick start](quickstart.md)
- [Web workbench](web.md)
- [Project workflow](project.md)
- [Font strategy](font-strategy.md)
