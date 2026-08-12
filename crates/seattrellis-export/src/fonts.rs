//! System CJK font discovery (PD-D12-PDFCJK, M5-A3).
//!
//! The PDF writer references a system CJK font by name instead of embedding
//! fonts: the generator enumerates the usual platform font directories, picks
//! the best available CJK font by a quality-priority chain (PingFang SC →
//! Noto Sans CJK SC → Microsoft YaHei → SimSun → any CJK), and the viewer
//! substitutes its own font when the name is missing (any GUI-capable device
//! has a CJK font). Quality below "preferred" surfaces an export warning.
//!
//! Deterministic by design: the chain order is fixed; only *which* font
//! exists is environment-dependent (same as any system-font reference).

use std::path::{Path, PathBuf};

/// Quality band of the discovered font (drives the export warning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontQuality {
    /// PingFang SC or Noto Sans CJK SC: preferred rendering.
    Preferred,
    /// Microsoft YaHei: acceptable.
    Acceptable,
    /// SimSun / generic CJK fallback: warn "effect may be below expectation".
    Fallback,
    /// No CJK font found at all: keep the ASCII-only Helvetica path.
    None,
}

/// A discovered system CJK font: the PostScript name to reference in the PDF
/// plus the file path (used later by the PNG rasterizer, M5-A4).
#[derive(Debug, Clone)]
pub struct SystemCjkFont {
    pub pdf_name: String,
    pub file: Option<PathBuf>,
    pub quality: FontQuality,
}

impl SystemCjkFont {
    /// A synthetic font for tests / explicit override.
    pub fn synthetic(name: &str, quality: FontQuality) -> Self {
        SystemCjkFont {
            pdf_name: name.to_string(),
            file: None,
            quality,
        }
    }
}

/// Candidate fonts in quality-priority order. Each entry maps a known font
/// file (per-platform variants) to its PostScript name and quality band.
const CANDIDATES: &[(&str, FontQuality, &[&str])] = &[
    // PingFang SC (macOS)
    (
        "PingFangSC-Regular",
        FontQuality::Preferred,
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/Library/Fonts/PingFang.ttc",
        ],
    ),
    // Noto Sans CJK SC (Linux/Windows installs)
    (
        "NotoSansCJKsc-Regular",
        FontQuality::Preferred,
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/local/share/fonts/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "C:\\Windows\\Fonts\\NotoSansCJK-Regular.ttc",
        ],
    ),
    // Microsoft YaHei (Windows)
    (
        "MicrosoftYaHei",
        FontQuality::Acceptable,
        &[
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
        ],
    ),
    // SimSun (Windows legacy)
    (
        "SimSun",
        FontQuality::Fallback,
        &[
            "C:\\Windows\\Fonts\\simsun.ttc",
            "C:\\Windows\\Fonts\\simsun.ttf",
        ],
    ),
];

/// Home-directory CJK fonts (user-installed, lower priority).
const USER_CANDIDATES: &[(&str, FontQuality, &[&str])] = &[
    (
        "PingFangSC-Regular",
        FontQuality::Preferred,
        &["Library/Fonts/PingFang.ttc"],
    ),
    (
        "NotoSansCJKsc-Regular",
        FontQuality::Preferred,
        &[
            ".fonts/NotoSansCJK-Regular.ttc",
            ".local/share/fonts/NotoSansCJK-Regular.ttc",
        ],
    ),
];

fn exists(path: &Path) -> bool {
    path.is_file()
}

/// Find the best available system CJK font.
pub fn find_system_cjk_font() -> SystemCjkFont {
    for (name, quality, paths) in CANDIDATES {
        for candidate in *paths {
            if exists(Path::new(candidate)) {
                return SystemCjkFont {
                    pdf_name: name.to_string(),
                    file: Some(PathBuf::from(candidate)),
                    quality: *quality,
                };
            }
        }
    }
    if let Some(home) = home_dir() {
        for (name, quality, paths) in USER_CANDIDATES {
            for candidate in *paths {
                let path = home.join(candidate);
                if path.is_file() {
                    return SystemCjkFont {
                        pdf_name: name.to_string(),
                        file: Some(path),
                        quality: *quality,
                    };
                }
            }
        }
    }
    SystemCjkFont {
        pdf_name: "Helvetica".to_string(),
        file: None,
        quality: FontQuality::None,
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Load the discovered CJK font for PNG/PDF export-time rasterization.
/// Returns `None` when no supported CJK font file is available.
pub fn load_cjk_font() -> Option<fontdue::Font> {
    let font = find_system_cjk_font();
    let file = font.file?;
    let bytes = std::fs::read(&file).ok()?;
    fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_chain_has_fixed_priority_order() {
        // The chain order is the contract: PingFang/Noto before YaHei before
        // SimSun. A regression in ordering changes rendering everywhere.
        let names: Vec<&str> = CANDIDATES.iter().map(|(n, _, _)| *n).collect();
        assert_eq!(
            names,
            vec![
                "PingFangSC-Regular",
                "NotoSansCJKsc-Regular",
                "MicrosoftYaHei",
                "SimSun"
            ]
        );
    }

    #[test]
    fn discovery_returns_a_font_or_none_without_panicking() {
        // On any machine this must terminate with a coherent result; the
        // quality band drives the warning logic downstream.
        let font = find_system_cjk_font();
        match font.quality {
            FontQuality::None => assert_eq!(font.pdf_name, "Helvetica"),
            _ => assert!(!font.pdf_name.is_empty()),
        }
    }

    #[test]
    fn quality_ordering_is_strict() {
        use std::cmp::Ordering;
        let a = FontQuality::Preferred;
        let b = FontQuality::Acceptable;
        let c = FontQuality::Fallback;
        let d = FontQuality::None;
        // Ord follows declaration order (derive).
        assert!(a < b && b < c && c < d);
        let _ = Ordering::Equal;
    }
}
