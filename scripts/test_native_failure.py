import unittest

from report_native_failure import failure_diagnostics


class NativeFailureReportTests(unittest.TestCase):
    def test_swift_compiler_and_xctest_errors_are_public_without_captured_bodies(self):
        output = """/workspace/Sources/App.swift:42:9: error: missing argument
/workspace/Tests/FlowTests.swift:19: error: FlowTests.testReopen : assertion failed
Test Case '-[FlowTests testReopen]' failed (0.2 seconds).
captured sensitive request body
"""
        diagnostics = failure_diagnostics(output)
        self.assertEqual(len(diagnostics), 3)
        self.assertTrue(any("App.swift:42:9" in item for item in diagnostics))
        self.assertTrue(any("testReopen" in item for item in diagnostics))
        self.assertFalse(any("sensitive request" in item for item in diagnostics))

    def test_linker_errors_are_bounded_and_duplicates_removed(self):
        self.assertEqual(failure_diagnostics("error: link command failed\n" * 3),
                         ["error: link command failed"])
        self.assertEqual(len(failure_diagnostics("\n".join(f"error: {i}" for i in range(30)))), 12)
        self.assertIn("underlying tool error", failure_diagnostics("")[0])
