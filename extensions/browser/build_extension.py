"""Build dependency-free Chromium and Firefox development extensions."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import shutil

HERE = Path(__file__).resolve().parent
FIREFOX_ID = "rustdm@mahtab04"


def chromium_key():
    return (HERE / "chromium-key.txt").read_text(encoding="utf-8").strip()


def chromium_id():
    digest = hashlib.sha256(base64.b64decode(chromium_key())).digest()[:16]
    return "".join(chr(97 + nibble) for byte in digest for nibble in (byte >> 4, byte & 15))


def build(output):
    source = HERE / "extension"
    base = json.loads((source / "manifest.base.json").read_text(encoding="utf-8"))
    for browser in ("chromium", "firefox"):
        destination = output / browser
        destination.mkdir(parents=True, exist_ok=True)
        for path in source.iterdir():
            if path.name != "manifest.base.json" and path.is_file():
                shutil.copyfile(path, destination / path.name)
        manifest = dict(base)
        if browser == "chromium":
            manifest["key"] = chromium_key()
            manifest["minimum_chrome_version"] = "121"
            manifest["background"] = {"service_worker": "background.js", "type": "module"}
        else:
            manifest["background"] = {"scripts": ["background.js"], "type": "module"}
            manifest["browser_specific_settings"] = {"gecko": {
                "id": FIREFOX_ID, "strict_min_version": "140.0",
                "data_collection_permissions": {"required": ["none"]},
            }}
        (destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=HERE.parents[1] / "dist" / "browser-extensions")
    args = parser.parse_args()
    print(build(args.output.resolve()))
    print(f"Chromium extension ID: {chromium_id()}")
    print(f"Firefox extension ID: {FIREFOX_ID}")


if __name__ == "__main__":
    main()
