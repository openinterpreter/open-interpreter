#!/usr/bin/env python3

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from codex_package.layout import build_package_dir
from codex_package.layout import prepare_package_dir
from codex_package.layout import validate_package_dir
from codex_package.targets import PACKAGE_VARIANTS
from codex_package.targets import PackageInputs
from codex_package.targets import TARGET_SPECS


class PackageLayoutTest(unittest.TestCase):
    def test_open_interpreter_package_contains_interpreter_alias_and_code_mode_host(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "interpreter"),
                managed_codex_bin=None,
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=touch_executable(root / "bwrap"),
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )
            package_dir = root / "package"

            prepare_package_dir(package_dir, force=False)
            build_package_dir(
                package_dir,
                "1.2.3",
                PACKAGE_VARIANTS["open-interpreter"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                inputs,
            )
            validate_package_dir(
                package_dir,
                PACKAGE_VARIANTS["open-interpreter"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                include_zsh=False,
            )

            self.assertEqual(
                (package_dir / "bin" / "interpreter").read_text(),
                "interpreter",
            )
            self.assertEqual((package_dir / "bin" / "i").read_text(), "interpreter")
            self.assertEqual(
                (package_dir / "bin" / "codex-code-mode-host").read_text(),
                "codex-code-mode-host",
            )
            self.assertFalse((package_dir / "bin" / "codex").exists())
            metadata = (package_dir / "codex-package.json").read_text(encoding="utf-8")
            self.assertIn('"entrypoint": "bin/interpreter"', metadata)
            self.assertNotIn('"managedCodex"', metadata)

    def test_app_server_package_places_code_mode_host_beside_entrypoint(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "codex-app-server"),
                managed_codex_bin=None,
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=touch_executable(root / "bwrap"),
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )

            build_package_dir(
                package_dir,
                "1.2.3",
                PACKAGE_VARIANTS["codex-app-server"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                inputs,
            )
            validate_package_dir(
                package_dir,
                PACKAGE_VARIANTS["codex-app-server"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                include_zsh=False,
            )

            self.assertTrue((package_dir / "bin" / "codex-code-mode-host").is_file())


def touch_executable(path: Path) -> Path:
    path.write_text(path.name, encoding="utf-8")
    path.chmod(0o755)
    return path.resolve()


if __name__ == "__main__":
    unittest.main()
