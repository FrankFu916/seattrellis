import unittest

from report_rust_test_failure import annotation_data, failure_diagnostics


class RustFailureReportTests(unittest.TestCase):
    def test_names_and_assertion_locations_are_reported_without_request_bodies(self):
        diagnostics = failure_diagnostics("""test tests::roundtrip ... FAILED
test tests::roundtrip ... FAILED
---- tests::roundtrip stdout ----
thread 'tests::roundtrip' panicked at crates/example.rs:42:9:
assertion `left == right` failed
private captured request body
""")
        self.assertEqual(len(diagnostics), 3)
        self.assertIn("Failed Rust test: tests::roundtrip", diagnostics)
        self.assertTrue(any("crates/example.rs:42:9" in line for line in diagnostics))
        self.assertFalse(any("request body" in line for line in diagnostics))

    def test_compile_failure_and_missing_output_remain_diagnostic(self):
        self.assertEqual(failure_diagnostics("error[E0425]: unresolved name\n"),
                         ["error[E0425]: unresolved name"])
        self.assertIn("inspect the job log", failure_diagnostics("")[0])
        self.assertEqual(len(failure_diagnostics("\n".join(f"error: {i}" for i in range(30)))), 12)

    def test_annotation_messages_cannot_inject_commands(self):
        self.assertEqual(annotation_data("100%\r\n::warning::fake"),
                         "100%25%0D%0A::warning::fake")
