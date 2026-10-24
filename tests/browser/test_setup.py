import json
from pathlib import Path
import sys
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "extensions/browser"))
import build_extension
import install_host


class BrowserSetupTests(unittest.TestCase):
    def test_both_manifests_use_explicit_permissions_and_their_supported_backgrounds(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = build_extension.build(Path(temporary))
            chrome = json.loads((output / "chromium/manifest.json").read_text())
            firefox = json.loads((output / "firefox/manifest.json").read_text())
            self.assertEqual(chrome["permissions"], ["contextMenus", "nativeMessaging", "storage"])
            self.assertNotIn("host_permissions", chrome)
            self.assertNotIn("content_scripts", chrome)
            self.assertEqual(chrome["background"]["service_worker"], "background.js")
            self.assertEqual(firefox["background"]["scripts"], ["background.js"])
            self.assertNotIn("service_worker", firefox["background"])
            self.assertEqual(firefox["browser_specific_settings"]["gecko"]["id"], "rustdm@mahtab04")
            self.assertEqual(build_extension.chromium_id(), "ipnpmnioeedgmnhlfameghfgaemfnmjm")
            workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
            self.assertEqual(chrome["version"], workspace["workspace"]["package"]["version"])

    def test_dry_run_cli_works_outside_the_script_directory_without_writing_installation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            host = root / "test-host"
            host.write_bytes(b"test fixture")
            result = subprocess.run([sys.executable, str(ROOT / "extensions/browser/install_host.py"),
                "--browser", "edge", "--host", str(host), "--download-dir", str(root / "downloads"),
                "--install-dir", str(root / "install"), "--dry-run"], cwd=root, capture_output=True, text=True, check=True)
            details = json.loads(result.stdout)
            self.assertEqual(details["source"], str(host))
            self.assertFalse((root / "install").exists())
            self.assertFalse((root / "downloads").exists())

    def test_windows_setup_is_current_user_scoped_and_restricts_the_caller(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = install_host.plan("edge", root / "source.exe", root / "downloads", root / "state", root / "install", windows=True)
            self.assertEqual(result["registration"], "Software\\Microsoft\\Edge\\NativeMessagingHosts\\com.rustdm.bridge")
            self.assertEqual(result["manifest"]["allowed_origins"], ["chrome-extension://ipnpmnioeedgmnhlfameghfgaemfnmjm/"])
            self.assertEqual(result["config"]["allowed_callers"], result["manifest"]["allowed_origins"])
            self.assertEqual(result["config"]["state_dir"], str(root / "state"))
            self.assertFalse((root / "install").exists(), "Planning must have no side effects")

    def test_firefox_manifest_uses_addon_ids_instead_of_chromium_origins(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = install_host.plan("firefox", root / "source.exe", root / "downloads", root / "state", root / "install", windows=True)
            self.assertEqual(result["manifest"]["allowed_extensions"], ["rustdm@mahtab04"])
            self.assertNotIn("allowed_origins", result["manifest"])

    def test_an_unrelated_existing_manifest_is_never_replaced(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = install_host.plan("edge", root / "source.exe", root / "downloads", root / "state", root / "install", windows=True)
            manifest = Path(result["manifest_path"])
            manifest.parent.mkdir()
            manifest.write_text('{"name":"different-host"}')
            with self.assertRaisesRegex(ValueError, "different host manifest"):
                install_host.register(result)
            self.assertEqual(json.loads(manifest.read_text()), {"name": "different-host"})


if __name__ == "__main__":
    unittest.main()
