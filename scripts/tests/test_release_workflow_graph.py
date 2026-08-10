import unittest
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"
ARTIFACT_JOBS = {
    "build.yaml": ("build", "release"),
    "docker-build.yaml": ("build", "manifest"),
}


def load(name):
    with (WORKFLOWS / name).open("rb") as handle:
        return yaml.safe_load(handle)


def needs_of(job):
    needs = job.get("needs", [])
    return [needs] if isinstance(needs, str) else list(needs)


def gate_reachable(jobs, job_name, gate, seen=None):
    """Whether job_name transitively depends on gate through the needs graph."""
    seen = seen or set()
    if job_name in seen:
        return False
    seen.add(job_name)
    for dependency in needs_of(jobs[job_name]):
        if dependency == gate or gate_reachable(jobs, dependency, gate, seen):
            return True
    return False


class ReleaseWorkflowGraphTest(unittest.TestCase):
    def test_stable_release_triggers_listen_to_main_only(self):
        for name in ARTIFACT_JOBS:
            with self.subTest(workflow=name):
                branches = load(name)[True]["push"]["branches"]

                self.assertIn("main", branches)
                self.assertNotIn("master", branches)

    def test_every_artifact_job_depends_on_both_gates(self):
        for name, artifact_jobs in ARTIFACT_JOBS.items():
            jobs = load(name)["jobs"]
            for job_name in artifact_jobs:
                for gate in ("version-gate", "warning-gate"):
                    with self.subTest(workflow=name, job=job_name, gate=gate):
                        self.assertTrue(gate_reachable(jobs, job_name, gate))

    def test_version_gate_does_not_wait_for_warning_gate(self):
        for name in ARTIFACT_JOBS:
            with self.subTest(workflow=name):
                version_gate = load(name)["jobs"]["version-gate"]

                self.assertNotIn("warning-gate", needs_of(version_gate))

    def test_version_gate_is_never_skipped_for_non_release_builds(self):
        # A skipped gate job propagates skipped to every downstream job, which
        # would silently drop branch and dry-run artifacts instead of gating.
        for name in ARTIFACT_JOBS:
            with self.subTest(workflow=name):
                self.assertNotIn("if", load(name)["jobs"]["version-gate"])

    def test_dev_rolling_workflow_has_no_version_gate(self):
        jobs = load("build-dev-release.yaml")["jobs"]

        self.assertNotIn("version-gate", jobs)
        for job in jobs.values():
            self.assertNotIn("version-gate", needs_of(job))


if __name__ == "__main__":
    unittest.main()
