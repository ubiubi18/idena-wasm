#!/usr/bin/env python3
"""Verify that idena-wasm uses the Wasmer revision from the stack lock."""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
LOCK = ROOT / "compatibility" / "stack-lock.json"
SHA1_RE = re.compile(r"^[0-9a-f]{40}$")


def fail(message: str) -> None:
    raise SystemExit(message)


def main() -> int:
    payload = json.loads(LOCK.read_text(encoding="utf-8"))
    components = {item["name"]: item for item in payload.get("components", [])}
    wasm_commit = components.get("idena-wasm", {}).get("commit", "")
    wasmer_commit = components.get("wasmer", {}).get("commit", "")
    if not SHA1_RE.fullmatch(wasm_commit) or not SHA1_RE.fullmatch(wasmer_commit):
        fail("stack lock does not contain valid idena-wasm and Wasmer commits")

    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    revisions = re.findall(
        r'git\s*=\s*"https://github\.com/ubiubi18/wasmer"\s*,\s*rev\s*=\s*"([0-9a-f]{40})"',
        cargo,
    )
    if len(revisions) != 5 or set(revisions) != {wasmer_commit}:
        fail("Cargo.toml does not pin every Wasmer crate to the locked revision")

    subprocess.run(["git", "cat-file", "-e", f"{wasm_commit}^{{commit}}"], cwd=ROOT, check=True)
    subprocess.run(["git", "merge-base", "--is-ancestor", wasm_commit, "HEAD"], cwd=ROOT, check=True)
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", f"{wasm_commit}..HEAD"], cwd=ROOT, text=True
    ).splitlines()
    allowed = {
        "README.md",
        "compatibility/stack-lock.json",
        "scripts/check-compatibility-lock.py",
        ".github/workflows/compatibility.yml",
    }
    unexpected = sorted(set(changed) - allowed)
    if unexpected:
        fail("runtime-affecting paths changed after locked idena-wasm commit: " + ", ".join(unexpected))

    print("idena-wasm compatibility lock passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
