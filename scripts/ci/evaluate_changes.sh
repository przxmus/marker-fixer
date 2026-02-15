#!/usr/bin/env bash
set -euo pipefail

BASE_SHA="${1:-}"
HEAD_SHA="${2:-}"

if [[ -z "$BASE_SHA" || -z "$HEAD_SHA" ]]; then
  echo "Usage: scripts/ci/evaluate_changes.sh <base_sha> <head_sha>" >&2
  exit 1
fi

if [[ "$BASE_SHA" == "0000000000000000000000000000000000000000" ]]; then
  # Initial push fallback
  BASE_SHA="$(git rev-list --max-parents=0 "$HEAD_SHA")"
fi

CHANGED_FILES=()
while IFS= read -r line; do
  CHANGED_FILES+=("$line")
done < <(git diff --name-only "$BASE_SHA" "$HEAD_SHA")

is_docs_file() {
  local file="$1"
  case "$file" in
    README.md|CHANGELOG.md|CONTRIBUTING.md|LICENSE*|*.md|docs/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

is_version_only_manifest_change() {
  local cargo_toml_diff
  cargo_toml_diff="$(git diff --unified=0 "$BASE_SHA" "$HEAD_SHA" -- Cargo.toml | grep -E '^[+-]' | grep -vE '^\+\+\+|^---' || true)"

  if [[ -z "$cargo_toml_diff" ]]; then
    return 1
  fi

  local filtered
  filtered="$(echo "$cargo_toml_diff" | grep -vE '^[+-]version = "[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?"$' || true)"

  [[ -z "$filtered" ]]
}

all_docs=true
for file in "${CHANGED_FILES[@]}"; do
  if ! is_docs_file "$file"; then
    all_docs=false
    break
  fi
done

build_needed=true
reason="code_changes"

if [[ ${#CHANGED_FILES[@]} -eq 0 ]]; then
  build_needed=false
  reason="no_changes"
elif [[ "$all_docs" == true ]]; then
  build_needed=false
  reason="docs_only"
else
  files_joined="$(printf '%s\n' "${CHANGED_FILES[@]}" | sort | tr '\n' ' ')"
  if [[ "$files_joined" == "Cargo.lock Cargo.toml " ]] && is_version_only_manifest_change; then
    build_needed=false
    reason="version_bump_only"
  fi
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "build_needed=$build_needed"
    echo "reason=$reason"
  } >> "$GITHUB_OUTPUT"
else
  echo "build_needed=$build_needed"
  echo "reason=$reason"
fi

>&2 echo "Changed files:" >&2
printf ' - %s\n' "${CHANGED_FILES[@]}" >&2 || true
>&2 echo "Decision: build_needed=$build_needed (reason=$reason)"
