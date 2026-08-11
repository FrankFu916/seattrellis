//! Fuzz-style roster importer bombardment (plan §11.4 CSV importer target):
//! arbitrary bytes into `parse_roster_csv` must never panic, never hang, and
//! never touch the filesystem (the importer is pure parsing by contract).

use proptest::prelude::*;
use seattrellis_io::roster::parse_roster_csv;

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

    #[test]
    fn csv_importer_never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = parse_roster_csv(&bytes);
    }

    #[test]
    fn csv_importer_handles_path_like_cells(cells in prop::collection::vec(
        proptest::string::string_regex(r"[a-zA-Z0-9/\\.,:; _-]{0,40}").unwrap(),
        0..40,
    )) {
        // Path-like cells (potentially containing traversal tokens) must
        // parse without panic; the importer has no file access.
        let doc = cells.join(",");
        let _ = parse_roster_csv(doc.as_bytes());
    }

    #[test]
    fn csv_importer_handles_bom_and_quotes(bytes in prop::collection::vec(
        proptest::string::string_regex(r#"[\x20-\x7E",\r\n]{0,60}"#).unwrap(),
        0..30,
    )) {
        let doc = format!("\u{feff}{}", bytes.join("\n"));
        let _ = parse_roster_csv(doc.as_bytes());
    }
}
