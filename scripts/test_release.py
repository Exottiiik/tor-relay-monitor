"""Release-boundary tests: workspace data must never enter source archives."""
from contextlib import redirect_stdout
import io
from pathlib import Path
import tempfile
import unittest
import zipfile

from package_release import ROOT_FILES, SOURCE_DIRS, package, source_files
from verify_release import safe_relative


class ReleaseTests(unittest.TestCase):
    def test_only_reviewed_source_roots_are_packaged(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ROOT_FILES:
                (root / name).write_text('[package]\nversion = "1.2.3"\n' if name == "Cargo.toml" else "fixture\n")
            for name in SOURCE_DIRS:
                (root / name).mkdir()
                (root / name / "example.txt").write_text("reviewed\n")
            for directory in (".tools", "target", "dist", ".git", ".aws"):
                (root / directory).mkdir()
                (root / directory / "private.txt").write_text("must stay local\n")
            (root / ".env").write_text("must stay local\n")
            cache = root / "scripts/__pycache__"
            cache.mkdir()
            (cache / "cached.pyc").write_bytes(b"not source")
            with redirect_stdout(io.StringIO()):
                assets = package(root, root / "dist", source_only=True)
            with zipfile.ZipFile(assets[0]) as archive:
                names = archive.namelist()
            self.assertIn("tor-relay-monitor-1.2.3/.github/example.txt", names)
            self.assertFalse(any(part in name for name in names for part in ("/.tools/", "/target/", "/dist/", "/.git/", "/.aws/", "/.env", "/__pycache__/")))
            self.assertEqual(len((root / "dist/SHA256SUMS").read_text().splitlines()), 1)

    def test_archive_paths_cannot_escape_the_release_root(self):
        for name in ("/absolute", "prefix/../../escape", "other/file", "prefix/dir\\file"):
            with self.assertRaises(ValueError):
                safe_relative(name, "prefix")
        self.assertEqual(str(safe_relative("prefix/src/main.rs", "prefix")), "src/main.rs")

    def test_missing_required_file_is_an_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(ValueError):
                source_files(Path(temporary))


if __name__ == "__main__":
    unittest.main()
