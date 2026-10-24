#!/usr/bin/env python3
"""Expose the installed IREE compiler to Cargo and the GitHub Actions loader."""

import os
from pathlib import Path
import sys

import iree.compiler


library_path = Path(iree.compiler.__path__[0]) / "_mlir_libs"
environment = {"LIB_IREE_COMPILER": str(library_path)}
if sys.platform == "win32":
    environment["LIBCLANG_PATH"] = str(Path(os.environ.get("LLVM_PATH", r"C:\Program Files\LLVM")) / "bin")
    environment["CARGO_TARGET_DIR"] = r"C:\t"
    with open(os.environ["GITHUB_PATH"], "a", encoding="utf-8") as output:
        output.write(f"{library_path}\n")
else:
    rpath = f"-C link-arg=-Wl,-rpath,{library_path}"
    environment.update(RUSTFLAGS=rpath, RUSTDOCFLAGS=rpath)
with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as output:
    for key, value in environment.items():
        output.write(f"{key}={value}\n")
