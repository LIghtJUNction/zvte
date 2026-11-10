#!/usr/bin/env python3
"""Version preparation and checksum-verified, retryable crates.io publication."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import tomllib
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


def bump_version(version, bump):
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise ValueError("release version must be an unprefixed stable major.minor.patch")
    major, minor, patch = map(int, version.split("."))
    if bump == "major":
        major, minor, patch = major + 1, 0, 0
    elif bump == "minor":
        minor, patch = minor + 1, 0
    elif bump == "patch":
        patch += 1
    elif bump != "none":
        raise ValueError("bump must be none, patch, minor, or major")
    return f"{major}.{minor}.{patch}"


def replace_package_version(text, old, new, *, lock=False):
    """Replace only zvte's package version, leaving dependencies and metadata intact."""
    document = tomllib.loads(text)
    packages = document["package"] if lock else [document["package"]]
    ours = [p for p in packages if p["name"] == "zvte"]
    if len(ours) != 1 or ours[0]["version"] != old:
        raise ValueError("expected exactly one zvte package with the current version")
    section = r"\[\[package\]\]" if lock else r"\[package\]"
    sections = list(re.finditer(rf"(?m)^{section}[^\n]*$", text))
    for start in sections:
        tail = text[start.end():]
        end = re.search(r"(?m)^\[", tail)
        length = end.start() if end else len(tail)
        block = tail[:length]
        parsed = tomllib.loads("[package]\n" + block)["package"]
        if parsed.get("name") != "zvte":
            continue
        updated, count = re.subn(
            rf'(?m)^(version\s*=\s*)"{re.escape(old)}"',
            lambda m: f'{m[1]}"{new}"', block,
        )
        if count != 1:
            raise ValueError("version must be a single quoted TOML string")
        return text[:start.end()] + updated + tail[length:]
    raise ValueError("zvte package section was not found")


def read_version(root):
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    if manifest["package"]["name"] != "zvte":
        raise ValueError("release script only publishes zvte")
    return bump_version(manifest["package"]["version"], "none")


def prepare(root, bump):
    old = read_version(root)
    new = bump_version(old, bump)
    if new != old:
        changes = []
        for path, lock in [(root / "Cargo.toml", False), (root / "Cargo.lock", True),
                           (root / "comparison/Cargo.lock", True)]:
            if path.exists():
                changes.append((path, replace_package_version(path.read_text(), old, new, lock=lock)))
        for path, text in changes:
            path.write_text(text)
    return old, new


def registry_checksum(version):
    request = Request(
        f"https://crates.io/api/v1/crates/zvte/{version}",
        headers={"User-Agent": "zvte-release (https://github.com/LIghtJUNction/zvte)"},
    )
    try:
        with urlopen(request, timeout=30) as response:
            return json.load(response)["version"]["checksum"]
    except HTTPError as error:
        if error.code == 404:
            return None
        raise


def wait_for_checksum(version, expected, *, attempts=8, delay=2):
    """Allow registry API/index propagation without accepting an unverified package."""
    for attempt in range(attempts):
        try:
            actual = registry_checksum(version)
        except HTTPError as error:
            if error.code != 429 and error.code < 500:
                raise
            actual = None
        except (URLError, TimeoutError):
            actual = None
        if actual is not None:
            if actual != expected:
                raise ValueError("published archive checksum differs from the verified dry-run archive")
            return
        if attempt + 1 < attempts:
            time.sleep(delay)
    raise ValueError(
        "crates.io has not exposed the published archive after bounded retries; "
        "dispatch Release with bump=none to verify and resume"
    )


def package_archive(root, version):
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version=1", "--locked"], cwd=root,
    ))
    archive = Path(metadata["target_directory"]) / "package" / f"zvte-{version}.crate"
    if not archive.is_file():
        raise ValueError("run cargo package --locked after the publish dry-run to finalize the archive")
    return archive


def publish(root):
    version = read_version(root)
    archive = package_archive(root, version)
    checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
    existing = registry_checksum(version)
    if existing is not None:
        if existing != checksum:
            raise ValueError("crates.io already contains a DIFFERENT archive for this version")
        print(f"zvte {version} is already published with the identical archive")
        return
    if not os.environ.get("CARGO_REGISTRY_TOKEN"):
        raise ValueError("the CARGO_REGISTRY_TOKEN GitHub secret is missing")
    subprocess.run(["cargo", "publish", "--locked"], cwd=root, check=True)
    # Cargo waits for the index, but the HTTP API can expose the version later.
    wait_for_checksum(version, checksum)
    print(f"Published zvte {version}; crates.io archive matches the verified package")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    prepare_command = subcommands.add_parser("prepare")
    prepare_command.add_argument("--bump", choices=["none", "patch", "minor", "major"], required=True)
    subcommands.add_parser("version")
    subcommands.add_parser("package-path")
    subcommands.add_parser("publish")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if args.command == "prepare":
        old, version = prepare(root, args.bump)
        outputs = f"previous={old}\nversion={version}\ntag=v{version}\n"
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.write(outputs)
        print(outputs, end="")
    elif args.command == "version":
        print(read_version(root))
    elif args.command == "package-path":
        archive = package_archive(root, read_version(root))
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.write(f"path={archive}\n")
        print(archive)
    else:
        publish(root)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, HTTPError, OSError, subprocess.CalledProcessError) as error:
        print(f"release failed: {error}", file=sys.stderr)
        sys.exit(1)
