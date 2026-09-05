"""Exercise private package copies, target pairing, provenance, and failure cleanup."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from assemble_package import assemble


class AssembleTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="voice package ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.package = self.root / "app"
        (self.package / "bin").mkdir(parents=True)
        (self.package / "codex-resources").mkdir()
        (self.package / "codex-path").mkdir()
        self.commit = "a" * 40
        self.metadata = {
            "layoutVersion": 1,
            "version": f"0.0.0+{self.commit}",
            "target": "aarch64-unknown-linux-musl",
            "variant": "codex",
            "entrypoint": "bin/codex",
            "resourcesDir": "codex-resources",
            "pathDir": "codex-path",
        }
        (self.package / "codex-package.json").write_text(json.dumps(self.metadata))
        (self.package / "bin/codex").write_bytes(b"unchanged app")
        self.helper = self.root / "helper.exe"
        self.helper.write_bytes(b"private helper")
        self.helper.chmod(0o755)
        self.output = self.root / "installed copy"

    def test_copies_app_unchanged_and_records_distinct_linux_targets(self):
        assemble(
            self.package,
            self.helper,
            "aarch64-unknown-linux-gnu",
            self.commit,
            self.output,
        )
        self.assertEqual((self.output / "bin/codex").read_bytes(), b"unchanged app")
        self.assertEqual((self.package / "bin/codex").read_bytes(), b"unchanged app")
        self.assertFalse((self.package / "codex-resources/voice").exists())
        self.assertEqual(
            (self.output / "codex-package.json").read_bytes(),
            (self.package / "codex-package.json").read_bytes(),
        )
        self.assertEqual(
            json.loads(
                (self.output / "codex-resources/voice/manifest.json").read_text()
            ),
            {
                "schemaVersion": 1,
                "buildCommit": self.commit,
                "appTarget": self.metadata["target"],
                "voiceTarget": "aarch64-unknown-linux-gnu",
                "appVersion": self.metadata["version"],
                "sha256": {
                    "bin/codex": hashlib.sha256(b"unchanged app").hexdigest(),
                    "codex-resources/voice/bin/codex-voice-host": hashlib.sha256(
                        b"private helper"
                    ).hexdigest(),
                },
            },
        )

    def test_rejects_incompatible_targets_and_unstamped_or_mixed_builds(self):
        for target, commit in [
            ("aarch64-unknown-linux-musl", self.commit),
            ("x86_64-unknown-linux-gnu", self.commit),
            ("aarch64-unknown-linux-gnu", "dev"),
            ("aarch64-unknown-linux-gnu", "b" * 40),
        ]:
            with (
                self.subTest(target=target, commit=commit),
                self.assertRaises(ValueError),
            ):
                assemble(self.package, self.helper, target, commit, self.output)
            self.assertFalse(self.output.exists())

    def test_assembles_matching_gnu_linux_app_and_helper_targets(self):
        for architecture in ("aarch64", "x86_64"):
            target = f"{architecture}-unknown-linux-gnu"
            with self.subTest(target=target):
                self.metadata["target"] = target
                (self.package / "codex-package.json").write_text(
                    json.dumps(self.metadata)
                )
                output = self.root / target
                assemble(self.package, self.helper, target, self.commit, output)
                manifest = json.loads(
                    (output / "codex-resources/voice/manifest.json").read_text()
                )
                self.assertEqual(
                    manifest,
                    {
                        "schemaVersion": 1,
                        "buildCommit": self.commit,
                        "appTarget": target,
                        "voiceTarget": target,
                        "appVersion": self.metadata["version"],
                        "sha256": {
                            "bin/codex": hashlib.sha256(b"unchanged app").hexdigest(),
                            "codex-resources/voice/bin/codex-voice-host": hashlib.sha256(
                                b"private helper"
                            ).hexdigest(),
                        },
                    },
                )
                self.assertEqual((output / "bin/codex").read_bytes(), b"unchanged app")
                self.assertEqual(
                    (
                        output / "codex-resources/voice/bin/codex-voice-host"
                    ).read_bytes(),
                    self.helper.read_bytes(),
                )

    def test_never_replaces_existing_or_nested_outputs(self):
        for output in (self.package, self.package / "nested", self.helper):
            with self.subTest(output=output), self.assertRaises(ValueError):
                assemble(
                    self.package,
                    self.helper,
                    "aarch64-unknown-linux-gnu",
                    self.commit,
                    output,
                )
        self.assertEqual(self.helper.read_bytes(), b"private helper")
        self.assertFalse((self.package / "nested").exists())

    def test_cleans_only_its_new_copy_on_failure(self):
        with patch("assemble_package.shutil.copy2", side_effect=OSError("copy failed")):
            with self.assertRaises(OSError):
                assemble(
                    self.package,
                    self.helper,
                    "aarch64-unknown-linux-gnu",
                    self.commit,
                    self.output,
                )
        self.assertFalse(self.output.exists())
        self.assertEqual((self.package / "bin/codex").read_bytes(), b"unchanged app")


if __name__ == "__main__":
    unittest.main()
