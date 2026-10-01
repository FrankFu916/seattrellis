"""Regression checks for publication gates; actionlint validates full YAML syntax.

Read only job boundaries, scalar job-level uses/needs, and their simple lists.
No third-party YAML runtime is needed by the repository hygiene CI job.
"""
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github/workflows"


def job_blocks(text):
    text = text.split("\njobs:\n", 1)[1]
    boundaries = list(re.finditer(r"^  ([\w-]+):\n", text, re.MULTILINE))
    return {
        match.group(1): text[match.end():boundaries[index + 1].start() if index + 1 < len(boundaries) else len(text)]
        for index, match in enumerate(boundaries)
    }


def jobs(name):
    return job_blocks((WORKFLOWS / name).read_text())


def permission_policy(text, indent=0):
    """Read the explicit permission map; omitted keys in a map mean none."""
    match = re.search(rf"^{' ' * indent}permissions:([^\n]*)\n", text, re.MULTILINE)
    if not match:
        return None
    scalar = match.group(1).strip()
    if scalar in {"read-all", "write-all", "{}"}:
        return {"*": {"read-all": 1, "write-all": 2, "{}": 0}[scalar]}
    if scalar:
        raise ValueError(f"unsupported permission declaration: {scalar}")
    policy = {"*": 0}
    for line in text[match.end():].splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        entry = re.fullmatch(rf"{' ' * (indent + 2)}([\w-]+): (none|read|write)", line)
        if not entry:
            break
        policy[entry.group(1)] = {"none": 0, "read": 1, "write": 2}[entry.group(2)]
    return policy


def check_nested_permissions(workflows, name, ceiling=None, ancestors=()):
    """Check all callee jobs, even jobs skipped by an event/ref condition.

    A workflow permission map supplies job defaults, not a ceiling on its own
    explicitly overridden jobs. Only an upstream reusable caller imposes that
    ceiling; each nested call passes its own effective job permissions onward.
    """
    if name in ancestors:
        raise ValueError(f"reusable workflow recursion: {name}")
    text = workflows[name]
    default = permission_policy(text)
    for job_name, job in job_blocks(text).items():
        policy = permission_policy(job, 4)
        if policy is None:
            policy = default if default is not None else ceiling
        if ceiling is not None and policy is not None:
            for permission in set(policy) | set(ceiling):
                requested = policy.get(permission, policy["*"])
                allowed = ceiling.get(permission, ceiling["*"])
                if requested > allowed:
                    raise ValueError(f"{name}/{job_name} elevates {permission} above its caller ceiling")
        call = re.search(r"^    uses: \./\.github/workflows/([\w.-]+)\s*$", job, re.MULTILINE)
        if call:
            check_nested_permissions(workflows, call.group(1), policy, ancestors + (name,))


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

    def test_nested_workflows_never_elevate_their_callers_permissions(self):
        workflows = {path.name: path.read_text() for path in WORKFLOWS.glob("*.yml")}
        for name in ["rust.yml", "tauri.yml"]:
            with self.subTest(workflow=name):
                check_nested_permissions(workflows, name)

    def test_skipped_publishers_still_require_a_sufficient_caller_ceiling(self):
        workflows = {
            "caller.yml": """name: Caller
permissions:
  contents: read
jobs:
  quality:
    uses: ./.github/workflows/quality.yml
""",
            "quality.yml": """name: Quality
permissions:
  contents: read
jobs:
  test:
    runs-on: ubuntu-latest
  publish:
    if: false
    permissions:
      contents: write
    runs-on: ubuntu-latest
""",
        }
        with self.assertRaisesRegex(ValueError, "quality.yml/publish elevates contents"):
            check_nested_permissions(workflows, "caller.yml")
        workflows["caller.yml"] = workflows["caller.yml"].replace("contents: read", "contents: write")
        check_nested_permissions(workflows, "caller.yml")

    def test_nested_calls_respect_intermediate_permission_reduction(self):
        workflows = {
            "caller.yml": """name: Caller
permissions:
  contents: write
jobs:
  quality:
    uses: ./.github/workflows/quality.yml
""",
            "quality.yml": """name: Quality
permissions:
  contents: read
jobs:
  nested:
    uses: ./.github/workflows/leaf.yml
""",
            "leaf.yml": """name: Leaf
jobs:
  publish:
    permissions:
      contents: write
    runs-on: ubuntu-latest
""",
        }
        with self.assertRaisesRegex(ValueError, "leaf.yml/publish elevates contents"):
            check_nested_permissions(workflows, "caller.yml")
        workflows["quality.yml"] = workflows["quality.yml"].replace("contents: read", "contents: write")
        check_nested_permissions(workflows, "caller.yml")
