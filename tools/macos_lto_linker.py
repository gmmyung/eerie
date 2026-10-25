#!/usr/bin/env python3
"""Translate rustc's GNU LTO plugin options for Clang and ld64.lld."""

import os
import re
import sys


def linker_args(arguments):
    result = []
    for argument in arguments:
        if not argument.startswith("-Wl,") or "-plugin-opt=" not in argument:
            result.append(argument)
            continue
        options = []
        for option in argument[4:].split(","):
            if match := re.fullmatch(r"-plugin-opt=O([0-3])", option):
                level = match[1]
                options.extend([f"--lto-O{level}", f"--lto-CGO{level}"])
            elif option.startswith("-plugin-opt=mcpu="):
                options.extend(["-mcpu", option.removeprefix("-plugin-opt=mcpu=")])
            else:
                options.append(option)
        result.append("-Wl," + ",".join(options))
    return result


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("usage: macos_lto_linker.py CLANG [LINKER_ARGUMENTS...]")
    compiler = sys.argv[1]
    os.execvp(compiler, [compiler, *linker_args(sys.argv[2:])])
