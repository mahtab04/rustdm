"""Package a native release build using Python 3.12+ and standard-library tools."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parents[2]
TARGETS = ("x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu")


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--app", choices=["cli", "desktop"], default="cli")
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--tag", default=os.environ.get("RELEASE_TAG") or None)
    args = parser.parse_args()

    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))[
        "workspace"
    ]["package"]["version"]
    if args.tag is not None and args.tag != f"v{version}":
        parser.error(f"Tag {args.tag!r} does not match workspace version v{version}")
    rustc = command("rustc", "-vV")
    if f"host: {args.target}" not in rustc.splitlines():
        parser.error("Package target must match the native Rust host")
    binary = args.binary.resolve(strict=True)
    executable = "rustdm-ui" if args.app == "desktop" else "rustdm"
    expected_filename = executable + (".exe" if args.target.endswith("windows-msvc") else "")
    if binary.name != expected_filename:
        parser.error(f"Expected executable filename {expected_filename}")
    if command(str(binary), "--version") != f"{executable} {version}":
        parser.error("Executable version does not match the workspace version")
    dirty = bool(command("git", "status", "--porcelain"))
    if args.tag and dirty:
        parser.error("Tagged release packages require a clean checkout")
    notes = ROOT / "docs" / "releases" / f"v{version}.md"
    if not notes.is_file():
        parser.error(f"Missing release notes: {notes.relative_to(ROOT)}")

    prefix = "rustdm-desktop" if args.app == "desktop" else "rustdm"
    name = f"{prefix}-v{version}-{args.target}"
    suffix = ".zip" if binary.suffix == ".exe" else ".tar.gz"
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output / f"{name}{suffix}"
    checksum = archive.with_name(archive.name + ".sha256")
    if archive.exists() or checksum.exists():
        parser.error("Package already exists; choose a new output directory")
    files = [(binary, binary.name)] + [
        (ROOT / path, path)
        for path in [
            "README.md", "CHANGELOG.md", "docs/QUEUE.md",
            "docs/ARCHITECTURE.md", "docs/RELEASE.md",
            f"docs/releases/v{version}.md",
        ]
    ]
    if args.app == "desktop":
        files.append((ROOT / "apps/ui/README.md", "apps/ui/README.md"))
    if (ROOT / "LICENSE").is_file():
        files.append((ROOT / "LICENSE", "LICENSE"))
    metadata = (json.dumps({
        "application": args.app,
        "executable": binary.name,
        "version": version,
        "target": args.target,
        "commit": command("git", "rev-parse", "HEAD"),
        "source_dirty": dirty,
        "rustc": rustc,
        "binary_sha256": digest(binary),
    }, indent=2) + "\n").encode()
    if suffix == ".zip":
        with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as bundle:
            for path, relative in files:
                bundle.write(path, f"{name}/{relative}")
            bundle.writestr(f"{name}/BUILD.json", metadata)
    else:
        with tarfile.open(archive, "x:gz") as bundle:
            for path, relative in files:
                info = bundle.gettarinfo(str(path), f"{name}/{relative}")
                info.mode = 0o755 if path == binary else 0o644
                with path.open("rb") as source:
                    bundle.addfile(info, source)
            info = tarfile.TarInfo(f"{name}/BUILD.json")
            info.size = len(metadata)
            info.mode = 0o644
            bundle.addfile(info, io.BytesIO(metadata))
    checksum.write_text(f"{digest(archive)}  {archive.name}\n", encoding="utf-8")
    print(archive.resolve())
    print(checksum.resolve())


if __name__ == "__main__":
    main()
