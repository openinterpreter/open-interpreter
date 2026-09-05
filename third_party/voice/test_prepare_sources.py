"""Exercise source identity and extraction boundaries with synthetic archives."""

import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import prepare_sources as preparation


class PreparationTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.output = self.root / "prepared sources"

    def source(self, entries):
        archive_path = self.root / "fixture.tar"
        with tarfile.open(archive_path, "w") as archive:
            for name, kind, payload in entries:
                member = tarfile.TarInfo(name)
                member.type = kind
                if kind == tarfile.SYMTYPE:
                    member.linkname = payload
                    archive.addfile(member)
                else:
                    member.size = len(payload)
                    archive.addfile(member, io.BytesIO(payload))
        return {
            "name": "fixture",
            "version": "1",
            "role": "native-library",
            "archive": archive_path.name,
            "root": "fixture-1",
            "url": "https://example.invalid/fixture.tar",
            "sha256": hashlib.sha256(archive_path.read_bytes()).hexdigest(),
            "provenance": "Synthetic test input",
        }

    def prepare(self, source):
        manifest = json.dumps({"schema_version": 1, "sources": [source]}).encode()
        preparation.prepare_sources(self.root, self.output, manifest)
        return manifest

    def test_preserves_files_licenses_when_symlinks_are_unavailable(self):
        source = self.source(
            [
                ("fixture-1/LICENSES/license.txt", tarfile.REGTYPE, b"License text\n"),
                ("fixture-1/COPYING", tarfile.SYMTYPE, "LICENSES/license.txt"),
            ]
        )
        with patch("tarfile.os.symlink", side_effect=OSError("No symlink privilege")):
            manifest = self.prepare(source)
        files = {
            p.relative_to(self.output / source["root"]).as_posix(): p.read_bytes()
            for p in (self.output / source["root"]).rglob("*")
            if p.is_file()
        }
        self.assertEqual(
            files,
            {
                "LICENSES/license.txt": b"License text\n",
                "COPYING": b"License text\n",
            },
        )
        self.assertFalse(any(p.is_symlink() for p in self.output.rglob("*")))
        self.assertEqual((self.output / "sources.json").read_bytes(), manifest)
        self.assertEqual(
            json.loads((self.output / "prepared.json").read_text()),
            {
                "schema_version": 1,
                "manifest_sha256": hashlib.sha256(manifest).hexdigest(),
                "sources": {
                    "fixture": {"root": "fixture-1", "sha256": source["sha256"]}
                },
            },
        )

    def test_rejects_changed_archive_before_extracting(self):
        source = self.source([("fixture-1/file", tarfile.REGTYPE, b"original")])
        (self.root / source["archive"]).write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            self.prepare(source)
        self.assertFalse(self.output.exists())

    def test_filter_failure_cleans_incomplete_output(self):
        source = self.source(
            [
                ("fixture-1/file", tarfile.REGTYPE, b"safe"),
                ("fixture-1/link", tarfile.SYMTYPE, "../../outside"),
            ]
        )
        with self.assertRaises(tarfile.FilterError):
            self.prepare(source)
        self.assertFalse(self.output.exists())

    def test_materialized_links_cannot_bypass_expansion_limit(self):
        source = self.source(
            [
                ("fixture-1/file", tarfile.REGTYPE, b"123456"),
                ("fixture-1/link", tarfile.SYMTYPE, "file"),
            ]
        )
        with patch.object(preparation, "MAX_SOURCE_BYTES", 10):
            with self.assertRaisesRegex(ValueError, "Expanded source exceeds limits"):
                self.prepare(source)
        self.assertFalse(self.output.exists())

    def test_preserves_existing_output(self):
        source = self.source([("fixture-1/file", tarfile.REGTYPE, b"data")])
        self.output.mkdir()
        (self.output / "keep").write_text("unchanged")
        with self.assertRaises(FileExistsError):
            self.prepare(source)
        self.assertEqual(
            {p.name: p.read_text() for p in self.output.iterdir()},
            {"keep": "unchanged"},
        )
