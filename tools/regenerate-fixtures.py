#!/usr/bin/env python3
"""Regenerate runtime fixtures with the compiler matching the IREE submodule."""

import argparse
import importlib.metadata
import pathlib
import sys

import iree.compiler as compiler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="check without writing")
    args = parser.parse_args()
    if importlib.metadata.version("iree-base-compiler") != "3.13.0":
        parser.error("install iree-base-compiler==3.13.0 before regenerating fixtures")
    root = pathlib.Path(__file__).resolve().parents[1]
    fixtures = [
        ("mul.mlir", "mul_vmvx.vmfb", "vmvx", []),
        ("mul.mlir", "mul_vmvx_inline.vmfb", "vmvx-inline", ["--iree-execution-model=inline-static"]),
        ("wide.mlir", "wide.vmfb", "vmvx", ["--compile-from=vm"]),
    ]
    stale = False
    for source, filename, backend, extra in fixtures:
        blob = compiler.compile_str(
            (root / "tests" / source).read_text(),
            target_backends=[backend],
            extra_args=["--iree-vm-bytecode-module-strip-source-map=true", *extra],
        )
        destination = root / "tests" / filename
        if args.check:
            if not destination.exists() or destination.read_bytes() != blob:
                print(f"stale fixture: {filename}", file=sys.stderr)
                stale = True
        else:
            destination.write_bytes(blob)
            print(f"wrote {filename} ({len(blob)} bytes)")
    return int(stale)


if __name__ == "__main__":
    sys.exit(main())
