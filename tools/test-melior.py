#!/usr/bin/env python3
"""Build and exercise Melior/IREE coexistence with a timeout for symbol-collision hangs."""

import json
import os
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "tests/melior/Cargo.toml"
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
    "--manifest-path", str(MANIFEST),
], cwd=ROOT))
subprocess.run([
    "cargo", "build", "--locked", "--manifest-path", str(MANIFEST),
], cwd=ROOT, check=True)
binary = Path(metadata["target_directory"]) / "debug" / (
    "eerie-melior-coexistence.exe" if os.name == "nt" else "eerie-melior-coexistence"
)
for order in ["iree-first", "melior-first"]:
    subprocess.run([str(binary), order], cwd=ROOT, check=True, timeout=30)
