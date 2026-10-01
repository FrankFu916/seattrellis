//! XML 1.0 character and escaping policy shared by SVG and Office writers.

use crate::render::SeatingGrid;

pub(crate) const SANITIZATION_WARNING: &str = "Some unsupported control or Unicode characters were removed from export text. Check the source data for complete values.";

// Rust strings cannot contain surrogates, but may contain both forbidden BMP
// noncharacters. Supplementary-plane noncharacters are legal XML 1.0 scalars.
fn is_xml_char(character: char) -> bool {
    matches!(character, '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

pub(crate) fn escape_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars().filter(|character| is_xml_char(*character)) {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

pub(crate) fn sanitize_grid(grid: &mut SeatingGrid) -> bool {
    fn sanitize(text: &mut String) -> bool {
        let length = text.len();
        text.retain(is_xml_char);
        text.len() != length
    }
    let mut changed = sanitize(&mut grid.title) | sanitize(&mut grid.subtitle);
    for cell in &mut grid.cells {
        changed |= sanitize(&mut cell.seat_id);
        for text in [&mut cell.student, &mut cell.student_key, &mut cell.detail]
            .into_iter()
            .flatten()
        {
            changed |= sanitize(text);
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_policy_preserves_allowed_unicode_and_escapes_markup() {
        assert_eq!(
            escape_text("<&>\"'\0\u{B}\u{FFFE}\u{FFFF}\t\n\r中文😀\u{1FFFE}"),
            "&lt;&amp;&gt;&quot;&apos;\t\n\r中文😀\u{1FFFE}"
        );
    }
}
