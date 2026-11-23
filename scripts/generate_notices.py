#!/usr/bin/env python3
"""Collect dependency-provided license/notice texts for the Linux x86_64 graph."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def render(offline=False):
    command = ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", "x86_64-unknown-linux-gnu"]
    if offline:
        command.append("--offline")
    data = json.loads(subprocess.check_output(command, cwd=ROOT, text=True))
    packages = sorted((p for p in data["packages"] if p["name"] != "tor-relay-monitor"), key=lambda p: (p["name"], p["version"]))
    lines = ["# Third-party notices", "",
        "Generated from Cargo.lock by `python3 scripts/generate_notices.py`.",
        "Scope: the Linux x86_64 dependency graph, including development/build",
        "dependencies. TRM's own license is in LICENSE. Dependency-provided texts",
        "are retained as supplied; license expressions describe alternatives where",
        "applicable. This inventory is not an independent legal review and does not",
        "cover platform graphs not shipped here.", "", f"Packages: {len(packages)}.", ""]
    for package in packages:
        directory = Path(package["manifest_path"]).parent
        files = set()
        for path in directory.iterdir():
            if path.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")):
                if path.is_file():
                    files.add(path)
                elif path.is_dir():
                    files.update(p for p in path.rglob("*") if p.is_file())
        if package.get("license_file"):
            files.add(directory / package["license_file"])
        if not files or not package.get("license"):
            raise ValueError(f"Missing license declaration/text for {package['name']}")
        name, version = package["name"], package["version"]
        lines.extend([f"## {name} {version}", "", f"Declared license: `{package['license']}`.", "",
            f"Source package: https://crates.io/crates/{name}/{version}", ""])
        for path in sorted(files):
            relative = path.resolve().relative_to(directory.resolve())
            lines.extend([f"### {relative.as_posix()}", "", "````````text", path.read_text(encoding="utf-8").rstrip(), "````````", ""])
    return "\n".join(lines)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    contents = render(args.offline)
    destination = ROOT / "THIRD_PARTY_NOTICES.md"
    if args.check:
        if not destination.is_file() or destination.read_text(encoding="utf-8") != contents:
            raise SystemExit("Third-party notices need regeneration")
        print("Third-party notices match the Linux dependency graph")
    else:
        destination.write_text(contents, encoding="utf-8", newline="\n")
        print(f"Generated {destination.name}")
