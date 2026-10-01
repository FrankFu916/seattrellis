"""Regression checks for publication gates; actionlint validates full YAML syntax.

Read only job boundaries, scalar job-level uses/needs, and their simple lists.
No third-party YAML runtime is needed by the repository hygiene CI job.
"""
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github/workflows"


def jobs(name):
    text = (WORKFLOWS / name).read_text().split("\njobs:\n", 1)[1]
    boundaries = list(re.finditer(r"^  ([\w-]+):\n", text, re.MULTILINE))
    return {
        match.group(1): text[match.end():boundaries[index + 1].start() if index + 1 < len(boundaries) else len(text)]
        for index, match in enumerate(boundaries)
    }


def needs(job):
    match = re.search(r"^    needs:([^\n]*)\n", job, re.MULTILINE)
    if not match:
        return set()
    if match.group(1).strip():
        return set(re.findall(r"[\w-]+", match.group(1)))
    lines = job[match.end():].splitlines()
    result = set()
    for line in lines:
        if not line.startswith("      - "):
            break
        result.add(line.strip().removeprefix("- "))
    return result


class ReleaseWorkflowTests(unittest.TestCase):
    def test_every_publication_waits_for_the_complete_gate(self):
        rust = jobs("rust.yml")
        self.assertTrue({
            "build-binaries", "release-version", "web-quality", "security-quality", "test",
            "contract-drift", "fmt", "dependency-audit", "long-run-gates", "fuzz-targets", "no-python-runtime",
        }.issubset(needs(rust["publish-assets"])))
        tauri = jobs("tauri.yml")
        self.assertIn("quality", needs(tauri["bundle"]))
        self.assertIn("bundle", needs(tauri["desktop-checksums"]))
        self.assertIn("uses: ./.github/workflows/rust.yml", tauri["quality"])
        self.assertIn("format('refs/tags/{0}'", tauri["quality"])
        self.assertIn("inputs.ref == ''", rust["build-binaries"])
        self.assertIn("inputs.ref == ''", rust["publish-assets"])
        self.assertTrue({"web-unit", "web-e2e-rust"}.issubset(jobs("tests.yml")))
        self.assertTrue({"package-hygiene", "secret-scan"}.issubset(jobs("security.yml")))

    def test_checkout_release_refs_and_frontend_build_tools_are_explicit(self):
        for name in ["rust.yml", "tests.yml", "security.yml"]:
            text = (WORKFLOWS / name).read_text()
            checkouts = re.findall(r"uses: actions/checkout@[^\n]*\n\s+with:\n\s+ref: ([^\n]+)", text)
            self.assertTrue(checkouts, name)
            for ref in checkouts:
                self.assertIn("inputs.ref", ref, name)
                self.assertIn("format('refs/tags/{0}'", ref, name)
        for job_name, job in jobs("rust.yml").items():
            if "npm ci" in job:
                self.assertIn("uses: actions/setup-node@", job, job_name)
                self.assertRegex(job, r'node-version: "?24"?\n', job_name)
                self.assertIn("cache-dependency-path: clients/web/package-lock.json", job, job_name)

    def test_reusable_workflow_calls_do_not_form_a_cycle(self):
        def walk(name, ancestors):
            self.assertNotIn(name, ancestors, "reusable workflow recursion")
            for job in jobs(name).values():
                call = re.search(r"^    uses: \./\.github/workflows/([\w.-]+)\s*$", job, re.MULTILINE)
                if call:
                    walk(call.group(1), ancestors + [name])
        walk("tauri.yml", [])

    def test_nested_workflows_have_separate_concurrency_namespaces(self):
        # github.workflow is inherited from the top-level caller. A shared
        # namespace plus cancel-in-progress could cancel the calling workflow.
        prefixes = []
        for name in ["rust.yml", "tauri.yml", "tests.yml", "security.yml"]:
            text = (WORKFLOWS / name).read_text()
            group = re.search(r"^  group: ([^\n]+)", text, re.MULTILINE).group(1)
            prefixes.append(group.split("${{", 1)[0])
        self.assertEqual(len(set(prefixes)), len(prefixes))
