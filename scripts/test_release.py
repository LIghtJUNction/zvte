import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("release", Path(__file__).with_name("release.py"))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def test_bumps(self):
        for bump, expected in [("none", "1.2.3"), ("patch", "1.2.4"),
                               ("minor", "1.3.0"), ("major", "2.0.0")]:
            self.assertEqual(release.bump_version("1.2.3", bump), expected)

    def test_invalid_versions_fail_closed(self):
        for version in ["v1.2.3", "01.2.3", "1.2", "1.2.3-rc.1", "1.2.3+build", "-1.2.3", "1.2.3\n"]:
            with self.assertRaises(ValueError):
                release.bump_version(version, "patch")
        with self.assertRaises(ValueError):
            release.bump_version("1.2.3", "invalid")

    def test_manifest_change_is_scoped(self):
        source = '[package]\nname = "zvte"\nversion = "0.1.0" # retained\n\n[other]\nversion = "0.1.0"\n'
        result = release.replace_package_version(source, "0.1.0", "0.1.1")
        self.assertIn('version = "0.1.1" # retained', result)
        self.assertIn('[other]\nversion = "0.1.0"', result)

    def test_lock_change_is_scoped(self):
        source = ('version = 3\n\n[[package]]\nname = "vte"\nversion = "0.1.0"\n'
                  '\n[[package]]\nname = "zvte"\nversion = "0.1.0"\n')
        result = release.replace_package_version(source, "0.1.0", "0.2.0", lock=True)
        self.assertIn('name = "vte"\nversion = "0.1.0"', result)
        self.assertIn('name = "zvte"\nversion = "0.2.0"', result)

    def test_wrong_package_and_version_rejected(self):
        for source in ['[package]\nname = "other"\nversion = "0.1.0"\n',
                       '[package]\nname = "zvte"\nversion = "0.2.0"\n']:
            with self.assertRaises(ValueError):
                release.replace_package_version(source, "0.1.0", "0.1.1")

    def test_prepare_validates_all_files_before_writing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = '[package]\nname = "zvte"\nversion = "0.1.0"\n'
            (root / "Cargo.toml").write_text(manifest)
            (root / "Cargo.lock").write_text('[[package]]\nname = "wrong"\nversion = "0.1.0"\n')
            with self.assertRaises(ValueError):
                release.prepare(root, "patch")
            self.assertEqual((root / "Cargo.toml").read_text(), manifest)

    def test_publish_rejects_different_existing_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[package]\nname = "zvte"\nversion = "0.1.0"\n')
            (root / "target/package").mkdir(parents=True)
            (root / "target/package/zvte-0.1.0.crate").write_bytes(b"archive")
            metadata = json.dumps({"target_directory": str(root / "target")}).encode()
            with patch.object(release, "registry_checksum", return_value="different"):
                with patch.object(release.subprocess, "run") as run:
                    with patch.object(release.subprocess, "check_output", return_value=metadata):
                        with self.assertRaises(ValueError):
                            release.publish(root)
                    run.assert_not_called()

    def test_identical_archive_retry_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[package]\nname = "zvte"\nversion = "0.1.0"\n')
            (root / "target/package").mkdir(parents=True)
            (root / "target/package/zvte-0.1.0.crate").write_bytes(b"archive")
            checksum = release.hashlib.sha256(b"archive").hexdigest()
            metadata = json.dumps({"target_directory": str(root / "target")}).encode()
            with patch.object(release, "registry_checksum", return_value=checksum):
                with patch.object(release.subprocess, "run") as run:
                    with patch.object(release.subprocess, "check_output", return_value=metadata):
                        release.publish(root)
                    run.assert_not_called()

    def test_archive_uses_cargo_metadata_target_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "custom-cargo-output"
            (target / "package").mkdir(parents=True)
            expected = target / "package/zvte-0.1.0.crate"
            expected.write_bytes(b"finalized archive")
            metadata = json.dumps({"target_directory": str(target)}).encode()
            with patch.object(release.subprocess, "check_output", return_value=metadata):
                self.assertEqual(release.package_archive(root, "0.1.0"), expected)

    def test_dry_run_temporary_archive_requires_finalization(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target"
            (target / "package/tmp-crate").mkdir(parents=True)
            (target / "package/tmp-crate/zvte-0.1.0.crate").write_bytes(b"dry-run archive")
            metadata = json.dumps({"target_directory": str(target)}).encode()
            with patch.object(release.subprocess, "check_output", return_value=metadata):
                with self.assertRaisesRegex(ValueError, "cargo package --locked"):
                    release.package_archive(root, "0.1.0")

    def test_registry_eventual_consistency_is_retried(self):
        with patch.object(release, "registry_checksum", side_effect=[None, None, "expected"]) as fetch:
            with patch.object(release.time, "sleep") as sleep:
                release.wait_for_checksum("0.1.0", "expected")
                self.assertEqual(fetch.call_count, 3)
                self.assertEqual(sleep.call_count, 2)

    def test_registry_transient_failure_is_retried(self):
        errors = [release.HTTPError("https://crates.io", 503, "busy", None, None),
                  release.HTTPError("https://crates.io", 429, "busy", None, None),
                  release.URLError("temporary network failure"), "expected"]
        with patch.object(release, "registry_checksum", side_effect=errors):
            with patch.object(release.time, "sleep") as sleep:
                release.wait_for_checksum("0.1.0", "expected")
                self.assertEqual(sleep.call_count, 3)

    def test_registry_verification_retries_are_bounded(self):
        with patch.object(release, "registry_checksum", return_value=None) as fetch:
            with patch.object(release.time, "sleep") as sleep:
                with self.assertRaisesRegex(ValueError, "bounded retries"):
                    release.wait_for_checksum("0.1.0", "expected", attempts=3)
                self.assertEqual(fetch.call_count, 3)
                self.assertEqual(sleep.call_count, 2)

    def test_registry_mismatch_fails_without_retrying(self):
        with patch.object(release, "registry_checksum", return_value="different"):
            with patch.object(release.time, "sleep") as sleep:
                with self.assertRaisesRegex(ValueError, "checksum differs"):
                    release.wait_for_checksum("0.1.0", "expected")
                sleep.assert_not_called()

    def test_registry_permission_denied_fails_without_retrying(self):
        error = release.HTTPError("https://crates.io", 403, "forbidden", None, None)
        with patch.object(release, "registry_checksum", side_effect=error):
            with patch.object(release.time, "sleep") as sleep:
                with self.assertRaises(release.HTTPError):
                    release.wait_for_checksum("0.1.0", "expected")
                sleep.assert_not_called()


if __name__ == "__main__":
    unittest.main()
