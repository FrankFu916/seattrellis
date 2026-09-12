# Font rendering and typography

SeatTrellis uses installed fonts without downloading or bundling font files. The important distinction is between a chart whose appearance is fixed during export and a document whose text remains editable in another application.

## How each format uses fonts

| Output | Font behavior |
| --- | --- |
| PNG / PDF | The selected local font is rasterized into the shared page scene at 216 dpi. Readers do not substitute that image's font; PDF text is not selectable. |
| SVG / HTML / print-ready HTML | When a supported local font is available, displayed text is converted to glyph paths from the same face used by PNG/PDF. The exported graphics do not require that font on the viewing computer. Without a usable local font, browser-font fallback can change appearance. |
| XLSX / DOCX / PPTX | Text remains editable. The file declares fonts, but the reader may substitute available fonts and change line wrapping, alignment, or pagination. The workbench preview represents the chart, not the final Office reader's rendering. |

SVG text outlines and PNG/PDF glyphs share the selected font face and advance widths. Page coordinates and font sizes are measured in points. Keeping those measurements together avoids the previous mismatch between independently laid-out previews and downloads. It does not remove printer margins, viewer zoom, or application-specific print behavior.

The workbench displays a PDF's page scene as an SVG preview rather than relying on a browser PDF plugin. Saving downloads the prepared PDF, not that SVG preview.

## Discovery and face selection

Discovery tries a fixed set of known files and exact PostScript face names, in this order:

1. PingFang SC Regular (`PingFangSC-Regular`).
2. Noto Sans CJK SC Regular (`NotoSansCJKsc-Regular`).
3. Microsoft YaHei (`MicrosoftYaHei`).
4. Heiti SC Medium (`STHeitiSC-Medium`); the supported macOS face has normal weight 400 despite its name.
5. WenQuanYi Zen Hei (`WenQuanYiZenHei`).
6. Arial Unicode MS (`ArialUnicodeMS`).
7. SimSun (`SimSun`).

Which candidate is present depends on the operating system and installed fonts. Supported user-installed candidates are considered before moving to lower-priority families. A preferred user installation can therefore win over a legacy system fallback.

A `.ttc` file contains several faces. SeatTrellis reads their names, selects the requested normal-weight Simplified Chinese face, and passes its actual collection index to the rasterizer. It does not assume that index zero is the correct face, and it does not label a Heiti Light file as PingFang.

Unreadable, malformed, wrong-face, or unsuitable candidates are skipped. A small common Chinese/Latin glyph check rejects unsuitable fonts; it is not a guarantee that every rare name character exists. A successful selection is cached for the process. Failed discovery has a one-second retry cooldown to avoid repeated filesystem scans while measuring text. It is not permanently cached, so installing a supported font can recover later exports. After changing an already selected font, restart SeatTrellis to discard the successful cache.

## Installation and custom-font limits

For a headless Linux installation, ensure that a supported Noto Sans CJK SC Regular font is available to the process that runs SeatTrellis. Installing it on the browser's computer does not supply it to a separate server or container.

Discovery is not a general operating-system font picker. Installing an arbitrary institutional font, KaiTi variant, or custom `.ttf`/`.otf` does **not** automatically make SeatTrellis use it. The export workspace currently has no custom-font selector. To apply a different font to editable output, open an Office export and choose the font in the document application, then check the layout again.

The implementation's known paths and face names are listed in `crates/seattrellis-export/src/fonts.rs`. No font file is included in the SVG/HTML output: it contains only the drawing paths for the text used in that chart. Font licensing remains the responsibility of whoever installs and uses a font.

## Diagnosing text problems

- **Text missing from PNG/PDF:** inspect the no-usable-font warning and install a supported font. A file that was successfully written can still lack text.
- **Replacement boxes or rare characters missing:** inspect the missing-glyph warning. The warning counts unsupported characters without reproducing student names. A successful font load does not prove complete language coverage.
- **Names too small or shortened:** inspect the small-text or shortening warning, change paper size/orientation, or use the editable assignment worksheet for complete values.
- **Office differs from its chart preview:** check the reader's installed fonts, zoom, and print settings. Local export-font warnings describe rendering performed by SeatTrellis; an Office reader chooses its own fonts.
- **Different SVG/HTML appearance on another device:** check whether export occurred without a usable local font and therefore used fallback text instead of fixed glyph outlines.

Always inspect the generated preview and, for editable formats, the file in its intended reader. Neither local rasterization nor font fallback promises correct glyphs for every script or identical output from every printer.

## Related guides

- [Export and printing](export.md)
- [Quick start](quickstart.md)
