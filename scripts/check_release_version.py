#!/usr/bin/env python3
"""Validate and resolve the release identity used by CI workflows."""

import argparse
import datetime as dt
import re
import subprocess
import sys
import tomllib
from pathlib import Path


CALVER_TAG = re.compile(r"^v(\d{4})\.([1-9]\d?)\.([1-9]\d?)$")


class ReleaseVersionError(ValueError):
    pass


def git(repo: Path, *args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=repo,
        check=False,
        capture_output=True,
        text=True,
    )
    if check and result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ReleaseVersionError(f"git {' '.join(args)} failed: {detail}")
    return result.stdout.strip()


def resolve_release_tag(repo: Path, commit: str) -> str:
    tags = [
        tag
        for tag in git(repo, "tag", "--points-at", commit).splitlines()
        if tag.startswith("v")
    ]
    if len(tags) != 1:
        rendered = ", ".join(tags) if tags else "none"
        raise ReleaseVersionError(
            f"expected exactly one v* tag at {commit}, found {len(tags)}: {rendered}"
        )
    return tags[0]


def read_cargo_version(repo: Path) -> str:
    with (repo / "Cargo.toml").open("rb") as manifest:
        data = tomllib.load(manifest)
    try:
        return data["package"]["version"]
    except (KeyError, TypeError) as error:
        raise ReleaseVersionError(
            "Cargo.toml must define [package].version"
        ) from error


def tag_object_type(repo: Path, tag: str, remote: str | None) -> str:
    """Report whether a release tag is annotated ("tag") or lightweight ("commit").

    The local ref is not authoritative: actions/checkout fetches
    `+<sha>:refs/tags/<tag>` when the tag already exists, which overwrites the
    ref with the commit object and makes every annotated tag look lightweight.
    When a remote is available, ask it instead -- it advertises an extra
    `<tag>^{}` peeled line only for annotated tags.
    """
    if remote:
        # A glob refspec is required: git suppresses the peeled `^{}` line when
        # the refspec names a ref exactly, and that line is the only signal
        # distinguishing an annotated tag from a lightweight one.
        listing = git(repo, "ls-remote", remote, f"refs/tags/{tag}*")
        refs = {
            line.split("\t", 1)[1].strip()
            for line in listing.splitlines()
            if "\t" in line
        }
        if f"refs/tags/{tag}" not in refs:
            raise ReleaseVersionError(
                f"release tag {tag!r} does not exist on remote {remote!r}"
            )
        return "tag" if f"refs/tags/{tag}^{{}}" in refs else "commit"
    return git(repo, "cat-file", "-t", f"refs/tags/{tag}")


def validate_release(
    repo: Path, tag: str, commit: str, main_ref: str, remote: str | None = None
) -> str:
    match = CALVER_TAG.fullmatch(tag)
    if not match:
        raise ReleaseVersionError(
            f"release tag {tag!r} must use the exact vYYYY.M.D format"
        )

    year, month, day = (int(value) for value in match.groups())
    try:
        dt.date(year, month, day)
    except ValueError as error:
        raise ReleaseVersionError(f"release tag {tag!r} is not a valid calendar date") from error

    object_type = tag_object_type(repo, tag, remote)
    if object_type != "tag":
        raise ReleaseVersionError(f"release tag {tag!r} must be an annotated tag")

    tag_commit = git(repo, "rev-parse", f"refs/tags/{tag}^{{commit}}")
    release_commit = git(repo, "rev-parse", commit)
    if tag_commit != release_commit:
        raise ReleaseVersionError(
            f"release tag {tag!r} must point to release commit {release_commit}, "
            f"but points to {tag_commit}"
        )

    expected = tag[1:]
    actual = read_cargo_version(repo)
    if actual != expected:
        raise ReleaseVersionError(
            f"Cargo.toml version {actual!r} does not match release tag {tag!r}; "
            f"set [package].version to {expected!r}, update Cargo.lock, commit, and recreate the tag"
        )

    ancestry = subprocess.run(
        ["git", "merge-base", "--is-ancestor", release_commit, main_ref],
        cwd=repo,
        check=False,
        capture_output=True,
        text=True,
    )
    if ancestry.returncode != 0:
        raise ReleaseVersionError(
            f"release commit {release_commit} must be reachable from {main_ref}"
        )

    return actual


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    result.add_argument("--repo", type=Path, default=Path.cwd())
    subparsers = result.add_subparsers(dest="command", required=True)

    resolve = subparsers.add_parser("resolve")
    resolve.add_argument("--commit", required=True)

    validate = subparsers.add_parser("validate")
    validate.add_argument("--tag", required=True)
    validate.add_argument("--commit", required=True)
    validate.add_argument("--main-ref", default="origin/main")
    validate.add_argument(
        "--remote",
        default=None,
        help="Remote to consult for the authoritative tag object type.",
    )
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "resolve":
            print(resolve_release_tag(args.repo, args.commit))
        else:
            version = validate_release(
                args.repo, args.tag, args.commit, args.main_ref, args.remote
            )
            print(f"release identity valid: {args.tag} (Cargo {version})")
    except ReleaseVersionError as error:
        print(f"::error::{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
