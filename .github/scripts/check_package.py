"""Verify and smoke-test the native archive produced by package_release.py."""

import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import tarfile
import tempfile
import zipfile


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    windows = platform.system() == "Windows"
    suffix = ".zip" if windows else ".tar.gz"
    archives = list(args.directory.glob(f"rustdm-*{suffix}"))
    if not archives:
        parser.error("Expected at least one native archive in the package directory")
    for archive in sorted(archives):
        check_archive(parser, archive, windows)


def check_archive(parser, archive, windows):
    expected, filename = archive.with_name(archive.name + ".sha256").read_text().split()
    if filename != archive.name or digest(archive) != expected:
        parser.error("Archive checksum verification failed")
    with tempfile.TemporaryDirectory(prefix="rustdm-package-") as temporary:
        destination = Path(temporary)
        if windows:
            with zipfile.ZipFile(archive) as bundle:
                bundle.extractall(destination)
        else:
            with tarfile.open(archive) as bundle:
                bundle.extractall(destination, filter="data")
        metadata_files = list(destination.glob("*/BUILD.json"))
        if len(metadata_files) != 1:
            parser.error("Expected one build metadata file")
        metadata = json.loads(metadata_files[0].read_text())
        application = metadata.get("application", "cli")
        if application not in {"cli", "desktop", "browser"}:
            parser.error("Unknown application in package metadata")
        executable = {"cli": "rustdm", "desktop": "rustdm-ui", "browser": "rustdm-browser-host"}[application]
        filename = executable + (".exe" if windows else "")
        if metadata.get("executable", filename) != filename:
            parser.error("Unexpected executable filename in package metadata")
        binary = metadata_files[0].parent / filename
        if digest(binary) != metadata["binary_sha256"]:
            parser.error("Packaged executable checksum verification failed")
        version = subprocess.check_output([str(binary), "--version"], text=True).strip()
        if version != f"{executable} {metadata['version']}":
            parser.error("Packaged executable version mismatch")
        subprocess.run([str(binary), "--help"], check=True, stdout=subprocess.DEVNULL)
        if application == "browser":
            for browser in ("chromium", "firefox"):
                manifest = binary.parent / "browser-extensions" / browser / "manifest.json"
                if json.loads(manifest.read_text())["manifest_version"] != 3:
                    parser.error("Missing or invalid browser extension manifest")
            if not (binary.parent / "extensions/browser/install_host.py").is_file():
                parser.error("Missing browser setup script")
        if application == "cli":
            subprocess.run([str(binary), "download", "--help"], check=True, stdout=subprocess.DEVNULL)
    print(f"PASS: {archive.name}: archive hash, executable hash, version, and help")


if __name__ == "__main__":
    main()
