"""Release checks must fail before uploads and support partial-release retries."""

import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import call, patch
import urllib.error

import publish


def crate(name="eerie", version="0.6.1", **files):
    entries = {"Cargo.toml.orig": b"original manifest", "src/lib.rs": b"pub fn test() {}"}
    entries.update(files)
    result = io.BytesIO()
    with tarfile.open(fileobj=result, mode="w:gz") as archive:
        for path, content in entries.items():
            member = tarfile.TarInfo(f"{name}-{version}/{path}")
            member.size = len(content)
            archive.addfile(member, io.BytesIO(content))
    return result.getvalue()


class ReleaseTests(unittest.TestCase):
    def test_tag_and_native_dependency_versions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "eerie-sys").mkdir()
            wrapper = '[package]\nversion = "0.6.1"\n[dependencies.eerie-sys]\nversion = "0.4.1"\n'
            (root / "Cargo.toml").write_text(wrapper)
            (root / "eerie-sys/Cargo.toml").write_text('[package]\nversion = "0.4.1"\n')
            self.assertEqual(publish.release_packages("v0.6.1", root), [
                ("eerie-sys", "0.4.1"), ("eerie", "0.6.1"),
            ])
            with self.assertRaisesRegex(ValueError, "must match"):
                publish.release_packages("v0.6.2", root)
            (root / "Cargo.toml").write_text(wrapper.replace('"0.4.1"', '"0.4.0"'))
            with self.assertRaisesRegex(ValueError, "minimum eerie-sys"):
                publish.release_packages("v0.6.1", root)

    def test_only_not_found_means_an_unpublished_version(self):
        for status in [403, 404, 429, 500]:
            with self.subTest(status=status), patch.object(publish, "fetch", side_effect=
                    urllib.error.HTTPError("https://crates.io", status, "error", {}, None)):
                if status == 404:
                    self.assertIsNone(publish.registry_version("eerie", "0.6.1"))
                else:
                    with self.assertRaises(urllib.error.HTTPError):
                        publish.registry_version("eerie", "0.6.1")

    def test_existing_version_requires_matching_sources(self):
        published = crate(**{"Cargo.lock": b"old lock", ".cargo_vcs_info.json": b"old revision"})
        record = {"yanked": False, "checksum": hashlib.sha256(published).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "eerie.crate"
            with patch.object(publish, "registry_version", return_value=record), \
                    patch.object(publish, "fetch", return_value=published):
                archive.write_bytes(crate(**{
                    "Cargo.lock": b"new lock", ".cargo_vcs_info.json": b"new revision",
                }))
                self.assertFalse(publish.needs_publish("eerie", "0.6.1", archive))
                for change in [
                    {"src/lib.rs": b"changed code"},
                    {"Cargo.toml.orig": b"changed dependency"},
                    {"new-file.rs": b"new code"},
                ]:
                    with self.subTest(change=change):
                        archive.write_bytes(crate(**change))
                        with self.assertRaisesRegex(ValueError, "bump its version"):
                            publish.needs_publish("eerie", "0.6.1", archive)

    def test_yanked_or_corrupt_existing_archive_is_rejected(self):
        for record, message in [
            ({"yanked": True}, "yanked"),
            ({"yanked": False, "checksum": "invalid"}, "checksum mismatch"),
        ]:
            with self.subTest(record=record), \
                    patch.object(publish, "registry_version", return_value=record), \
                    patch.object(publish, "fetch", return_value=crate()):
                with self.assertRaisesRegex(ValueError, message):
                    publish.needs_publish("eerie", "0.6.1", Path("unused.crate"))

    def test_archive_without_original_manifest_is_rejected(self):
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode="w:gz"):
            pass
        with self.assertRaisesRegex(ValueError, "original manifest"):
            publish.archive_sources(data.getvalue(), "eerie", "0.6.1")

    def run_release(self, pending, dry_run=False, expected_error=None, package_error=None):
        packages = [("eerie-sys", "0.4.1"), ("eerie", "0.6.1")]
        with patch.object(publish, "release_packages", return_value=packages), \
                patch.object(publish.subprocess, "check_output", return_value=json.dumps({
                    "target_directory": "/tmp/eerie-release-test",
                })), \
                patch.object(publish.subprocess, "run", side_effect=package_error) as run, \
                patch.object(publish, "needs_publish", side_effect=pending), \
                patch.dict(publish.os.environ, {"CARGO_REGISTRY_TOKEN": "test-only-placeholder"}):
            if expected_error:
                with self.assertRaisesRegex(Exception, expected_error):
                    publish.publish("v0.6.1", dry_run=dry_run)
            else:
                publish.publish("v0.6.1", dry_run=dry_run)
            return run.call_args_list

    def test_publish_order_and_retry_after_native_publish(self):
        for pending, names in [([True, True], ["eerie-sys", "eerie"]), ([False, True], ["eerie"])]:
            with self.subTest(pending=pending):
                commands = self.run_release(pending)
                self.assertEqual(commands[0], call(
                    ["cargo", "package", "--locked", "--workspace"], cwd=publish.ROOT, check=True,
                ))
                self.assertEqual([command.args[0][-1] for command in commands[1:]], names)

    def test_all_versions_checked_before_any_upload(self):
        commands = self.run_release(
            [True, ValueError("version collision")], expected_error="version collision",
        )
        self.assertEqual(len(commands), 1)  # Verification only; no upload.
        # A package verification failure also prevents the upload phase.
        commands = self.run_release(
            [], expected_error="returned non-zero",
            package_error=publish.subprocess.CalledProcessError(1, "cargo package"),
        )
        self.assertEqual(len(commands), 1)
        with patch.object(publish, "release_packages", return_value=[]), \
                patch.dict(publish.os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, "TOKEN is missing"):
                publish.publish("v0.6.1")

    def test_dry_run_never_uploads(self):
        self.assertEqual(len(self.run_release([True, True], dry_run=True)), 1)


if __name__ == "__main__":
    unittest.main()
