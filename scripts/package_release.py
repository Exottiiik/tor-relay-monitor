#!/usr/bin/env python3
"""Package the project from an explicit source allowlist; requires Python 3.11+."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
ROOT_FILES = (
    "Cargo.toml", "Cargo.lock", "README.md", "LICENSE", "AGENTS.md",
    "CHANGELOG.md", "CONTRIBUTING.md", "SECURITY.md", "THIRD_PARTY_NOTICES.md", "config.example.toml",
    ".gitignore", ".gitattributes", ".editorconfig",
)
SOURCE_DIRS = ("src", "tests", "scripts", "docs", ".github")


def version(root):
    value = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", value):
        raise ValueError("Unsafe or unsupported package version")
    return value


def source_files(root):
    files = [root / name for name in ROOT_FILES]
    for directory in SOURCE_DIRS:
        for path in (root / directory).rglob("*"):
            if "__pycache__" in path.parts:
                continue
            if path.is_symlink():
                raise ValueError(f"Source symlinks are not packaged: {path.relative_to(root)}")
            if path.is_file():
                files.append(path)
    for path in files:
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Missing or non-regular source file: {path.relative_to(root)}")
    return sorted(files)


def package(root, output, binary_directory=None, source_only=False):
    release_version = version(root)
    prefix = f"tor-relay-monitor-{release_version}"
    files = source_files(root)
    binary = None
    if not source_only:
        if binary_directory is None:
            binary_directory = Path(os.environ.get("CARGO_TARGET_DIR", root / "target")) / "release"
        binary = Path(binary_directory).resolve() / "trm"
        result = subprocess.run([str(binary), "--version"], capture_output=True, text=True, check=True)
        if result.stdout.strip() != f"trm {release_version}":
            raise ValueError("Binary version does not match Cargo.toml; build the release first")
        # Do not accidentally ship a Windows or non-x86_64 executable under a Linux name.
        with binary.open("rb") as contents:
            header = contents.read(20)
        if header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\x3e\x00":
            raise ValueError("Expected a Linux x86_64 ELF executable")
    output.mkdir(parents=True, exist_ok=True)
    source = output / f"{prefix}-source.zip"
    with zipfile.ZipFile(source, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            archive.write(path, f"{prefix}/{path.relative_to(root).as_posix()}")
    artifacts = [source]
    if binary is not None:
        linux = output / f"{prefix}-linux-x86_64.tar.gz"
        with tarfile.open(linux, "w:gz") as archive:
            entry = archive.gettarinfo(str(binary), arcname=f"{prefix}/trm")
            entry.mode, entry.uid, entry.gid, entry.uname, entry.gname = 0o755, 0, 0, "", ""
            with binary.open("rb") as contents:
                archive.addfile(entry, contents)
            alias = tarfile.TarInfo(f"{prefix}/tor-relay-monitor")
            alias.type, alias.linkname, alias.mode = tarfile.SYMTYPE, "trm", 0o755
            archive.addfile(alias)
            for path in files:
                if path.suffix == ".md" or path.name in ("LICENSE", "config.example.toml"):
                    archive.add(path, arcname=f"{prefix}/{path.relative_to(root).as_posix()}")
        artifacts.append(linux)
    with (output / "SHA256SUMS").open("w", encoding="utf-8", newline="\n") as checksums:
        for path in artifacts:
            with path.open("rb") as contents:
                digest = hashlib.file_digest(contents, "sha256").hexdigest()
            checksums.write(f"{digest}  {path.name}\n")
            print(f"{path.name}: {path.stat().st_size} bytes")
    return artifacts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist/release")
    parser.add_argument("--binary-dir", type=Path, help="Directory containing the release trm executable")
    parser.add_argument("--source-only", action="store_true", help="Create the source archive only")
    args = parser.parse_args()
    package(ROOT, args.output.resolve(), args.binary_dir, args.source_only)


if __name__ == "__main__":
    main()
