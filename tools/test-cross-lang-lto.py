#!/usr/bin/env python3
"""Prove that the explicit LTO feature removes a Rust-to-IREE C call."""

import json
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
SYMBOL = "eerie_cross_lang_lto_rank"
CALL_OR_JUMP = re.compile(r"\b(?:callq?|jmpq?|bl|blr|b|br)\s")


def probe(features):
    output = subprocess.check_output([
        "cargo", "test", "--locked", "--release", "--no-default-features",
        "--features", features, "--test", "lto_probe", "--no-run",
        "--message-format=json",
    ], cwd=ROOT, text=True)
    executables = []
    for line in output.splitlines():
        item = json.loads(line)
        if item.get("reason") == "compiler-message":
            print(item["message"]["rendered"], end="", file=sys.stderr)
        if (
            item.get("reason") == "compiler-artifact"
            and item["target"]["name"] == "lto_probe"
            and item.get("executable")
        ):
            executables.append(item["executable"])
    if len(executables) != 1:
        raise RuntimeError(f"expected one LTO probe executable, got {executables}")
    executable = executables[0]
    subprocess.run([executable], cwd=ROOT, check=True, timeout=30)
    assembly = subprocess.check_output([
        "llvm-objdump", "--disassemble", "--no-show-raw-insn",
        f"--disassemble-symbols={SYMBOL}", executable,
    ], text=True)
    if f"<{SYMBOL}>:" not in assembly:
        raise RuntimeError(f"probe symbol was not retained:\n{assembly}")
    return assembly


def main():
    # The Rust linker-plugin settings are identical in both builds. Only the
    # Cargo feature may enable native bitcode and cross-language optimization.
    control = probe("std,runtime")
    if "iree_hal_buffer_view_shape_rank" not in control:
        raise RuntimeError(f"control build did not retain the C call:\n{control}")
    optimized = probe("std,runtime,cross-lang-lto")
    if CALL_OR_JUMP.search(optimized):
        raise RuntimeError(f"Rust wrapper still calls another function:\n{optimized}")
    print(optimized)
    print("Verified: cross-lang-lto inlined IREE's C getter into Rust.")


if __name__ == "__main__":
    main()
