#!/usr/bin/env bash
# Runs `fsr check` on every example app. Extra arguments go to each check (--no-typecheck, --tsc-version).
# $FSR names the binary; without it the workspace CLI is built first.
set -u
cd "$(dirname "$0")"

if [ -z "${FSR:-}" ]; then
  cargo build -q -p snapfire_fsr_cli --manifest-path ../Cargo.toml || exit 1
  FSR=../target/debug/fsr
fi

failed=()
for config in */config/app.toml; do
  ex=${config%%/*}
  [ -d "$ex/app" ] || continue
  shell_app=$(sed -n 's|^shell = "\(.*\)/generated/shell.json"$|\1|p' "$config")
  if [ -n "$shell_app" ] && [ ! -f "$ex/$shell_app/generated/shell.json" ]; then
    echo "== $ex: building its shell $shell_app"
    if ! "$FSR" build "$ex/$shell_app" --no-typecheck > /dev/null; then
      failed+=("$ex (shell $shell_app did not build)")
      continue
    fi
  fi
  echo "== $ex"
  "$FSR" check "$ex/app" "$@" || failed+=("$ex")
done

if [ ${#failed[@]} -gt 0 ]; then
  printf 'failed: %s\n' "${failed[@]}"
  exit 1
fi
echo "every example passed"
