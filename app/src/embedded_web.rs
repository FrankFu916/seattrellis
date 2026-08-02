//! Access to the React workbench compiled into `seattrellis_app`.

include!(concat!(env!("OUT_DIR"), "/embedded_web_assets.rs"));

/// Return an embedded production asset by its normalized, slash-separated
/// path (for example `index.html` or `assets/index.js`).
pub fn get(path: &str) -> Option<&'static [u8]> {
    EMBEDDED_WEB_ASSETS
        .iter()
        .find_map(|(asset_path, bytes)| (*asset_path == path).then_some(*bytes))
}

pub fn has_index() -> bool {
    get("index.html").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_workbench_is_embedded() {
        let index = get("index.html").expect("index.html should be embedded");
        assert!(index.starts_with(b"<!doctype html>") || index.starts_with(b"<!DOCTYPE html>"));
        assert!(EMBEDDED_WEB_ASSETS
            .iter()
            .any(|(path, _)| path.ends_with(".css")));
    }

    #[test]
    fn unknown_assets_are_not_returned() {
        assert!(get("../Cargo.toml").is_none());
        assert!(get("missing.js").is_none());
    }
}
