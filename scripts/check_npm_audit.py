#!/usr/bin/env python3
"""Fail on npm vulnerabilities outside a narrow, expiring allowlist."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ALLOWLIST = ROOT / "security" / "npm-audit-allowlist.json"


def audit(project: Path) -> dict:
    result = subprocess.run(
        ["npm", "audit", "--json"],
        cwd=project,
        capture_output=True,
        text=True,
    )
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"npm audit did not return JSON for {project}: {detail}") from error
    if report.get("error"):
        raise RuntimeError(f"npm audit failed for {project}: {report['error']}")
    return report


def advisory_urls(entry: dict, vulnerabilities: dict, seen: set[str]) -> set[str]:
    urls: set[str] = set()
    for cause in entry.get("via", []):
        if isinstance(cause, dict):
            url = cause.get("url")
            if url:
                urls.add(url)
        elif isinstance(cause, str) and cause not in seen:
            dependency = vulnerabilities.get(cause)
            if dependency is not None:
                urls.update(advisory_urls(dependency, vulnerabilities, seen | {cause}))
    return urls


def locked_version(project: Path, package: str) -> str | None:
    lock = json.loads((project / "package-lock.json").read_text(encoding="utf-8"))
    entry = lock.get("packages", {}).get(f"node_modules/{package}")
    return entry.get("version") if entry else None


def check_project(project: Path, allowlist: dict) -> tuple[list[str], set[str]]:
    report = audit(project)
    vulnerabilities = report.get("vulnerabilities", {})
    if not vulnerabilities:
        print(f"npm audit passed: {project.relative_to(ROOT)} (0 vulnerabilities)")
        return [], set()

    allowed_urls = set(allowlist["advisories"])
    observed_urls: set[str] = set()
    problems = []
    for name, entry in vulnerabilities.items():
        urls = advisory_urls(entry, vulnerabilities, {name})
        observed_urls.update(urls)
        if not urls:
            problems.append(f"{project.name}: {name} has no traceable advisory")
        unexpected = urls - allowed_urls
        if unexpected:
            problems.append(
                f"{project.name}: {name} reaches unapproved advisories: "
                + ", ".join(sorted(unexpected))
            )

    for package, expected in allowlist["packages"].items():
        actual = locked_version(project, package)
        if package in vulnerabilities and actual != expected:
            problems.append(
                f"{project.name}: allowlisted {package} must be {expected}, found {actual}"
            )

    if not problems:
        total = report.get("metadata", {}).get("vulnerabilities", {}).get("total", "?")
        print(
            f"npm audit accepted {total} transitive report entries in {project.name}; "
            f"all trace to {len(observed_urls)} reviewed, mitigated advisories"
        )
    return problems, observed_urls


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("projects", nargs="+", type=Path)
    parser.add_argument("--allowlist", type=Path, default=DEFAULT_ALLOWLIST)
    args = parser.parse_args()

    allowlist = json.loads(args.allowlist.read_text(encoding="utf-8"))
    expiry = dt.date.fromisoformat(allowlist["expires"])
    if expiry < dt.date.today():
        print(f"npm audit allowlist expired on {expiry}", file=sys.stderr)
        return 1

    problems = []
    observed_urls: set[str] = set()
    for raw_project in args.projects:
        project = raw_project if raw_project.is_absolute() else ROOT / raw_project
        try:
            project_problems, project_urls = check_project(project, allowlist)
            problems.extend(project_problems)
            observed_urls.update(project_urls)
        except (OSError, RuntimeError, KeyError, json.JSONDecodeError) as error:
            problems.append(str(error))

    missing = set(allowlist["advisories"]) - observed_urls
    if missing:
        problems.append(
            "allowlist contains resolved advisories; remove them: "
            + ", ".join(sorted(missing))
        )

    if problems:
        print("npm audit policy failed:", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
