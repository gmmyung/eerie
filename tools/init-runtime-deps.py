#!/usr/bin/env python3
"""Initialize IREE's pinned runtime dependencies without compiler submodules."""

import pathlib
import subprocess


root = pathlib.Path(__file__).resolve().parents[1] / "eerie-sys/iree"
modules = (root / "build_tools/scripts/git/runtime_submodules.txt").read_text().split()
subprocess.run([
    "git", "-C", str(root), "submodule", "update", "--init", "--depth", "1", "--", *modules,
], check=True)
