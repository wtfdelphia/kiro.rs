import subprocess
import tempfile
import tomllib
import unittest
import json
import os
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class ReleaseGovernanceFilesTest(unittest.TestCase):
    def read(self, relative):
        return (ROOT / relative).read_text(encoding="utf-8")

    def test_reusable_version_gate_is_read_only_and_validates_identity(self):
        workflow = self.read(".github/workflows/version-gate.yaml")

        self.assertIn("workflow_call:", workflow)
        self.assertIn("contents: read", workflow)
        self.assertIn("enforce:", workflow)
        self.assertIn("release_tag:", workflow)
        self.assertIn("scripts/check_release_version.py validate", workflow)

    def test_build_workflow_runs_version_gate_before_artifacts(self):
        workflow = self.read(".github/workflows/build.yaml")

        self.assertIn("version-gate:", workflow)
        self.assertIn("uses: ./.github/workflows/version-gate.yaml", workflow)
        self.assertIn("      - version-gate", workflow)

    def test_docker_workflow_separates_dry_run_and_release_identity(self):
        workflow = self.read(".github/workflows/docker-build.yaml")

        self.assertIn("release_tag: ${{ steps.check.outputs.release_tag }}", workflow)
        self.assertIn("scripts/check_release_version.py resolve", workflow)
        self.assertIn("version-gate:", workflow)
        self.assertIn("needs.pre-check.outputs.release_tag", workflow)
        self.assertIn("build-args: |", workflow)
        self.assertIn("VERSION=${{ steps.version.outputs.version }}", workflow)

    def test_dev_workflow_stays_outside_version_gate_and_keeps_commit_metadata(self):
        workflow = self.read(".github/workflows/build-dev-release.yaml")

        self.assertNotIn("version-gate", workflow)
        self.assertIn("- Commit: ${{ github.sha }}", workflow)
        self.assertIn("- Short SHA: ${{ needs.prepare.outputs.short_sha }}", workflow)

    def test_dockerfile_declares_oci_version_label(self):
        dockerfile = self.read("Dockerfile")

        self.assertIn("ARG VERSION=unknown", dockerfile)
        self.assertIn("LABEL org.opencontainers.image.version=$VERSION", dockerfile)

    def test_manifest_declares_msrv(self):
        with (ROOT / "Cargo.toml").open("rb") as manifest:
            package = tomllib.load(manifest)["package"]

        self.assertEqual(package["rust-version"], "1.97.1")

    def test_startup_reports_version_before_config_error(self):
        subprocess.run(
            ["cargo", "build", "--quiet"],
            cwd=ROOT,
            check=True,
            timeout=180,
        )
        metadata = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        target_dir = Path(json.loads(metadata.stdout)["target_directory"])
        binary = target_dir / "debug" / "kiro-rs.exe"
        with tempfile.TemporaryDirectory() as temp_dir:
            invalid = Path(temp_dir) / "invalid-config.json"
            invalid.write_text("{", encoding="utf-8")
            env = os.environ.copy()
            env["RUST_LOG"] = "info"
            result = subprocess.run(
                [
                    str(binary),
                    "--config",
                    str(invalid),
                ],
                cwd=ROOT,
                check=False,
                capture_output=True,
                text=True,
                env=env,
                timeout=10,
            )

        log_lines = [line for line in result.stdout.splitlines() if line.strip()]
        self.assertNotEqual(result.returncode, 0)
        self.assertGreaterEqual(len(log_lines), 2)
        self.assertIn("kiro-rs v", log_lines[0])
        self.assertIn("加载配置失败", log_lines[1])


if __name__ == "__main__":
    unittest.main()
