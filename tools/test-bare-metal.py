#!/usr/bin/env python3
"""Link and execute the no_std runtime on QEMU's Cortex-M4 board."""

import os
import pathlib
import re
import subprocess


def main():
    root = pathlib.Path(__file__).resolve().parents[1]
    environment = os.environ.copy()
    # Host-specific Rust flags do not apply to the freestanding executable.
    environment.pop("RUSTFLAGS", None)
    environment.pop("RUSTDOCFLAGS", None)
    environment.pop("CARGO_ENCODED_RUSTFLAGS", None)
    for kind, tool in [("CC", "gcc"), ("CXX", "g++"), ("AR", "ar")]:
        name = f"{kind}_thumbv7em_none_eabihf"
        environment.setdefault(name, f"arm-none-eabi-{tool}")
    directory = root / "tests/bare-metal"
    link_map = directory / "target/eerie-bare-metal.map"
    subprocess.run([
        "cargo", "rustc", "--locked", "--target-dir", str(directory / "target"),
        "--", "-C", f"link-arg=-Map={link_map}",
    ], cwd=directory, env=environment, check=True)
    if re.search(r"\b(?:libc|libm|libnosys|libgcc|libatomic)\.a\b", link_map.read_text()):
        raise RuntimeError("bare-metal executable linked a forbidden C runtime archive")
    executable = directory / "target/thumbv7em-none-eabihf/debug/eerie-bare-metal-smoke"
    subprocess.run([
        "qemu-system-arm", "-M", "mps2-an386", "-cpu", "cortex-m4", "-nographic",
        "-semihosting-config", "enable=on,target=native", "-kernel", str(executable),
    ], check=True, timeout=60)
    print("Verified: no newlib/libc, libm, nosys, libgcc, or libatomic linkage")


if __name__ == "__main__":
    main()
