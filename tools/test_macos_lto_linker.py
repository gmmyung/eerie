import unittest

from macos_lto_linker import linker_args


class MacOSLtoLinkerTests(unittest.TestCase):
    def test_preserves_objects_paths_and_unrelated_options(self):
        arguments = ["file Δ with spaces.o", "-Wl,-rpath,/path,with,commas", "-lSystem"]
        self.assertEqual(linker_args(arguments), arguments)

    def test_translates_grouped_optimization_and_cpu(self):
        self.assertEqual(
            linker_args(["-Wl,-plugin-opt=O3,-plugin-opt=mcpu=apple-m1,-dead_strip"]),
            ["-Wl,--lto-O3,--lto-CGO3,-mcpu,apple-m1,-dead_strip"],
        )

    def test_preserves_each_optimization_level(self):
        for level in range(4):
            with self.subTest(level=level):
                self.assertEqual(
                    linker_args([f"-Wl,-plugin-opt=O{level}"]),
                    [f"-Wl,--lto-O{level},--lto-CGO{level}"],
                )

    def test_leaves_unrecognized_plugin_options_for_the_linker_to_reject(self):
        arguments = ["-Wl,-plugin-opt=unknown=value"]
        self.assertEqual(linker_args(arguments), arguments)
