#!/usr/bin/env python3
"""Publish every crate whose version is not on crates.io yet, root workspace first, then fsr.

  scripts/release_publish.py --plan      list what would be published and stop
  scripts/release_publish.py --dry-run   package and verify without uploading
  scripts/release_publish.py             publish

cargo publish --workspace orders the crates by their dependencies, dev-dependencies included. It waits for each batch to reach the index.
"""

import argparse
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKSPACES = [ROOT, ROOT / "fsr"]
INDEX = "https://index.crates.io"
USER_AGENT = "snapfire-release-publish"


def index_path(name: str) -> str:
  name = name.lower()
  if len(name) <= 2:
    return f"{len(name)}/{name}"
  if len(name) == 3:
    return f"3/{name[0]}/{name}"
  return f"{name[:2]}/{name[2:4]}/{name}"


def published_versions(name: str) -> set[str]:
  request = urllib.request.Request(f"{INDEX}/{index_path(name)}", headers={"User-Agent": USER_AGENT})
  try:
    with urllib.request.urlopen(request, timeout=30) as response:
      return {json.loads(line)["vers"] for line in response.read().decode().splitlines() if line.strip()}
  except urllib.error.HTTPError as e:
    if e.code == 404:
      return set()
    raise


def members(workspace: Path) -> list[tuple[str, str]]:
  out = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=workspace, check=True, capture_output=True, text=True).stdout
  return [(p["name"], p["version"]) for p in json.loads(out)["packages"] if p.get("publish") != []]


def has_patch(workspace: Path) -> bool:
  return any(line.strip().startswith("[patch") for line in (workspace / "Cargo.toml").read_text().splitlines())


def main() -> int:
  parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
  parser.add_argument("--plan", action="store_true", help="list the crates that would be published and stop")
  parser.add_argument("--dry-run", action="store_true", help="pass --dry-run to cargo publish")
  args = parser.parse_args()

  plans = []
  for workspace in WORKSPACES:
    crates = members(workspace)
    pending = [(n, v) for n, v in crates if v not in published_versions(n)]
    done = [n for n, v in crates if (n, v) not in pending]
    plans.append((workspace, pending, done))
    for name, version in pending:
      print(f"{workspace.relative_to(ROOT)}  {name} {version}")

  if args.plan:
    return 0

  for workspace, pending, _ in plans:
    if pending and has_patch(workspace):
      print(f"refused: {workspace / 'Cargo.toml'} has a [patch] table", file=sys.stderr)
      return 1

  started = time.monotonic()
  for workspace, pending, done in plans:
    if not pending:
      continue
    command = ["cargo", "publish", "--workspace"] + [arg for name in done for arg in ("--exclude", name)]
    if args.dry_run:
      command.append("--dry-run")
    print(f"publishing {len(pending)} from {workspace}", flush=True)
    step = time.monotonic()
    result = subprocess.run(command, cwd=workspace)
    if result.returncode != 0:
      print(f"cargo publish failed in {workspace} with exit {result.returncode}", file=sys.stderr)
      return result.returncode
    print(f"{workspace}: {len(pending)} crates in {time.monotonic() - step:.0f} s", flush=True)

  print(f"PUBLISH DONE in {time.monotonic() - started:.0f} s")
  return 0


if __name__ == "__main__":
  sys.exit(main())
