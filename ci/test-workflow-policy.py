#!/usr/bin/env python3
"""Workflow trigger/wiring and cross-repository gate-drift regression checks.

Requires PyYAML. With no arguments checks this repository; --siblings DIR checks
the complete active workspace. --staged validates the Git index independently
of other working-tree edits. No remote API, publication or cloud operation.
"""

import argparse
import copy
import fnmatch
from pathlib import Path
import subprocess

import yaml


CI = {
    "typednotes": "ci.yml",
    "linen": "lean_action_ci.yml",
    "ledger": "lean_action_ci.yml",
    "liaison": "lean_action_ci.yml",
    "lode": "lean_action_ci.yml",
    "lun": "lean_action_ci.yml",
    "secrets": "ci.yml",
    "infra": "lean_action_ci.yml",
    "typednotes-infra": "plan.yml",
    "web-data": "lean_action_ci.yml",
    "lean-curl": "lean_action_ci.yml",
    "lean-pq": "lean_action_ci.yml",
    "lean-server": "lean_action_ci.yml",
    "typednotes-compiler": "lean_action_ci.yml",
}
DOCKER = {"typednotes", "ledger", "liaison", "lode", "lun", "secrets"}
PUBLISHERS = DOCKER | {"linen", "infra"}
GATE_TEST_NAME = "Offline release gate regressions"
MANUAL = {
    "infra": ["live-test.yml"],
    "typednotes-infra": ["apply.yml", "destroy.yml"],
}


def contents(repo, relative, staged=False):
    if staged:
        return subprocess.check_output(["git", "-C", str(repo), "show", f":{relative}"])
    return (repo / relative).read_bytes()


def load(path, staged=False):
    # BaseLoader preserves GitHub's `on` as a string instead of YAML 1.1 bool.
    repo = path.parents[2]
    return yaml.load(contents(repo, path.relative_to(repo), staged), Loader=yaml.BaseLoader)


def original(repo, filename):
    content = subprocess.check_output(
        ["git", "-C", str(repo), "show", f"HEAD:.github/workflows/{filename}"]
    )
    return yaml.load(content, Loader=yaml.BaseLoader)


def triggers(workflow, event, ref):
    config = workflow["on"].get(event)
    if event not in workflow["on"]:
        return False
    config = config or {}
    if event == "push":
        key, value = ("tags", ref[10:]) if ref.startswith("refs/tags/") else ("branches", ref[11:])
        if {"tags", "branches"} & config.keys() and key not in config:
            return False
    else:
        key, value = "branches", ref
    return any(fnmatch.fnmatchcase(value, pattern) for pattern in config.get(key, ["*"]))


def check_ci(repo, name, staged=False):
    filename = CI[name]
    workflow = load(repo / ".github/workflows" / filename, staged)
    for event in ("push", "pull_request"):
        assert workflow["on"][event] == {"branches": ["main"]}, (name, event)
    assert triggers(workflow, "push", "refs/heads/main")
    assert not triggers(workflow, "push", "refs/heads/feature")
    assert not triggers(workflow, "push", "refs/tags/v1.2.3")
    assert triggers(workflow, "pull_request", "main")
    assert not triggers(workflow, "pull_request", "feature")
    before = original(repo, filename)
    assert ("workflow_dispatch" in workflow["on"]) == ("workflow_dispatch" in before["on"])
    if name in PUBLISHERS:
        job_name = "check" if name == "typednotes" else "test" if name == "secrets" else "build"
        steps = workflow["jobs"][job_name]["steps"]
        gate_tests = [
            step
            for job in workflow["jobs"].values()
            for step in job["steps"]
            if "ci/test-require-main-ci.sh" in step.get("run", "")
        ]
        expected = {"name": GATE_TEST_NAME, "shell": "bash", "run": "bash ci/test-require-main-ci.sh"}
        if name in {"linen", "infra"}:
            expected["if"] = "matrix.os == 'ubuntu-24.04'"
            matrix = workflow["jobs"][job_name]["strategy"]["matrix"]["os"]
            assert matrix.count("ubuntu-24.04") == 1
        assert gate_tests == [expected], f"{name}: gate tests must run once"
        assert steps[1] == expected and "checkout@" in steps[0]["uses"], f"{name}: gate tests must run early"
    if name != "typednotes":
        # Preserve every existing test/platform and Plan step; allow only the
        # explicitly validated offline regression step added above.
        def without_gate_tests(jobs):
            jobs = copy.deepcopy(jobs)
            for job in jobs.values():
                job["steps"] = [step for step in job["steps"] if step.get("name") != GATE_TEST_NAME]
            return jobs

        assert without_gate_tests(workflow["jobs"]) == without_gate_tests(before["jobs"]), f"{name}: changed CI jobs"
    for filename in MANUAL.get(name, []):
        live = load(repo / ".github/workflows" / filename, staged)
        assert set(live["on"]) == {"workflow_dispatch"}
        assert live == original(repo, filename), f"{name}: changed live operation"
    if name == "infra":
        for filename in ("cleanup.yml", "pages.yml"):
            assert load(repo / ".github/workflows" / filename, staged) == original(repo, filename)
        scaffold = contents(repo, "Infra/Cli/New.lean", staged).decode()
        header = scaffold.split("private def githubPlan (name : String) : String :=", 1)[1]
        generated = yaml.load(header.split("jobs:", 1)[0].lstrip().removeprefix('"'), Loader=yaml.BaseLoader)
        assert generated["on"] == workflow["on"], "infra: generated Plan trigger drift"


def check_publisher(repo, name, filename, publish_job, staged=False):
    workflow = load(repo / ".github/workflows" / filename, staged)
    assert workflow["on"]["push"] == {"tags": ["v*.*.*"]}, (name, filename)
    assert not triggers(workflow, "push", "refs/heads/main")
    assert not triggers(workflow, "push", "refs/heads/feature")
    assert triggers(workflow, "push", "refs/tags/v1.2.3")
    assert not triggers(workflow, "push", "refs/tags/nightly")
    assert "pull_request" not in workflow["on"] and "workflow_run" not in workflow["on"]
    assert workflow["concurrency"]["cancel-in-progress"] == "false"
    assert workflow["permissions"] == {"contents": "read"}
    jobs = workflow["jobs"]
    verify = jobs["verify"]
    assert verify["permissions"] == {"contents": "read", "actions": "read"}
    checkout = verify["steps"][0]
    assert checkout["with"]["fetch-depth"] == "0"
    assert checkout["with"]["persist-credentials"] == "false"
    gate = next(step for step in verify["steps"] if step.get("id") == "ci")
    assert gate["run"] == f'bash ci/require-main-ci.sh {CI[name]} "$TAG"'
    assert gate["env"]["GH_TOKEN"] == "${{ github.token }}"
    assert verify["outputs"]["sha"] == "${{ steps.ci.outputs.sha }}"
    publish = jobs[publish_job]
    assert publish["needs"] == "verify"
    assert publish["steps"][0]["with"]["ref"] == "${{ needs.verify.outputs.sha }}"
    assert publish["steps"][0]["with"]["persist-credentials"] == "false"
    if "workflow_dispatch" in workflow["on"]:
        tag = workflow["on"]["workflow_dispatch"]["inputs"]["tag"]
        assert tag["required"] == "true" and tag["type"] == "string"
        assert checkout["with"]["ref"] == "refs/tags/${{ inputs.tag || github.ref_name }}"
    if filename == "docker-publish.yml":
        assert set(jobs) == {"verify", "build-and-push"}
        assert publish["permissions"] == {"contents": "read", "packages": "write"}
        metadata = next(step for step in publish["steps"] if step.get("id") == "meta")
        tags = metadata["with"]["tags"]
        assert "edge" not in tags and "type=raw,value=latest" not in tags
        assert "type=semver,pattern={{version}}" in tags
    elif name == "linen":
        assert set(jobs) == {"verify", "publish"}, "Tag releases must not rerun the test matrix"
        assert any("check-release.sh" in step.get("run", "") for step in verify["steps"])
        assert any("upload-artifact" in step.get("uses", "") for step in verify["steps"])
        assert any("download-artifact" in step.get("uses", "") for step in publish["steps"])
    elif name == "infra":
        assert any("check-release-version.sh" in step.get("run", "") for step in publish["steps"])
        assert any("CHANGELOG.md" in step.get("run", "") for step in publish["steps"])
    elif filename == "crates-publish.yml":
        assert "CARGO_REGISTRY_TOKEN" not in publish.get("env", {})
        version = next(step for step in publish["steps"] if step.get("name") == "Check tag matches workspace version")
        assert "if" not in version and version["env"]["TAG"] == "${{ needs.verify.outputs.tag }}"
        assert [step["name"] for step in publish["steps"] if "CARGO_REGISTRY_TOKEN" in step.get("env", {})] == ["Publish library crates"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--siblings", type=Path)
    parser.add_argument("--staged", action="store_true", help="Validate staged files instead of the working tree")
    args = parser.parse_args()
    app = Path(__file__).resolve().parents[1]
    repos = {name: args.siblings / name for name in CI} if args.siblings else {"typednotes": app}
    canonical = contents(app, "ci/require-main-ci.sh", args.staged)
    canonical_test = contents(app, "ci/test-require-main-ci.sh", args.staged)
    for name, repo in repos.items():
        check_ci(repo, name, args.staged)
        if name in PUBLISHERS:
            assert contents(repo, "ci/require-main-ci.sh", args.staged) == canonical, f"{name}: shared gate drift"
            assert contents(repo, "ci/test-require-main-ci.sh", args.staged) == canonical_test, f"{name}: shared gate test drift"
        if name in DOCKER:
            check_publisher(repo, name, "docker-publish.yml", "build-and-push", args.staged)
        if name in {"linen", "infra"}:
            check_publisher(repo, name, "release.yml", "publish" if name == "linen" else "release", args.staged)
        if name == "secrets":
            check_publisher(repo, name, "crates-publish.yml", "publish", args.staged)
        print(f"ok: trigger/publisher/guard policy: {name}")
    print(f"ok: {len(repos)} repositories; exact gate copies; main/PR/tag/manual behavior")


if __name__ == "__main__":
    main()
