import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.check_release_version import (
    ReleaseVersionError,
    resolve_release_tag,
    validate_release,
)


class ReleaseVersionTest(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp_dir.name)
        self.git("init")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Release Test")
        self.write_manifest("2026.8.7")
        self.git("add", "Cargo.toml")
        self.git("commit", "-m", "initial")
        self.git("branch", "-M", "main")
        self.commit = self.git("rev-parse", "HEAD")

    def tearDown(self):
        self.temp_dir.cleanup()

    def git(self, *args):
        return subprocess.run(
            ["git", *args],
            cwd=self.repo,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def write_manifest(self, version):
        (self.repo / "Cargo.toml").write_text(
            f'[package]\nname = "test"\nversion = "{version}"\nedition = "2024"\n',
            encoding="utf-8",
        )

    def annotated_tag(self, name="v2026.8.7"):
        self.git("tag", "-a", name, "-m", f"Release {name}")
        return name

    def assert_validation_error(self, expected, tag):
        with self.assertRaisesRegex(ReleaseVersionError, expected):
            validate_release(self.repo, tag, self.commit, "main")

    def test_accepts_matching_annotated_calver_on_main(self):
        tag = self.annotated_tag()

        result = validate_release(self.repo, tag, self.commit, "main")

        self.assertEqual(result, "2026.8.7")

    def test_rejects_cargo_version_mismatch(self):
        tag = self.annotated_tag()
        self.write_manifest("2026.8.6")

        self.assert_validation_error("Cargo.toml version", tag)

    def test_rejects_invalid_calendar_date(self):
        tag = self.annotated_tag("v2026.2.30")

        self.assert_validation_error("valid calendar date", tag)

    def test_rejects_revision_suffix(self):
        tag = self.annotated_tag("v2026.8.7-1")

        self.assert_validation_error("vYYYY.M.D", tag)

    def test_rejects_leading_zero_month_or_day(self):
        tag = self.annotated_tag("v2026.08.07")

        self.assert_validation_error("vYYYY.M.D", tag)

    def test_rejects_lightweight_tag(self):
        self.git("tag", "v2026.8.7")

        self.assert_validation_error("annotated", "v2026.8.7")

    def test_rejects_commit_not_reachable_from_main(self):
        self.git("checkout", "--orphan", "other")
        self.write_manifest("2026.8.8")
        self.git("add", "Cargo.toml")
        self.git("commit", "-m", "other")
        commit = self.git("rev-parse", "HEAD")
        self.git("tag", "-a", "v2026.8.8", "-m", "Release v2026.8.8")

        with self.assertRaisesRegex(ReleaseVersionError, "reachable from main"):
            validate_release(self.repo, "v2026.8.8", commit, "main")

    def test_rejects_tag_that_points_to_a_different_commit(self):
        tag = self.annotated_tag()
        self.write_manifest("2026.8.7")
        self.git("commit", "--allow-empty", "-m", "later")
        later_commit = self.git("rev-parse", "HEAD")

        with self.assertRaisesRegex(ReleaseVersionError, "must point to release commit"):
            validate_release(self.repo, tag, later_commit, "main")

    def test_resolves_unique_release_tag_at_commit(self):
        self.annotated_tag()

        self.assertEqual(resolve_release_tag(self.repo, self.commit), "v2026.8.7")

    def test_rejects_zero_release_tags_at_commit(self):
        with self.assertRaisesRegex(ReleaseVersionError, r"exactly one v\* tag"):
            resolve_release_tag(self.repo, self.commit)

    def test_rejects_multiple_release_tags_at_commit(self):
        self.annotated_tag()
        self.annotated_tag("v2026.8.8")

        with self.assertRaisesRegex(ReleaseVersionError, r"exactly one v\* tag"):
            resolve_release_tag(self.repo, self.commit)


if __name__ == "__main__":
    unittest.main()
