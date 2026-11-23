#!/usr/bin/env python3
"""Verify release checksums/content and execute only this locally built release."""
import argparse
import hashlib
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import tempfile
import zipfile

from package_release import ROOT, ROOT_FILES, SOURCE_DIRS, version


def safe_relative(name, prefix):
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or "\\" in name or not name.startswith(prefix + "/"):
        raise ValueError(f"Unsafe archive path: {name}")
    return path.relative_to(prefix)


def verify(directory, source_only=False):
    release_version = version(ROOT)
    prefix = f"tor-relay-monitor-{release_version}"
    expected = {f"{prefix}-source.zip"}
    if not source_only:
        expected.add(f"{prefix}-linux-x86_64.tar.gz")
    rows = (directory / "SHA256SUMS").read_text(encoding="utf-8").splitlines()
    seen = set()
    for row in rows:
        digest, name = row.split("  ", 1)
        if name not in expected or name in seen:
            raise ValueError("Unexpected or duplicate checksum entry")
        with (directory / name).open("rb") as contents:
            if hashlib.file_digest(contents, "sha256").hexdigest() != digest:
                raise ValueError(f"Checksum mismatch: {name}")
        seen.add(name)
    if seen != expected:
        raise ValueError("Missing release checksum")
    with zipfile.ZipFile(directory / f"{prefix}-source.zip") as archive:
        if archive.testzip() is not None:
            raise ValueError("Source ZIP CRC failure")
        names = set()
        for entry in archive.infolist():
            relative = safe_relative(entry.filename, prefix)
            if entry.filename in names:
                raise ValueError("Duplicate source member")
            names.add(entry.filename)
            if relative.parts[0] not in SOURCE_DIRS and relative.as_posix() not in ROOT_FILES:
                raise ValueError(f"Unexpected source member: {relative}")
            if "__pycache__" in relative.parts or entry.is_dir() or entry.external_attr >> 28 == 0xA:
                raise ValueError(f"Unexpected source file type: {relative}")
        for required in ("Cargo.lock", "SECURITY.md", ".github/workflows/ci.yml", "scripts/package_release.py", "src/bin/trm.rs"):
            if f"{prefix}/{required}" not in names:
                raise ValueError(f"Missing source member: {required}")
    if source_only:
        print("Source checksum, CRC, required files and path allowlist verified")
        return
    with tempfile.TemporaryDirectory(prefix="trm-installed-") as install_directory:
        installed = Path(install_directory) / "trm"
        with tempfile.TemporaryDirectory(prefix="trm-extracted-") as extraction:
            with tarfile.open(directory / f"{prefix}-linux-x86_64.tar.gz") as archive:
                members = archive.getmembers()
                seen = set()
                for member in members:
                    relative = safe_relative(member.name, prefix)
                    if member.name in seen:
                        raise ValueError("Duplicate binary archive member")
                    seen.add(member.name)
                    if member.issym():
                        if relative.as_posix() != "tor-relay-monitor" or member.linkname != "trm":
                            raise ValueError("Unexpected archive link")
                    elif not member.isfile():
                        raise ValueError("Unexpected archive file type")
                if archive.getmember(f"{prefix}/trm").mode != 0o755:
                    raise ValueError("Release executable has incorrect permissions")
                if not archive.getmember(f"{prefix}/tor-relay-monitor").issym():
                    raise ValueError("Missing compatibility alias")
                # Paths/types/links were checked before extraction. Python 3.11 is supported.
                subprocess.run(["tar", "-xzf", str(directory / f"{prefix}-linux-x86_64.tar.gz"), "-C", extraction], check=True)
            extracted = Path(extraction) / prefix
            for name in ("trm", "tor-relay-monitor"):
                result = subprocess.run([str(extracted / name), "--version"], capture_output=True, text=True, check=True)
                if result.stdout.strip() != f"{name} {release_version}":
                    raise ValueError("Unexpected packaged binary version")
            shutil.copy2(extracted / "trm", installed)
        # Installation remains usable after removal of the extracted directory.
        result = subprocess.run([str(installed), "--help"], capture_output=True, text=True, check=True)
        if "Usage: trm" not in result.stdout:
            raise ValueError("Installed binary failed its help check")
    print("Checksums, source allowlist, archive paths/modes, aliases and independent installation verified")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=ROOT / "dist/release")
    parser.add_argument("--source-only", action="store_true")
    args = parser.parse_args()
    verify(args.directory.resolve(), args.source_only)
