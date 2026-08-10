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


class RemoteTagObjectTypeTest(ReleaseVersionTest):
    """Cover tag-type detection against a remote.

    actions/checkout fetches `+<sha>:refs/tags/<tag>` when the tag already
    exists, which replaces the local annotated tag ref with a commit object.
    The gate must consult the remote so a legitimate annotated tag is not
    misreported as lightweight.
    """

    def setUp(self):
        super().setUp()
        self.remote_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.remote_dir.cleanup)
        self.remote = Path(self.remote_dir.name)
        subprocess.run(
            ["git", "init", "--bare", str(self.remote)],
            check=True,
            capture_output=True,
            text=True,
        )
        self.git("remote", "add", "origin", str(self.remote))

    def push_all(self):
        self.git("push", "--quiet", "origin", "main", "--tags")

    def overwrite_local_tag_ref_like_checkout(self, tag):
        """Reproduce the actions/checkout ref rewrite that hides annotation."""
        self.git("update-ref", f"refs/tags/{tag}", self.commit)
        self.assertEqual(
            self.git("cat-file", "-t", f"refs/tags/{tag}"),
            "commit",
            "precondition: local tag ref must look lightweight",
        )

    def test_accepts_annotated_tag_after_checkout_rewrites_local_ref(self):
        tag = self.annotated_tag()
        self.push_all()
        self.overwrite_local_tag_ref_like_checkout(tag)

        result = validate_release(self.repo, tag, self.commit, "main", str(self.remote))

        self.assertEqual(result, "2026.8.7")

    def test_reports_cargo_mismatch_instead_of_tag_type_after_rewrite(self):
        tag = self.annotated_tag()
        self.push_all()
        self.overwrite_local_tag_ref_like_checkout(tag)
        self.write_manifest("2026.8.6")

        with self.assertRaisesRegex(ReleaseVersionError, "Cargo.toml version"):
            validate_release(self.repo, tag, self.commit, "main", str(self.remote))

    def test_rejects_lightweight_tag_on_remote(self):
        self.git("tag", "v2026.8.7")
        self.push_all()

        with self.assertRaisesRegex(ReleaseVersionError, "annotated"):
            validate_release(
                self.repo, "v2026.8.7", self.commit, "main", str(self.remote)
            )

    def test_rejects_tag_missing_on_remote(self):
        tag = self.annotated_tag()

        with self.assertRaisesRegex(ReleaseVersionError, "does not exist on remote"):
            validate_release(self.repo, tag, self.commit, "main", str(self.remote))

    def test_glob_refspec_does_not_confuse_prefix_sharing_tags(self):
        """`v2026.8.1*` also matches `v2026.8.10`; exact ref filtering must hold."""
        self.write_manifest("2026.8.1")
        self.git("commit", "--allow-empty", "-am", "version 2026.8.1")
        self.commit = self.git("rev-parse", "HEAD")
        self.git("tag", "-a", "v2026.8.1", "-m", "Release v2026.8.1")
        self.git("tag", "v2026.8.10")  # lightweight, shares the glob prefix
        self.push_all()
        self.overwrite_local_tag_ref_like_checkout("v2026.8.1")

        result = validate_release(
            self.repo, "v2026.8.1", self.commit, "main", str(self.remote)
        )

        self.assertEqual(result, "2026.8.1")

    def test_lightweight_tag_still_rejected_when_prefix_peer_is_annotated(self):
        self.write_manifest("2026.8.1")
        self.git("commit", "--allow-empty", "-am", "version 2026.8.1")
        self.commit = self.git("rev-parse", "HEAD")
        self.git("tag", "v2026.8.1")  # lightweight, the one under validation
        self.git("tag", "-a", "v2026.8.10", "-m", "Release v2026.8.10")
        self.push_all()

        with self.assertRaisesRegex(ReleaseVersionError, "annotated"):
            validate_release(
                self.repo, "v2026.8.1", self.commit, "main", str(self.remote)
            )


if __name__ == "__main__":
    unittest.main()
