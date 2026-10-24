#!/usr/bin/env python3
"""Prints a Markdown table per ecosystem with every pinned dependency of the repository and the newest
stable version of its registry (crates.io, NuGet, PyPI, npm, GitHub releases, the Rust and .NET channels).

Usage: python3 experiments/deps/versions_report.py [repository root]  (needs network access and `gh`)
"""

import gzip
import json
import re
import subprocess
import sys
import tomllib
import urllib.request
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
AGENT = {"User-Agent": "Comparisons.RedisVSDoublets versions report"}


def fetch(url: str) -> bytes:
    with urllib.request.urlopen(urllib.request.Request(url, headers=AGENT)) as response:  # nosec B310
        data = response.read()
    return gzip.decompress(data) if data[:2] == b"\x1f\x8b" else data


def stable(version: str) -> bool:
    return re.fullmatch(r"\d+(\.\d+)*", version) is not None


def key(version: str) -> tuple[int, ...]:
    return tuple(int(part) for part in version.split("."))


def crate(name: str) -> str:
    return json.loads(fetch(f"https://crates.io/api/v1/crates/{name}"))["crate"]["max_stable_version"]


def nuget(name: str) -> str:
    index = json.loads(fetch(f"https://api.nuget.org/v3/registration5-gz-semver2/{name.lower()}/index.json"))
    versions = []
    for page in index["items"]:
        items = page.get("items") or json.loads(fetch(page["@id"]))["items"]
        versions += [
            item["catalogEntry"]["version"]
            for item in items
            if item["catalogEntry"].get("listed", True) and stable(item["catalogEntry"]["version"])
        ]
    return max(versions, key=key)


def pypi(name: str) -> str:
    return json.loads(fetch(f"https://pypi.org/pypi/{name}/json"))["info"]["version"]


def npm(name: str) -> str:
    return json.loads(fetch(f"https://registry.npmjs.org/{name}/latest"))["version"]


def release(repository: str) -> tuple[str, str]:
    """Returns the newest release tag of a GitHub repository and the commit it points to."""
    tag = subprocess.run(
        ["gh", "api", f"repos/{repository}/releases/latest", "--jq", ".tag_name"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    sha = subprocess.run(
        ["gh", "api", f"repos/{repository}/commits/{tag}", "--jq", ".sha"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    return tag, sha


def table(title: str, rows: list[tuple[str, str, str, str]]) -> None:
    print(f"\n### {title}\n")
    print("| Dependency | Where | Pinned | Latest | |")
    print("|---|---|---|---|---|")
    for name, where, pinned, latest in rows:
        # A commit or a minor version (Python '3.15') is current when the latest release starts with it.
        current = latest.split()[0] == pinned or latest.startswith(pinned + ".")
        print(f"| {name} | {where} | {pinned} | {latest} | {'✅' if current else '⚠️'} |")


def rust() -> None:
    rows = []
    toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    channel = tomllib.loads(fetch("https://static.rust-lang.org/dist/channel-rust-stable.toml").decode())
    rows.append(("Rust toolchain", "rust-toolchain.toml", toolchain, channel["pkg"]["rust"]["version"].split()[0]))
    workflows = "\n".join(path.read_text() for path in (ROOT / ".github/workflows").glob("*.yml"))
    for name, version in re.findall(r"cargo install (\S+) --version (\S+)", workflows):
        rows.append((name, ".github/workflows", version, crate(name)))
    packages: dict[str, tuple[set[str], set[str]]] = {}
    for lock in sorted(ROOT.glob("**/Cargo.lock")):
        if "target" in lock.parts:
            continue
        where = str(lock.parent.relative_to(ROOT))
        for package in tomllib.loads(lock.read_text())["package"]:
            if package.get("source", "").startswith("registry+"):
                versions, places = packages.setdefault(package["name"], (set(), set()))
                versions.add(package["version"])
                places.add(where)
    for name in sorted(packages):
        versions, places = packages[name]
        rows.append((name, ", ".join(sorted(places)), ", ".join(sorted(versions, key=key)), crate(name)))
    table("Rust (rust-toolchain.toml, cargo install and every Cargo.lock entry)", rows)


def dotnet() -> None:
    rows = []
    sdk = json.loads((ROOT / "csharp/global.json").read_text())["sdk"]["version"]
    releases = json.loads(fetch("https://dotnetcli.blob.core.windows.net/dotnet/release-metadata/releases-index.json"))
    newest = max(
        (entry for entry in releases["releases-index"] if entry["support-phase"] in ("active", "maintenance")),
        key=lambda entry: key(entry["channel-version"]),
    )
    rows.append((".NET SDK", "csharp/global.json", sdk, f"{newest['latest-sdk']} (channel {newest['channel-version']})"))
    packages: dict[str, tuple[set[str], set[str], set[str]]] = {}
    for lock in sorted(ROOT.glob("**/packages.lock.json")):
        if "obj" in lock.parts or "bin" in lock.parts:
            continue
        where = str(lock.parent.relative_to(ROOT))
        for framework in json.loads(lock.read_text())["dependencies"].values():
            for name, entry in framework.items():
                if entry["type"] == "Project":
                    continue
                versions, places, kinds = packages.setdefault(name, (set(), set(), set()))
                versions.add(entry["resolved"])
                places.add(where)
                kinds.add(entry["type"])
    for name in sorted(packages, key=str.lower):
        versions, places, kinds = packages[name]
        where = f"{', '.join(sorted(places))} ({'/'.join(sorted(kinds))})"
        rows.append((name, where, ", ".join(sorted(versions, key=key)), nuget(name)))
    table(".NET (global.json and every packages.lock.json entry)", rows)


def python() -> None:
    rows = []
    workflows = "\n".join(path.read_text() for path in (ROOT / ".github/workflows").glob("*.yml"))
    pinned = sorted(set(re.findall(r"python-version: '([^']+)'", workflows)))
    versions = json.loads(fetch("https://raw.githubusercontent.com/actions/python-versions/main/versions-manifest.json"))
    latest = max((entry["version"] for entry in versions if entry["stable"]), key=key)
    rows.append(("Python", ".github/workflows (python-version)", ", ".join(pinned), latest))
    for requirements in sorted((ROOT / "scripts").glob("requirements*.txt")):
        for line in requirements.read_text().splitlines():
            if "==" in line:
                name, version = line.split("==")
                rows.append((name, str(requirements.relative_to(ROOT)), version, pypi(name)))
    table("Python (setup-python and scripts/requirements*.txt)", rows)


def actions() -> None:
    rows = []
    seen: dict[tuple[str, str], set[str]] = {}
    for workflow in sorted((ROOT / ".github/workflows").glob("*.yml")):
        for action, sha in re.findall(r"uses: ([\w.-]+/[\w.-]+)@([0-9a-f]{40})", workflow.read_text()):
            seen.setdefault((action, sha), set()).add(workflow.name)
    for (action, sha), workflows in sorted(seen.items()):
        tag, latest = release(action)
        rows.append((action, ", ".join(sorted(workflows)), sha[:12], f"{latest[:12]} ({tag})"))
    workflows_text = "\n".join(path.read_text() for path in (ROOT / ".github/workflows").glob("*.yml"))
    for name, version in sorted(set(re.findall(r"npx --yes (?:-p )?((?:@[\w-]+/)?[\w-]+)@([\d.]+)", workflows_text))):
        rows.append((f"{name} (npx)", ".github/workflows", version, npm(name)))
    for name, version in sorted(set(re.findall(r"-p ((?:@[\w-]+/)?[\w-]+)@([\d.]+)", workflows_text))):
        rows.append((f"{name} (npx)", ".github/workflows", version, npm(name)))
    unique = list(dict.fromkeys(rows))
    table("GitHub Actions (every uses: and npx pin of .github/workflows)", unique)


def pre_commit() -> None:
    rows = []
    config = (ROOT / ".pre-commit-config.yaml").read_text()
    for repository, rev in re.findall(r"repo: https://github.com/(\S+)\n\s+rev: (\S+)", config):
        tag, _ = release(repository)
        rows.append((repository, ".pre-commit-config.yaml", rev, tag))
    table("pre-commit (.pre-commit-config.yaml)", rows)


for ecosystem in (rust, dotnet, python, actions, pre_commit):
    ecosystem()
