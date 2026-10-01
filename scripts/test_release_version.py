import json
import tempfile
import unittest
from pathlib import Path

from check_release_version import problems, release_version


class ReleaseVersionTests(unittest.TestCase):
    def test_requires_a_numeric_v2_release_tag(self):
        self.assertEqual(release_version("desktop-v2.1.0"), "2.1.0")
        self.assertEqual(release_version("refs/tags/v2.1.0"), "2.1.0")
        self.assertEqual(release_version("refs/tags/desktop-v2.1.0-rc.1"), "2.1.0-rc.1")
        for tag in ["main", "v1.9.0", "v2.next", "v2.1.0/path", "refs/heads/v2.1.0"]:
            with self.assertRaises(ValueError):
                release_version(tag)

    def test_rejects_each_independently_drifting_product(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in ["crates/core/Cargo.toml", "xtask/Cargo.toml", "app/Cargo.toml", "app/src-tauri/Cargo.toml"]:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('[package]\nversion = "2.1.0"\n')
            for relative in ["clients/web/package.json", "website/package.json", "app/src-tauri/tauri.conf.json"]:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps({"version": "2.1.0", "bundle": {"windows": {"wix": {"version": "2.1.0.0"}}}}))
                if relative.endswith("package.json"):
                    path.with_name("package-lock.json").write_text(json.dumps({"version": "2.1.0", "packages": {"": {"version": "2.1.0"}}}))
            self.assertEqual(problems(root, "v2.1.0"), [])
            self.assertEqual(problems(root, "refs/tags/v2.1.0"), [])
            app = root / "app/Cargo.toml"
            app.write_text('[package]\nversion = "2.2.0"\n')
            self.assertEqual(problems(root, "v2.1.0"), ["app/Cargo.toml: 2.2.0 != 2.1.0"])
            app.write_text('[package]\nversion = "2.1.0"\n')
            for directory in ["clients/web", "website"]:
                lock = root / directory / "package-lock.json"
                for field in ["version", "package_root"]:
                    document = {"version": "2.1.0", "packages": {"": {"version": "2.1.0"}}}
                    if field == "version":
                        document["version"] = "2.2.0"
                    else:
                        document["packages"][""]["version"] = "2.2.0"
                    lock.write_text(json.dumps(document))
                    errors = problems(root, "v2.1.0")
                    self.assertEqual(len(errors), 1)
                    self.assertIn(f"{directory}/package-lock.json", errors[0])
                lock.write_text(json.dumps({"version": "2.1.0", "packages": {"": {"version": "2.1.0"}}}))
            (root / "clients/web/package.json").write_text('{"version":"2.2.0"}')
            self.assertEqual(len(problems(root, "v2.1.0")), 1)
            self.assertTrue(problems(root, "v2.2.0"))
