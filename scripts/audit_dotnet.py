#!/usr/bin/env python3
"""Fails on direct or transitive NuGet vulnerabilities of every C# project
(`dotnet list package --vulnerable` itself exits with zero).

Usage: audit_dotnet.py
"""

from __future__ import annotations

import json
import shutil
import subprocess  # nosec B404
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


def vulnerabilities(report: dict[str, Any]) -> list[tuple[str, str, str, str]]:
    """(project, package, severity, advisory) of every vulnerable package of a JSON report."""
    if report.get("problems") or not report.get("projects"):
        raise ValueError(f"the NuGet audit report is incomplete: {report.get('problems')}")
    found = []
    for project in report["projects"]:
        for framework in project.get("frameworks", []):
            for group in ("topLevelPackages", "transitivePackages"):
                for package in framework.get(group, []):
                    for advisory in package.get("vulnerabilities", []):
                        found.append((project["path"], package["id"], advisory["severity"], advisory["advisoryurl"]))
    return found


def projects() -> list[Path]:
    return sorted(
        path
        for directory in ("csharp", "experiments")
        for path in (ROOT / directory).rglob("*.csproj")
        if not {"bin", "obj"} & set(path.parts)
    )


def main() -> None:
    dotnet = shutil.which("dotnet")
    if dotnet is None:
        sys.exit("dotnet is not installed")
    found = []
    for project in projects():
        for arguments in (["restore"], ["list", "package", "--vulnerable", "--include-transitive", "--format", "json"]):
            # Only a tracked project path and fixed options, without a shell.
            output = subprocess.run(  # nosec B603
                [dotnet, arguments[0], str(project), *arguments[1:]], capture_output=True, text=True, check=True
            ).stdout
        found.extend(vulnerabilities(json.loads(output)))
    for project_path, package, severity, advisory in found:
        print(f"{project_path}: {package}: {severity}: {advisory}")
    if found:
        sys.exit("vulnerable NuGet packages found")
    print(f"No vulnerable NuGet packages in {len(projects())} projects")


if __name__ == "__main__":
    main()
