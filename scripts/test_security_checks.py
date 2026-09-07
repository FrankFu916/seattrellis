from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from check_npm_audit import advisory_urls
from check_repository_hygiene import (
    _unsafe_image_format,
    unsafe_documentation_images,
)


class DocumentationImageSafetyTests(unittest.TestCase):
    def test_rejects_every_affected_signature_family(self) -> None:
        samples = {
            b"icns" + bytes(28): "ICNS",
            b"\xff\x0a" + bytes(30): "JPEG XL codestream",
            b"\x00\x00\x00\x0cJXL \r\n\x87\n" + bytes(20): "JPEG XL container",
            b"\x00\x00\x00\x0cjP  \r\n\x87\n" + bytes(20): "JPEG 2000",
            b"\x00\x00\x00\x18ftypavif" + bytes(20): "ISO BMFF (avif)",
            b"\x00\x00\x00\x18ftypheic" + bytes(20): "ISO BMFF (heic)",
        }
        for data, expected in samples.items():
            with self.subTest(expected=expected):
                self.assertEqual(_unsafe_image_format(data), expected)

    def test_allows_supported_documentation_assets(self) -> None:
        self.assertIsNone(_unsafe_image_format(b"\x89PNG\r\n\x1a\n" + bytes(24)))
        self.assertIsNone(_unsafe_image_format(b"<svg xmlns='http://www.w3.org/2000/svg'>"))

    def test_checks_docs_but_not_native_application_icons(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs").mkdir()
            (root / "app").mkdir()
            (root / "docs" / "hidden.png").write_bytes(b"icns" + bytes(28))
            (root / "app" / "icon.icns").write_bytes(b"icns" + bytes(28))
            problems = unsafe_documentation_images(
                root,
                ["docs/hidden.png", "app/icon.icns"],
            )
        self.assertEqual(len(problems), 1)
        self.assertIn("docs/hidden.png", problems[0])


class NpmAuditPolicyTests(unittest.TestCase):
    def test_resolves_transitive_advisory_chain(self) -> None:
        vulnerabilities = {
            "framework": {"via": ["loader"]},
            "loader": {"via": ["image-size"]},
            "image-size": {
                "via": [
                    {"url": "https://github.com/advisories/GHSA-example"},
                ]
            },
        }
        self.assertEqual(
            advisory_urls(vulnerabilities["framework"], vulnerabilities, {"framework"}),
            {"https://github.com/advisories/GHSA-example"},
        )

    def test_cycle_without_advisory_is_not_silently_allowed(self) -> None:
        vulnerabilities = {
            "a": {"via": ["b"]},
            "b": {"via": ["a"]},
        }
        self.assertEqual(advisory_urls(vulnerabilities["a"], vulnerabilities, {"a"}), set())


if __name__ == "__main__":
    unittest.main()
