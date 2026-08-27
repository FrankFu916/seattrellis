# Font Strategy

[English](font-strategy.md) / [简体中文](font-strategy.zh.md)

This document describes how SeatTrellis v2.0.0 handles fonts in HTML, print
HTML, SVG, PDF, and PNG exports.

## Principles

- Font files are not committed to the repository. CJK fonts are large and often
  have licensing restrictions.
- HTML, print HTML, SVG, and Office files do not package a SeatTrellis font;
  they use their format's normal consumer-side font handling.
- PNG and PDF load a system font file locally and rasterize glyphs at export
  time. They are viewer-independent images, not documents containing font
  references.

## System font discovery

The raster exporters use a fixed quality-priority chain:

1. `PingFangSC-Regular` from the standard macOS font locations;
2. `NotoSansCJKsc-Regular` from common Linux or Windows locations;
3. `MicrosoftYaHei` from Windows font locations;
4. `SimSun` from Windows font locations;
5. no usable font.

User-installed PingFang or Noto fonts under the usual home-directory locations
are considered after the standard locations. The chain order is deterministic;
only the fonts installed on a particular machine vary.

Typical platform choices are:

| Platform | Preferred fonts |
| --- | --- |
| macOS | PingFang SC |
| Windows | Microsoft YaHei, then SimSun |
| Linux | Noto Sans CJK SC |

## PNG and PDF

The PNG renderer draws the classroom map at 2x density. The PDF renderer creates
a single-page A4 image by default, at 144 DPI, and can use the configured paper
size, orientation, margin, and scale. Both use the same process-wide cached
`fontdue` system-font parse.

The PDF contains a compressed raster image. Its text is already drawn into that
image, so output does not depend on a PDF viewer selecting a matching font. This
avoids the old failure mode in which glyph data was separated from the font that
defined it and viewers displayed boxes, dots, or unrelated letters.

If discovery or parsing finds no usable system font, PNG/PDF rendering still
returns a complete image but omits text and emits:

```text
no usable system font found; PNG/PDF text was omitted
```

Install a CJK font before exporting names in Chinese or other non-ASCII scripts:

```bash
# Debian/Ubuntu
sudo apt-get install fonts-noto-cjk

# CentOS/RHEL
sudo yum install google-noto-sans-cjk-fonts
```

## HTML, print HTML, and SVG

These formats retain text and provide CSS or SVG font-family fallbacks. The
print template uses a local sans-serif stack including PingFang SC, Microsoft
YaHei, and Noto Sans CJK SC. The browser or consuming application resolves that
stack when it displays or prints the document; this is separate from PNG/PDF
rasterization.

## Custom fonts

If a school has a licensed font, install the `.ttf` or `.otf` file in the
operating system's font directory, such as `~/Library/Fonts` on macOS or
`~/.fonts` on Linux. PNG/PDF discovery can then use it only when it is one of the
supported discovered paths or is exposed through the platform's expected font
location. SeatTrellis does not copy or distribute the font.

## Compatibility

The strategy is an implementation detail of v2 exports. It does not require a
font package in the repository and does not make export depend on WeasyPrint,
Pango, or Python. The exact raster appearance depends on the selected local
system font, while the discovery order and missing-font warning are stable.
