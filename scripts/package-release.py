#!/usr/bin/env python3
"""Build one amt-cli release archive and its SHA-256 manifest."""

import argparse
import hashlib
import shutil
import tarfile
import tempfile
import zipfile
from pathlib import Path


PLATFORMS = {
    "windows-amd64": ("amt-cli.exe", ".zip"),
    "windows-arm64": ("amt-cli.exe", ".zip"),
    "linux-amd64": ("amt-cli", ".tar.gz"),
    "macos-arm64": ("amt-cli", ".tar.gz"),
}

OPTIONAL_GUIDES = (
    "windows-amd64.md",
    "windows-arm64.md",
    "linux-amd64.md",
    "macos-arm64.md",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", choices=PLATFORMS, required=True)
    parser.add_argument("--output-dir", type=Path, default=Path("dist"))
    return parser.parse_args()


def copy_payload(root: Path, binary: Path, package_dir: Path, binary_name: str) -> None:
    if not binary.is_file():
        raise FileNotFoundError(f"Release binary not found: {binary}")

    (package_dir / "docs").mkdir(parents=True)
    shutil.copy2(binary, package_dir / binary_name)

    for name in ("LICENSE", "README.md", "config.example.yaml"):
        source = root / name
        if not source.is_file():
            raise FileNotFoundError(f"Required release file not found: {source}")
        shutil.copy2(source, package_dir / name)

    getting_started = root / "docs" / "getting-started.md"
    if not getting_started.is_file():
        raise FileNotFoundError(f"Required release file not found: {getting_started}")
    shutil.copy2(getting_started, package_dir / "docs" / getting_started.name)

    for name in OPTIONAL_GUIDES:
        source = root / "docs" / name
        if source.is_file():
            shutil.copy2(source, package_dir / "docs" / name)


def make_zip(package_dir: Path, output: Path) -> None:
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(package_dir.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(package_dir).as_posix())

    with zipfile.ZipFile(output) as archive:
        damaged = archive.testzip()
        if damaged is not None:
            raise RuntimeError(f"ZIP integrity check failed at {damaged}")


def make_tarball(package_dir: Path, output: Path) -> None:
    with tarfile.open(output, "w:gz", compresslevel=9) as archive:
        for path in sorted(package_dir.rglob("*")):
            arcname = f"{package_dir.name}/{path.relative_to(package_dir).as_posix()}"
            archive.add(path, arcname=arcname, recursive=False)

    with tarfile.open(output, "r:gz") as archive:
        members = archive.getmembers()
        if not members or not any(member.name.endswith("/amt-cli") for member in members):
            raise RuntimeError("Tarball integrity check failed: amt-cli is missing")


def main() -> None:
    args = parse_args()
    root = args.root.resolve()
    binary = args.binary.resolve()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    binary_name, suffix = PLATFORMS[args.platform]
    basename = f"amt-cli-{args.version}-{args.platform}"
    output = output_dir / f"{basename}{suffix}"

    with tempfile.TemporaryDirectory(prefix="amt-release-") as temporary:
        package_dir = Path(temporary) / basename
        copy_payload(root, binary, package_dir, binary_name)
        if suffix == ".zip":
            make_zip(package_dir, output)
        else:
            make_tarball(package_dir, output)

    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    manifest = output.with_name(f"{output.name}.sha256")
    manifest.write_bytes(f"{digest}  {output.name}\n".encode("utf-8"))
    print(f"Created {output}")
    print(f"SHA256 {digest}")


if __name__ == "__main__":
    main()
