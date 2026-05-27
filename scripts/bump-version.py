#!/usr/bin/env python3
"""
Bump the version number across all packaging files.

Usage:
    python scripts/bump-version.py             # show current versions
    python scripts/bump-version.py 0.3.2        # set all to 0.3.2
    python scripts/bump-version.py --check      # exit 1 if files differ
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

FILES = [
    ("Rust crate", REPO / "Cargo.toml", r'^version\s*=\s*"(.+?)"', r'version = "{}"'),
    (
        "Python bindings crate",
        REPO / "python" / "Cargo.toml",
        r'^version\s*=\s*"(.+?)"',
        r'version = "{}"',
    ),
    (
        "Python package",
        REPO / "python" / "pyproject.toml",
        r'^version\s*=\s*"(.+?)"',
        r'version = "{}"',
    ),
    (
        "Node bindings crate",
        REPO / "node" / "Cargo.toml",
        r'^version\s*=\s*"(.+?)"',
        r'version = "{}"',
    ),
    (
        "npm package",
        REPO / "node" / "package.json",
        r'^\s*"version":\s*"(.+?)",?',
        r'  "version": "{}",',
    ),
]


def read_versions():
    versions = {}
    for label, path, pattern, _fmt in FILES:
        text = path.read_text()
        m = re.search(pattern, text, re.MULTILINE)
        if not m:
            print(f"ERROR: could not find version in {path}")
            sys.exit(1)
        versions[label] = m.group(1)
    return versions


def write_versions(new_version):
    for label, path, pattern, fmt in FILES:
        text = path.read_text()
        repl = fmt.format(new_version)
        new_text, count = re.subn(pattern, repl, text, count=1, flags=re.MULTILINE)
        if count == 0:
            print(f"ERROR: could not replace version in {path}")
            sys.exit(1)
        path.write_text(new_text)
        print(f"  {label:30s}  → {new_version}")


def main():
    versions = read_versions()

    # Show current
    print("Current versions:")
    for label, ver in versions.items():
        print(f"  {label:30s}  {ver}")

    all_match = len(set(versions.values())) == 1
    common = next(iter(versions.values()))

    if not all_match:
        print()
        print("WARNING: versions are NOT in sync — fix manually first.")

    if "--check" in sys.argv:
        sys.exit(0 if all_match else 1)

    if len(sys.argv) < 2:
        # Just display
        return

    new_version = sys.argv[1]
    if not re.match(r"^\d+\.\d+\.\d+", new_version):
        print(f"ERROR: '{new_version}' does not look like a semver version")
        sys.exit(1)

    if all_match and new_version == common:
        print(f"\nAlready at {new_version} — nothing to do.")
        return

    print(f"\nBumping to {new_version}:")
    write_versions(new_version)
    print(
        "\nDon't forget to update the tag example in README.md "
        f"({common} → {new_version}) if needed."
    )


if __name__ == "__main__":
    main()
