#!/usr/bin/env bash
# Runs the fuzz targets in fuzz/. One script, so CI, the planted-panic check
# and a developer all run a target the same way.
#
#   scripts/fuzz.sh list               the targets, one per line
#   scripts/fuzz.sh run TARGET SECONDS run one target from its seeds
#   scripts/fuzz.sh plant [TARGET...]  plant a panic in the code each target
#                                      reads, run the target, expect the crash
#
# A target's seeds are fuzz/seeds/<target>/; the cfg target also reads the
# cfg files the tests and the grammar ship. Both are only read:
# what libFuzzer adds goes to fuzz/corpus/<target>/ and a crash input to
# fuzz/artifacts/<target>/, both ignored by git.
#
# Each plant is a line of fuzz/plants.txt: target, file, and the text of
# exactly one line of that file (without its indentation), separated by
# tabs. The line
#   panic!("planted fault");
# goes in just before that line, with its indentation. The target must
# crash with that message, at that line, within PLANT_SECONDS (default
# 300). The file is then put back and compared with its copy taken before
# the plant, byte for byte. The copies are kept in fuzz/target/plant/ until
# then, so a run that is killed is put right by the next one.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
fuzz="$root/fuzz"

targets() {
  python3 -I -c '
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    bins = tomllib.load(f).get("bin", [])
for b in bins:
    print(b["name"])
' "$fuzz/Cargo.toml"
}

extra_corpora() {
  case "$1" in
    cfg) printf '%s\n' "$root/tests/grammar" "$root/tree-sitter-opensips/test/highlight" ;;
  esac
}

# run TARGET SECONDS [WORK_DIR ARTIFACT_PREFIX]
run() {
  local target="$1" seconds="$2"
  local work="${3:-$fuzz/corpus/$target}" artifacts="${4:-$fuzz/artifacts/$target/}"
  if ! [[ "$seconds" =~ ^[1-9][0-9]*$ ]]; then
    echo "fuzz.sh: seconds must be a positive whole number, got '$seconds'" >&2
    return 2
  fi
  if ! targets | grep -qx -- "$target"; then
    echo "fuzz.sh: no target named '$target'" >&2
    return 2
  fi
  mkdir -p "$work" "$(dirname "$artifacts/x")"
  local corpora=("$work" "$fuzz/seeds/$target")
  local extra
  while IFS= read -r extra; do
    [ -n "$extra" ] && corpora+=("$extra")
  done < <(extra_corpora "$target")
  # cargo fuzz runs from fuzz/, so fuzz/rust-toolchain.toml picks the
  # nightly toolchain the sanitizer needs. The target is that toolchain's
  # host: without --target, cargo fuzz builds for the platform it was itself
  # built for, which for the musl release binary CI uses has no sanitizer.
  local host
  host="$(cd "$fuzz" && rustc -vV | sed -n 's/^host: //p')"
  # A target that writes files writes them under TMPDIR; a crash skips its
  # own cleanup, so the run removes them.
  local tmp status=0
  tmp="$(mktemp -d)"
  (cd "$fuzz" && TMPDIR="$tmp" cargo fuzz run --fuzz-dir "$fuzz" --target "$host" "$target" "${corpora[@]}" -- \
    -max_total_time="$seconds" -print_final_stats=1 -artifact_prefix="$artifacts") || status=$?
  rm -rf "$tmp"
  return "$status"
}

plant() {
  local seconds="${PLANT_SECONDS:-300}"
  local wanted=("$@") all list failed=0
  list="$(targets)"
  mapfile -t all <<<"$list"
  [ "${#wanted[@]}" -gt 0 ] || wanted=("${all[@]}")
  # Every target has a plant, so none can go unchecked.
  local t
  for t in "${all[@]}"; do
    if ! cut -f1 "$fuzz/plants.txt" | grep -qx -- "$t"; then
      echo "fuzz.sh: target '$t' has no plant in fuzz/plants.txt" >&2
      failed=1
    fi
  done
  [ "$failed" = 0 ] || return 1

  scratch="$fuzz/target/plant"
  if [ -f "$scratch/planted" ]; then
    echo "fuzz.sh: an earlier run left files planted; putting them back" >&2
    restore_all
  fi
  rm -rf "$scratch"
  mkdir -p "$scratch"
  trap 'restore_all; rm -rf "$scratch"' EXIT
  trap 'exit 130' INT TERM
  local n=0 caught=0
  local target file anchor
  while IFS=$'\t' read -r target file anchor; do
    case "$target" in '' | '#'*) continue ;; esac
    printf '%s\n' "${wanted[@]}" | grep -qx -- "$target" || continue
    n=$((n + 1))
    local path="$root/$file" copy="$scratch/copy-$n"
    cp -p "$path" "$copy"
    printf '%s\t%s\n' "$path" "$copy" >>"$scratch/planted"
    local at
    if ! at="$(python3 -I - "$path" "$anchor" <<'PLANT'
import sys
path, anchor = sys.argv[1], sys.argv[2]
lines = open(path, encoding="utf-8").read().split("\n")
hits = [i for i, line in enumerate(lines) if line.strip() == anchor]
if len(hits) != 1:
    sys.exit(f"{len(hits)} lines of {path} read {anchor!r}, not 1")
i = hits[0]
indent = lines[i][: len(lines[i]) - len(lines[i].lstrip())]
lines.insert(i, indent + 'panic!("planted fault");')
open(path, "w", encoding="utf-8").write("\n".join(lines))
print(i + 1)
PLANT
)"; then
      echo "fuzz.sh: plant $n ($target) did not apply" >&2
      return 1
    fi
    if cmp -s "$path" "$copy"; then
      echo "fuzz.sh: plant $n ($target) left the file unchanged" >&2
      return 1
    fi
    local log="$scratch/log-$n"
    set +e
    run "$target" "$seconds" "$scratch/corpus-$n" "$scratch/artifacts-$n/" </dev/null >"$log" 2>&1
    local status=$?
    set -e
    restore "$path" "$copy"
    # The crash must be this plant's, at the planted line, and not one
    # left in a build from an earlier plant.
    if [ "$status" -ne 0 ] && grep -qx 'planted fault' "$log" &&
      grep -qE "panicked at (.*/)?${file//./\\.}:$at:[0-9]+:\$" "$log" &&
      grep -q 'libFuzzer: deadly signal' "$log"; then
      caught=$((caught + 1))
      echo "caught: $target ($file:$at)"
    else
      echo "MISSED: $target ($file), exit $status; the log ends:" >&2
      tail -n 30 "$log" >&2
      failed=1
    fi
    rm -rf "$scratch/corpus-$n" "$scratch/artifacts-$n" "$log"
  done <"$fuzz/plants.txt"
  if [ "$n" -eq 0 ]; then
    echo "fuzz.sh: no plant matched ${wanted[*]}" >&2
    return 1
  fi
  echo "$caught of $n planted panics caught"
  return "$failed"
}

# Puts a planted file back and proves it is the file as it was. The copy
# keeps no timestamp: a restored file must look newer than the planted
# build, or cargo would keep using that build.
restore() {
  cp "$2" "$1"
  touch "$1"
  if ! cmp -s "$1" "$2"; then
    echo "fuzz.sh: RESTORE FAILED for $1; its copy is $2" >&2
    trap - EXIT
    exit 3
  fi
  : >"$scratch/restored-$(basename "$2")"
}

# On any exit, puts back every file still planted.
restore_all() {
  [ -f "$scratch/planted" ] || return 0
  local path copy
  while IFS=$'\t' read -r path copy; do
    [ -e "$scratch/restored-$(basename "$copy")" ] || restore "$path" "$copy"
  done <"$scratch/planted"
}

case "${1:-}" in
  list) targets ;;
  run)
    [ $# -eq 3 ] || { echo "usage: scripts/fuzz.sh run TARGET SECONDS" >&2; exit 2; }
    run "$2" "$3" ;;
  plant) shift; plant "$@" ;;
  *) echo "usage: scripts/fuzz.sh list | run TARGET SECONDS | plant [TARGET...]" >&2; exit 2 ;;
esac
