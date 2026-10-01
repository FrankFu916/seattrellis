//! Regression: Vite may replace hashed assets after Cargo runs the build script.
//! Rustdoc must still compile the original immutable embedded asset manifest.
use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn checked(command: &mut Command) {
    let output = command.output().expect("test subprocess starts");
    assert!(
        output.status.success(),
        "subprocess failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn original_embedded_manifest_compiles_after_frontend_assets_are_replaced() {
    let fixture = Fixture(std::env::temp_dir().join(format!(
            "seattrellis-asset-snapshot-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    let manifest = fixture.0.join("crates/server");
    let dist = fixture.0.join("clients/web/dist");
    let output = fixture.0.join("build-output");
    fs::create_dir_all(&manifest).unwrap();
    fs::create_dir_all(dist.join("assets")).unwrap();
    fs::create_dir_all(&output).unwrap();
    fs::write(dist.join("index.html"), b"first page").unwrap();
    fs::write(dist.join("assets/first-hash.js"), b"first script").unwrap();
    let executable = fixture
        .0
        .join(format!("asset-build{}", std::env::consts::EXE_SUFFIX));
    checked(
        Command::new("rustc")
            .arg("--edition=2021")
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
            .arg("-o")
            .arg(&executable),
    );
    checked(
        Command::new(&executable)
            .env("CARGO_MANIFEST_DIR", &manifest)
            .env("OUT_DIR", &output),
    );
    let first = fixture.0.join("first-manifest.rs");
    fs::copy(output.join("embedded_web_assets.rs"), &first).unwrap();
    // Replace precisely the checkout file referenced by the old implementation.
    fs::remove_file(dist.join("assets/first-hash.js")).unwrap();
    fs::write(dist.join("assets/second-hash.js"), b"second script").unwrap();
    fs::write(dist.join("index.html"), b"second page").unwrap();
    checked(
        Command::new(&executable)
            .env("CARGO_MANIFEST_DIR", &manifest)
            .env("OUT_DIR", &output),
    );
    let reader = fixture.0.join("reader.rs");
    fs::write(&reader,format!("include!({:?}); fn main() {{ assert!(EMBEDDED_WEB_ASSETS.iter().any(|(name,bytes)| *name==\"assets/first-hash.js\" && *bytes==b\"first script\")); assert!(EMBEDDED_WEB_ASSETS.iter().any(|(name,bytes)| *name==\"index.html\" && *bytes==b\"first page\")); }}",first.to_string_lossy())).unwrap();
    let binary = fixture
        .0
        .join(format!("reader{}", std::env::consts::EXE_SUFFIX));
    checked(
        Command::new("rustc")
            .arg("--edition=2021")
            .arg(&reader)
            .arg("-o")
            .arg(&binary),
    );
    checked(&mut Command::new(&binary));
    // Published crates use the same snapshot contract from vendored web-dist.
    fs::create_dir_all(manifest.join("web-dist")).unwrap();
    fs::write(manifest.join("web-dist/index.html"), b"vendored page").unwrap();
    checked(
        Command::new(&executable)
            .env("CARGO_MANIFEST_DIR", &manifest)
            .env("OUT_DIR", &output),
    );
    assert!(!fs::read_to_string(output.join("embedded_web_assets.rs"))
        .unwrap()
        .contains("second-hash.js"));
}
