//! Local CJK font discovery for PNG/PDF rasterization.
//!
//! PNG and PDF share the same parsed system font. PDF pages contain the rendered
//! image, not a reference to a font on the viewer's computer. A TTC is a collection
//! of distinct faces: its first face is not necessarily Simplified Chinese or a
//! normal weight. We locate the requested face by its actual PostScript name and
//! pass that face's collection index to the rasterizer.
//!
//! Discovery and rendering share one successful process-wide cache. Unreadable,
//! malformed, or unsuitable candidates do not stop the search. Failed discovery
//! has a short retry cooldown rather than a permanent cache, so installing a
//! font can recover later exports without re-reading files for every character.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Quality band of the selected font, retained in export metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontQuality {
    /// A normal-weight PingFang SC or Noto Sans CJK SC face.
    Preferred,
    /// A normal-weight Microsoft YaHei or Heiti SC face.
    Acceptable,
    /// A legacy CJK face; appearance can differ from the editor.
    Fallback,
    /// No usable local CJK font; rasterized text cannot be drawn.
    None,
}

/// Metadata for the font actually selected for local rasterization.
///
/// `pdf_name` is retained for API compatibility; it is the face's real PostScript
/// name, not a promise that the PDF references or embeds that font.
#[derive(Debug, Clone)]
pub struct SystemCjkFont {
    pub pdf_name: String,
    pub file: Option<PathBuf>,
    pub quality: FontQuality,
}

impl SystemCjkFont {
    /// Synthetic metadata for tests / explicit override; it has no raster font.
    pub fn synthetic(name: &str, quality: FontQuality) -> Self {
        Self {
            pdf_name: name.to_string(),
            file: None,
            quality,
        }
    }
}

struct Candidate {
    postscript_name: &'static str,
    quality: FontQuality,
    system_paths: &'static [&'static str],
    user_paths: &'static [&'static str],
}

/// Families are prioritized before installation location, so a user-installed
/// preferred face wins over a legacy system fallback. No fonts are downloaded.
const CANDIDATES: &[Candidate] = &[
    Candidate {
        postscript_name: "PingFangSC-Regular",
        quality: FontQuality::Preferred,
        system_paths: &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Supplemental/PingFang.ttc",
            "/Library/Fonts/PingFang.ttc",
        ],
        user_paths: &["Library/Fonts/PingFang.ttc"],
    },
    Candidate {
        postscript_name: "NotoSansCJKsc-Regular",
        quality: FontQuality::Preferred,
        system_paths: &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
            "/usr/local/share/fonts/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
            "C:\\Windows\\Fonts\\NotoSansCJK-Regular.ttc",
        ],
        user_paths: &[
            "Library/Fonts/NotoSansCJK-Regular.ttc",
            "Library/Fonts/NotoSansCJKsc-Regular.otf",
            ".fonts/NotoSansCJK-Regular.ttc",
            ".local/share/fonts/NotoSansCJK-Regular.ttc",
            ".local/share/fonts/NotoSansCJKsc-Regular.otf",
            "AppData/Local/Microsoft/Windows/Fonts/NotoSansCJK-Regular.ttc",
        ],
    },
    Candidate {
        postscript_name: "MicrosoftYaHei",
        quality: FontQuality::Acceptable,
        system_paths: &[
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
        ],
        user_paths: &[],
    },
    Candidate {
        // This macOS face has OS/2 weight 400 despite "Medium" in its name.
        // The Light collection is neither PingFang nor a normal-weight face.
        postscript_name: "STHeitiSC-Medium",
        quality: FontQuality::Acceptable,
        system_paths: &["/System/Library/Fonts/STHeiti Medium.ttc"],
        user_paths: &[],
    },
    Candidate {
        postscript_name: "WenQuanYiZenHei",
        quality: FontQuality::Fallback,
        system_paths: &[
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/wqy-zenhei/wqy-zenhei.ttc",
        ],
        user_paths: &[".local/share/fonts/wqy-zenhei.ttc"],
    },
    Candidate {
        postscript_name: "ArialUnicodeMS",
        quality: FontQuality::Fallback,
        system_paths: &[
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/Library/Fonts/Arial Unicode.ttf",
            "C:\\Windows\\Fonts\\ARIALUNI.TTF",
        ],
        user_paths: &[],
    },
    Candidate {
        postscript_name: "SimSun",
        quality: FontQuality::Fallback,
        system_paths: &[
            "C:\\Windows\\Fonts\\simsun.ttc",
            "C:\\Windows\\Fonts\\simsun.ttf",
        ],
        user_paths: &[],
    },
];

/// A small coverage sanity check, not a claim to cover every person's name.
const REQUIRED_GLYPHS: &str = "Aa019教室座位张三";

struct LoadedFont {
    metadata: SystemCjkFont,
    font: fontdue::Font,
    // Preserve the same source face for vector outlines. These bytes stay local;
    // exported SVG contains ordinary drawing paths, not an embedded font file.
    bytes: Vec<u8>,
    collection_index: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct ResolvedFace {
    postscript_name: String,
    collection_index: u32,
}

fn candidate_sources(home: Option<&Path>) -> Vec<SystemCjkFont> {
    CANDIDATES
        .iter()
        .flat_map(|candidate| {
            let system = candidate.system_paths.iter().map(PathBuf::from);
            let user = candidate
                .user_paths
                .iter()
                .filter_map(move |path| home.map(|home| home.join(path)));
            system.chain(user).map(|file| SystemCjkFont {
                pdf_name: candidate.postscript_name.to_string(),
                quality: candidate.quality,
                file: Some(file),
            })
        })
        .collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Choose by the face name, never by an assumed collection order. Font metadata
/// is cheap to inspect; only the matching, normal-weight CJK face is rasterized.
fn resolve_face(bytes: &[u8], expected_name: &str) -> Option<ResolvedFace> {
    // A corrupt collection count must not turn local discovery into a huge loop.
    let count = ttf_parser::fonts_in_collection(bytes).unwrap_or(1).min(256);
    (0..count).find_map(|collection_index| {
        let face = ttf_parser::Face::parse(bytes, collection_index).ok()?;
        let postscript_name = face.names().into_iter().find_map(|name| {
            if name.name_id != ttf_parser::name_id::POST_SCRIPT_NAME {
                return None;
            }
            let decoded = name.to_string().or_else(|| {
                name.name
                    .is_ascii()
                    .then(|| String::from_utf8_lossy(name.name).into_owned())
            })?;
            (decoded == expected_name).then_some(decoded)
        })?;
        if !(350..=500).contains(&face.weight().to_number())
            || !REQUIRED_GLYPHS
                .chars()
                .all(|character| face.glyph_index(character).is_some())
        {
            return None;
        }
        Some(ResolvedFace {
            postscript_name,
            collection_index,
        })
    })
}

fn load_source(mut source: SystemCjkFont) -> Option<LoadedFont> {
    let bytes = std::fs::read(source.file.as_ref()?).ok()?;
    let face = resolve_face(&bytes, &source.pdf_name)?;
    let font = fontdue::Font::from_bytes(
        bytes.as_slice(),
        fontdue::FontSettings {
            collection_index: face.collection_index,
            ..fontdue::FontSettings::default()
        },
    )
    .ok()?;
    if !REQUIRED_GLYPHS
        .chars()
        .all(|character| font.has_glyph(character))
    {
        return None;
    }
    source.pdf_name = face.postscript_name;
    Some(LoadedFont {
        metadata: source,
        font,
        bytes,
        collection_index: face.collection_index,
    })
}

fn first_usable<S, T>(
    sources: impl IntoIterator<Item = S>,
    attempt: impl FnMut(S) -> Option<T>,
) -> Option<T> {
    sources.into_iter().find_map(attempt)
}

const FAILED_DISCOVERY_COOLDOWN: Duration = Duration::from_secs(1);

fn cached_success<'a, T>(
    cache: &'a OnceLock<T>,
    retry_after: &Mutex<Option<Instant>>,
    clock: impl Fn() -> Instant,
    load: impl FnOnce() -> Option<T>,
) -> Option<&'a T> {
    if let Some(value) = cache.get() {
        return Some(value);
    }
    let mut retry_after = retry_after
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // A competing render may have completed discovery while this one waited.
    if let Some(value) = cache.get() {
        return Some(value);
    }
    if retry_after.is_some_and(|deadline| clock() < deadline) {
        return None;
    }
    let Some(value) = load() else {
        // Text fitting asks for metrics repeatedly. A missing or corrupt font
        // must not cause a full file search for each character. Start cooling
        // down AFTER a potentially slow read/parse, not before it.
        *retry_after = Some(clock() + FAILED_DISCOVERY_COOLDOWN);
        return None;
    };
    *retry_after = None;
    let _ = cache.set(value);
    cache.get()
}

fn selected_font() -> Option<&'static LoadedFont> {
    static CACHE: OnceLock<LoadedFont> = OnceLock::new();
    static RETRY_AFTER: Mutex<Option<Instant>> = Mutex::new(None);
    cached_success(&CACHE, &RETRY_AFTER, Instant::now, || {
        first_usable(candidate_sources(home_dir().as_deref()), load_source)
    })
}

/// Find metadata for the best font that can actually be parsed and rendered.
/// Discovery and [`load_cjk_font`] always agree about the selected face.
pub fn find_system_cjk_font() -> SystemCjkFont {
    selected_font()
        .map(|loaded| loaded.metadata.clone())
        .unwrap_or_else(|| SystemCjkFont::synthetic("Helvetica", FontQuality::None))
}

/// Reuse the selected CJK raster font. Unsupported and corrupt candidates are
/// skipped. A successful parse is cached; failed discovery retries after a
/// one-second cooldown so missing fonts do not turn each glyph into filesystem IO.
pub fn load_cjk_font() -> Option<&'static fontdue::Font> {
    selected_font().map(|loaded| &loaded.font)
}

/// Draw a complete line with the exact face used by PNG/PDF. Coordinates are
/// points, x increases rightward, y increases upward, and the baseline starts at
/// `(0, 0)`. An SVG writer can translate to its baseline and apply `scale(1,-1)`.
/// Font files are not embedded; only the glyph outlines used by this line leave
/// the process. Return `None` when no usable local font or valid size exists.
pub fn svg_text_outline(text: &str, font_size: f64) -> Option<String> {
    if !font_size.is_finite() || font_size <= 0.0 || font_size > f64::from(f32::MAX) {
        return None;
    }
    text_outline_from(selected_font()?, text, font_size)
}

fn text_outline_from(loaded: &LoadedFont, text: &str, font_size: f64) -> Option<String> {
    let face = ttf_parser::Face::parse(&loaded.bytes, loaded.collection_index).ok()?;
    let mut builder = SvgOutlineBuilder {
        path: String::new(),
        scale: font_size / f64::from(face.units_per_em()),
        offset_x: 0.0,
    };
    for character in text.chars() {
        // Use fontdue's exact charmap selection as well as its advances: fonts
        // with multiple cmap subtables must not pick a different glyph in SVG.
        let glyph = ttf_parser::GlyphId(loaded.font.lookup_glyph_index(character));
        // Whitespace has a positive advance but deliberately has no outline.
        face.outline_glyph(glyph, &mut builder);
        builder.offset_x += f64::from(
            loaded
                .font
                .metrics(character, font_size as f32)
                .advance_width,
        );
    }
    Some(builder.path)
}

struct SvgOutlineBuilder {
    path: String,
    scale: f64,
    offset_x: f64,
}

impl ttf_parser::OutlineBuilder for SvgOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(
            self.path,
            "M{:.4} {:.4}",
            self.offset_x + f64::from(x) * self.scale,
            f64::from(y) * self.scale
        );
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(
            self.path,
            "L{:.4} {:.4}",
            self.offset_x + f64::from(x) * self.scale,
            f64::from(y) * self.scale
        );
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(
            self.path,
            "Q{:.4} {:.4} {:.4} {:.4}",
            self.offset_x + f64::from(x1) * self.scale,
            f64::from(y1) * self.scale,
            self.offset_x + f64::from(x) * self.scale,
            f64::from(y) * self.scale
        );
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(
            self.path,
            "C{:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
            self.offset_x + f64::from(x1) * self.scale,
            f64::from(y1) * self.scale,
            self.offset_x + f64::from(x2) * self.scale,
            f64::from(y2) * self.scale,
            self.offset_x + f64::from(x) * self.scale,
            f64::from(y) * self.scale
        );
    }

    fn close(&mut self) {
        self.path.push('Z');
    }
}

/// Raster export still produces a document if no suitable font exists; callers
/// must surface this warning rather than silently treating omitted text as okay.
pub const NO_USABLE_FONT_WARNING: &str = "no usable system font found; PNG/PDF text was omitted";

/// Non-rasterized formats do not need a local font and should not call this.
pub fn font_unavailable_warning() -> Option<String> {
    load_cjk_font()
        .is_none()
        .then(|| NO_USABLE_FONT_WARNING.to_string())
}

/// Report unsupported characters without copying potentially private names into
/// diagnostic messages. Common CJK coverage does not guarantee rare name glyphs.
pub fn missing_glyph_warning(text: &str) -> Option<String> {
    let font = load_cjk_font()?;
    missing_glyph_warning_with(text, |character| font.has_glyph(character))
}

fn missing_glyph_warning_with(text: &str, has_glyph: impl Fn(char) -> bool) -> Option<String> {
    let missing: HashSet<char> = text
        .chars()
        .filter(|character| !character.is_whitespace() && !has_glyph(*character))
        .collect();
    (!missing.is_empty()).then(|| format!(
        "selected system font does not support {} distinct characters; rendered text may show replacement glyphs",
        missing.len()
    ))
}

/// Injectable variant for callers testing an explicit font source.
pub fn font_warning_with(find: impl FnOnce() -> SystemCjkFont) -> Option<String> {
    load_source(find())
        .is_none()
        .then(|| NO_USABLE_FONT_WARNING.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_chain_prefers_normal_weight_simplified_chinese_faces() {
        let names: Vec<_> = CANDIDATES.iter().map(|c| c.postscript_name).collect();
        assert_eq!(
            &names[..4],
            [
                "PingFangSC-Regular",
                "NotoSansCJKsc-Regular",
                "MicrosoftYaHei",
                "STHeitiSC-Medium"
            ]
        );
        assert!(CANDIDATES.iter().all(|candidate| candidate
            .system_paths
            .iter()
            .all(|path| !path.contains("STHeiti Light"))));
    }

    #[test]
    fn preferred_user_font_precedes_legacy_system_fallback() {
        let sources = candidate_sources(Some(Path::new("/user")));
        let user_noto = sources
            .iter()
            .position(|source| {
                source.file.as_ref().unwrap()
                    == &PathBuf::from("/user/.local/share/fonts/NotoSansCJK-Regular.ttc")
            })
            .unwrap();
        let fallback = sources
            .iter()
            .position(|source| source.pdf_name == "STHeitiSC-Medium")
            .unwrap();
        assert!(user_noto < fallback);
    }

    #[test]
    fn unreadable_or_invalid_candidate_does_not_prevent_trying_the_next_one() {
        let mut attempted = Vec::new();
        let result = first_usable(["unreadable", "invalid", "usable", "unused"], |source| {
            attempted.push(source);
            (source == "usable").then_some(source)
        });
        assert_eq!(result, Some("usable"));
        assert_eq!(attempted, ["unreadable", "invalid", "usable"]);
    }

    #[test]
    fn malformed_font_metadata_is_rejected() {
        assert_eq!(resolve_face(b"not a font", "PingFangSC-Regular"), None);
        assert_eq!(resolve_face(&[], "PingFangSC-Regular"), None);
    }

    #[test]
    fn svg_outline_builder_scales_units_and_preserves_baseline_axis() {
        use ttf_parser::OutlineBuilder;
        let mut builder = SvgOutlineBuilder {
            path: String::new(),
            scale: 0.01,
            offset_x: 12.0,
        };
        builder.move_to(-5.0, -20.0);
        builder.line_to(10.0, 40.0);
        builder.quad_to(20.0, 50.0, 30.0, 60.0);
        builder.curve_to(40.0, 70.0, 50.0, 80.0, 60.0, 90.0);
        builder.close();
        assert_eq!(builder.path, "M11.9500 -0.2000L12.1000 0.4000Q12.2000 0.5000 12.3000 0.6000C12.4000 0.7000 12.5000 0.8000 12.6000 0.9000Z");
    }

    #[test]
    fn svg_outline_rejects_nonpositive_or_nonfinite_font_size() {
        for size in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            assert_eq!(svg_text_outline("张三", size), None);
        }
    }

    #[test]
    fn svg_latin_and_cjk_outlines_use_raster_face_and_advances() {
        let Some(loaded) = selected_font() else {
            return;
        };
        let size = 18.0;
        let latin = text_outline_from(loaded, "A", size).unwrap();
        let cjk = text_outline_from(loaded, "张", size).unwrap();
        let combined = text_outline_from(loaded, "A张", size).unwrap();
        assert!(latin.starts_with('M') && latin.contains('Z'));
        assert!(cjk.starts_with('M') && cjk.contains('Z'));
        assert!(combined.starts_with(&latin));
        assert!(!combined.contains("NaN") && !combined.contains("inf"));
        let face = ttf_parser::Face::parse(&loaded.bytes, loaded.collection_index).unwrap();
        let mut expected_second = SvgOutlineBuilder {
            path: String::new(),
            scale: size / f64::from(face.units_per_em()),
            offset_x: f64::from(loaded.font.metrics('A', size as f32).advance_width),
        };
        face.outline_glyph(
            ttf_parser::GlyphId(loaded.font.lookup_glyph_index('张')),
            &mut expected_second,
        );
        assert_eq!(combined, format!("{latin}{}", expected_second.path));
        assert_eq!(text_outline_from(loaded, " ", size), Some(String::new()));
    }

    #[test]
    fn missing_glyph_warning_deduplicates_without_leaking_roster_text() {
        let warning =
            missing_glyph_warning_with("张张三 A\n\t", |character| character == 'A').unwrap();
        assert!(warning.contains("2 distinct characters"));
        assert!(!warning.contains('张'));
        assert!(!warning.contains('三'));
        assert_eq!(missing_glyph_warning_with(" \n\t", |_| false), None);
        assert_eq!(missing_glyph_warning_with("张三 Aa", |_| true), None);
    }

    #[test]
    fn cache_retries_failure_and_reuses_only_a_successful_load() {
        let cache = OnceLock::new();
        let retry_after = Mutex::new(None);
        let now = Instant::now();
        assert_eq!(
            cached_success(&cache, &retry_after, || now, || None::<usize>),
            None
        );
        assert_eq!(
            cached_success(
                &cache,
                &retry_after,
                || now + FAILED_DISCOVERY_COOLDOWN,
                || Some(42)
            ),
            Some(&42)
        );
        assert_eq!(
            cached_success(&cache, &retry_after, Instant::now, || panic!(
                "cached font was reparsed"
            )),
            Some(&42)
        );
    }

    #[test]
    fn failed_discovery_is_not_repeated_for_each_character() {
        let cache = OnceLock::<usize>::new();
        let retry_after = Mutex::new(None);
        let attempts = std::cell::Cell::new(0);
        let now = Instant::now();
        for _ in 0..10_000 {
            assert_eq!(
                cached_success(
                    &cache,
                    &retry_after,
                    || now,
                    || {
                        attempts.set(attempts.get() + 1);
                        None
                    }
                ),
                None
            );
        }
        assert_eq!(
            attempts.get(),
            1,
            "one missing-font scan per cooldown, not per character"
        );
        assert!(cache.get().is_none(), "failure is not permanently cached");
    }

    #[test]
    fn failure_cooldown_starts_after_a_slow_font_parse_finishes() {
        let cache = OnceLock::<usize>::new();
        let retry_after = Mutex::new(None);
        let start = Instant::now();
        let now = std::cell::Cell::new(start);
        assert_eq!(
            cached_success(
                &cache,
                &retry_after,
                || now.get(),
                || {
                    now.set(start + Duration::from_secs(30));
                    None
                }
            ),
            None
        );
        now.set(start + Duration::from_secs(30) + Duration::from_millis(999));
        assert_eq!(
            cached_success(
                &cache,
                &retry_after,
                || now.get(),
                || panic!("cooldown expired before the slow read finished")
            ),
            None
        );
        now.set(start + Duration::from_secs(31));
        assert_eq!(
            cached_success(&cache, &retry_after, || now.get(), || Some(42)),
            Some(&42)
        );
    }

    #[test]
    fn discovery_and_rasterizer_have_coherent_metadata() {
        let metadata = find_system_cjk_font();
        match load_cjk_font() {
            None => {
                assert_eq!(metadata.quality, FontQuality::None);
                assert_eq!(metadata.pdf_name, "Helvetica");
                assert_eq!(metadata.file, None);
            }
            Some(font) => {
                assert_ne!(metadata.quality, FontQuality::None);
                assert!(metadata.file.as_ref().unwrap().is_file());
                assert!(REQUIRED_GLYPHS
                    .chars()
                    .all(|character| font.has_glyph(character)));
            }
        }
    }

    #[test]
    fn quality_ordering_is_strict() {
        assert!(FontQuality::Preferred < FontQuality::Acceptable);
        assert!(FontQuality::Acceptable < FontQuality::Fallback);
        assert!(FontQuality::Fallback < FontQuality::None);
    }

    #[test]
    fn unusable_font_sources_surface_the_export_warning() {
        let warning =
            font_warning_with(|| SystemCjkFont::synthetic("Helvetica", FontQuality::None));
        assert_eq!(warning.as_deref(), Some(NO_USABLE_FONT_WARNING));
        let warning = font_warning_with(|| SystemCjkFont {
            pdf_name: "Ghost".to_string(),
            file: Some(PathBuf::from(
                "/nonexistent/seattrellis-should-not-exist.ttc",
            )),
            quality: FontQuality::Preferred,
        });
        assert_eq!(warning.as_deref(), Some(NO_USABLE_FONT_WARNING));
    }

    #[test]
    fn font_cache_returns_one_parse_for_the_whole_process() {
        let first = load_cjk_font();
        let second = load_cjk_font();
        assert_eq!(
            first.map(|font| font as *const fontdue::Font),
            second.map(|font| font as *const fontdue::Font)
        );
    }

    #[test]
    fn real_lookup_warning_agrees_with_load_outcome() {
        assert_eq!(
            font_unavailable_warning().is_some(),
            load_cjk_font().is_none()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_heiti_selects_sc_normal_weight_instead_of_first_tc_light_face() {
        let path = PathBuf::from("/System/Library/Fonts/STHeiti Medium.ttc");
        let Ok(bytes) = std::fs::read(&path) else {
            // macOS installations without this legacy font use the other candidates.
            return;
        };
        let selected = resolve_face(&bytes, "STHeitiSC-Medium").unwrap();
        assert_eq!(selected.postscript_name, "STHeitiSC-Medium");
        let face = ttf_parser::Face::parse(&bytes, selected.collection_index).unwrap();
        assert_eq!(face.weight().to_number(), 400);
        let expected_full_name = face.names().into_iter().find_map(|name| {
            (name.name_id == ttf_parser::name_id::FULL_NAME)
                .then(|| name.to_string())
                .flatten()
        });
        assert_eq!(resolve_face(&bytes, "PingFangSC-Regular"), None);
        let loaded = load_source(SystemCjkFont {
            pdf_name: "STHeitiSC-Medium".to_string(),
            file: Some(path),
            quality: FontQuality::Acceptable,
        })
        .unwrap();
        // fontdue selects the first Unicode full-name record; its language can
        // vary. Match the resolved face rather than assuming an English name.
        assert_eq!(loaded.font.name(), expected_full_name.as_deref());
    }
}
