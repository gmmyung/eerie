#!/usr/bin/env python3
"""Publish a version tag, verifying both archives before uploading either crate."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tomllib
import urllib.error
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
USER_AGENT = "eerie-release (https://github.com/gmmyung/eerie)"
# Cargo generates these files. Compare the original manifest and all source
# files, allowing retries across Cargo versions and dependency lock resolution.
GENERATED_FILES = {"Cargo.toml", "Cargo.lock", ".cargo_vcs_info.json"}


def release_packages(tag, root=ROOT):
    wrapper = tomllib.loads((root / "Cargo.toml").read_text())
    native = tomllib.loads((root / "eerie-sys/Cargo.toml").read_text())
    version = wrapper["package"]["version"]
    if tag != f"v{version}":
        raise ValueError(f"tag {tag!r} must match the eerie version: v{version}")
    native_version = native["package"]["version"]
    if wrapper["dependencies"]["eerie-sys"]["version"] != native_version:
        raise ValueError("eerie's minimum eerie-sys dependency must match its workspace version")
    return [("eerie-sys", native_version), ("eerie", version)]


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def registry_version(name, version):
    try:
        return json.loads(fetch(f"https://crates.io/api/v1/crates/{name}/{version}"))["version"]
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def archive_sources(data, name, version):
    prefix = f"{name}-{version}/"
    sources = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            if member.isdir():
                continue
            if not member.isfile() or not member.name.startswith(prefix):
                raise ValueError(f"unexpected archive entry: {member.name}")
            path = member.name[len(prefix):]
            if path not in GENERATED_FILES:
                with archive.extractfile(member) as source:
                    sources[path] = hashlib.file_digest(source, "sha256").hexdigest()
    if "Cargo.toml.orig" not in sources:
        raise ValueError("crate archive is missing its original manifest")
    return sources


def needs_publish(name, version, local_archive):
    existing = registry_version(name, version)
    if existing is None:
        return True
    if existing["yanked"]:
        raise ValueError(f"{name} {version} is yanked; choose a new version")
    published = fetch(f"https://static.crates.io/crates/{name}/{name}-{version}.crate")
    if hashlib.sha256(published).hexdigest() != existing["checksum"]:
        raise ValueError(f"checksum mismatch for published {name} {version}")
    local_sources = archive_sources(local_archive.read_bytes(), name, version)
    published_sources = archive_sources(published, name, version)
    if local_sources != published_sources:
        raise ValueError(f"{name} {version} is already published with different sources; bump its version")
    print(f"Already published: {name} {version} (matching sources)", flush=True)
    return False


def publish(tag, dry_run=False):
    packages = release_packages(tag)
    if not dry_run and not os.environ.get("CARGO_REGISTRY_TOKEN"):
        raise ValueError("CARGO_REGISTRY_TOKEN is missing")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT,
    ))
    package_dir = Path(metadata["target_directory"]) / "package"
    # Cargo stages workspace dependencies locally, allowing verification before
    # the new eerie-sys version is available on crates.io.
    subprocess.run(["cargo", "package", "--locked", "--workspace"], cwd=ROOT, check=True)
    # Complete all source/version checks before the first irreversible upload.
    pending = [
        (name, version) for name, version in packages
        if needs_publish(name, version, package_dir / f"{name}-{version}.crate")
    ]
    for name, version in pending:
        print(f"{'Would publish' if dry_run else 'Publishing'}: {name} {version}", flush=True)
        if not dry_run:
            # Cargo waits for the version to appear in the registry index before
            # returning, so eerie can resolve the newly published dependency.
            subprocess.run([
                "cargo", "publish", "--locked", "--registry", "crates-io", "-p", name,
            ], cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag", help="release tag, matching v<eerie version>")
    parser.add_argument("--check-tag", action="store_true", help="validate manifests only")
    parser.add_argument("--dry-run", action="store_true", help="verify packages without uploading")
    args = parser.parse_args()
    try:
        if args.check_tag:
            for name, version in release_packages(args.tag):
                print(f"Release: {name} {version}")
        else:
            publish(args.tag, args.dry_run)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release failed: {error}\n")


if __name__ == "__main__":
    main()
