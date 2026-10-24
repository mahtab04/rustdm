"""Prepare and register RustDM native messaging for the current user only."""
import argparse
import json
import os
from pathlib import Path
import shutil
import sys
# Portable Python builds can omit the script directory from sys.path.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_extension import chromium_id, FIREFOX_ID

NAME = "com.rustdm.bridge"
WINDOWS_KEYS = {"chrome": "Google\\Chrome", "chromium": "Chromium", "edge": "Microsoft\\Edge", "firefox": "Mozilla"}


def default_state():
    if os.environ.get("LOCALAPPDATA") or os.environ.get("XDG_DATA_HOME"):
        return Path(os.environ.get("LOCALAPPDATA") or os.environ["XDG_DATA_HOME"]) / "RustDM"
    return Path.home() / ".local/share/RustDM"


def plan(browser, host, downloads, state, install, windows=None):
    windows = sys.platform == "win32" if windows is None else windows
    executable = install / ("rustdm-browser-host.exe" if windows else "rustdm-browser-host")
    caller = FIREFOX_ID if browser == "firefox" else f"chrome-extension://{chromium_id()}/"
    manifest = {"name": NAME, "description": "RustDM browser queue bridge", "path": str(executable), "type": "stdio"}
    manifest["allowed_extensions" if browser == "firefox" else "allowed_origins"] = [caller]
    if windows:
        registration = "Software\\" + WINDOWS_KEYS[browser] + "\\NativeMessagingHosts\\" + NAME
        manifest_path = install / f"{NAME}.json"
    else:
        if sys.platform != "linux":
            raise ValueError("The setup script currently supports Windows and Linux")
        locations = {"chrome": ".config/google-chrome/NativeMessagingHosts", "chromium": ".config/chromium/NativeMessagingHosts", "edge": ".config/microsoft-edge/NativeMessagingHosts", "firefox": ".mozilla/native-messaging-hosts"}
        manifest_path = Path.home() / locations[browser] / f"{NAME}.json"
        registration = str(manifest_path)
    return {
        "source": str(host), "executable": str(executable), "manifest_path": str(manifest_path),
        "registration": registration, "manifest": manifest,
        "config": {"version": 1, "state_dir": str(state), "download_directory": str(downloads), "allowed_callers": [caller]},
    }


def register(details):
    manifest_path = Path(details["manifest_path"])
    if manifest_path.exists() and json.loads(manifest_path.read_text(encoding="utf-8")) != details["manifest"]:
        raise ValueError("A different host manifest already exists; review it before replacing it")
    if sys.platform == "win32":
        import winreg
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, details["registration"]) as key:
                previous = winreg.QueryValue(key, None)
                if os.path.normcase(previous) != os.path.normcase(str(manifest_path)):
                    raise ValueError("A different host registration already exists; it was preserved")
        except FileNotFoundError:
            pass
    destination = Path(details["executable"])
    destination.parent.mkdir(parents=True, exist_ok=True)
    Path(details["config"]["download_directory"]).mkdir(parents=True, exist_ok=True)
    if Path(details["source"]).resolve() != destination.resolve():
        shutil.copy2(details["source"], destination)
    (destination.parent / "browser-host.json").write_text(json.dumps(details["config"], indent=2) + "\n", encoding="utf-8")
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(details["manifest"], indent=2) + "\n", encoding="utf-8")
    if sys.platform == "win32":
        import winreg
        with winreg.CreateKey(winreg.HKEY_CURRENT_USER, details["registration"]) as key:
            winreg.SetValueEx(key, "", 0, winreg.REG_SZ, str(manifest_path))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", choices=WINDOWS_KEYS, required=True)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--download-dir", type=Path, required=True)
    parser.add_argument("--state-dir", type=Path, default=default_state())
    parser.add_argument("--install-dir", type=Path)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--dry-run", action="store_true")
    action.add_argument("--register", action="store_true")
    args = parser.parse_args()
    host = args.host.resolve(strict=True)
    state = args.state_dir.resolve()
    install = (args.install_dir or default_state() / "browser-host" / args.browser).resolve()
    details = plan(args.browser, host, args.download_dir.resolve(), state, install)
    print(json.dumps(details, indent=2))
    if args.register:
        register(details)
        print("Registered for the current user. Load the extension and queue a reviewed file link.")


if __name__ == "__main__":
    main()
