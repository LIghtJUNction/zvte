#!/usr/bin/env python3
"""Check the published crate's dependency and file boundaries using Cargo."""

import json
from pathlib import Path
import subprocess
import tomllib


def main():
    root = Path(__file__).resolve().parents[1]
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    package = manifest["package"]
    assert package["name"] == "zvte", "unexpected package name"
    assert package["rust-version"] == "1.74", "update MSRV CI with the manifest"
    assert not (root / "build.rs").exists(), "build.rs is forbidden"
    assert package.get("build", False) is False, "build scripts are forbidden"
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version=1", "--no-deps", "--locked"],
        cwd=root,
    ))
    ours = [p for p in metadata["packages"] if p["name"] == "zvte"]
    assert len(ours) == 1, "ambiguous zvte package"
    assert not ours[0]["dependencies"], "all dependency categories must be empty"
    assert metadata["workspace_members"] == [ours[0]["id"]], "comparison must be isolated"
    files = subprocess.check_output(
        ["cargo", "package", "--list", "--allow-dirty", "--locked"],
        cwd=root, text=True,
    ).splitlines()
    forbidden = ("comparison/", ".github/", "scripts/", "benchmark-results/")
    assert not any(p.startswith(forbidden) for p in files), "development files leaked into crate"
    assert "build.rs" not in files, "build.rs leaked into crate"
    assert "src/lib.rs" in files, "missing library source"
    print(f"zvte {package['version']}: zero dependencies; {len(files)} packaged files; isolated comparison")


if __name__ == "__main__":
    main()
