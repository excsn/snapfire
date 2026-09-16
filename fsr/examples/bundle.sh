#!/usr/bin/env bash
# Bundles every example app and loads each deploy tree back through the host.
# `fsr check` builds from the project, where the configuration is hand-written
# and `icons/`, `vendor/` and `dist/` sit where inference looks. A bundle moves
# all three, so a tree the host cannot boot is a defect neither half's own
# tests see. Extra arguments go to each bundle.
# $FSR names the binary; without it the workspace CLI is built first.
set -u
cd "$(dirname "$0")"

if [ -z "${FSR:-}" ]; then
  cargo build -q -p snapfire_fsr_cli --manifest-path ../Cargo.toml || exit 1
  FSR=../target/debug/fsr
fi
FSR=$(cd "$(dirname "$FSR")" && pwd)/$(basename "$FSR")

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

failed=()
for config in */config/app.toml; do
  ex=${config%%/*}
  [ -d "$ex/app" ] || continue
  # A tree is complete only with what `fsr bundle` says goes beside it. A
  # shell's site artifacts sit outside the tree and a Rust extension lives in
  # the binary, so neither loads headless; the skip is printed, not silent.
  if grep -q '^\[site\]' "$config"; then
    continue
  fi
  if grep -q '^\[sites' "$config"; then
    echo "== $ex (skipped: a shell needs its site artifacts placed beside the tree)"
    continue
  fi
  if grep -rq '\.extension(' "$ex/src" 2>/dev/null; then
    echo "== $ex (skipped: its extensions are registered by the binary)"
    continue
  fi
  echo "== $ex"
  tree="$out/$ex"
  if ! "$FSR" bundle "$ex/app" --out "$tree" --no-doctor "$@" > /dev/null; then
    failed+=("$ex (did not bundle)")
    continue
  fi
  "$FSR" prerender "$tree/app" || failed+=("$ex (the host did not load the tree)")
done

if [ ${#failed[@]} -gt 0 ]; then
  printf 'failed: %s\n' "${failed[@]}"
  exit 1
fi
echo "every deploy tree loaded"
