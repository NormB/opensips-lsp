#!/usr/bin/env bash
# Run the tree-sitter grammar's C under AddressSanitizer and
# UndefinedBehaviorSanitizer.
#
# The grammar ships C that editors compile and load: the hand-written
# src/scanner.c and the generated src/parser.c.  `check` builds both with
# -fsanitize=address,undefined, runs the grammar's corpus tests through
# that build, then parses every .cfg under the given directories with it,
# and fails on any sanitizer report.  `plant` proves `check` can fail: it
# plants a one-byte out-of-bounds read, and then a signed integer
# overflow, in a COPY of the grammar's scanner.c, expects `check` to
# report each, and compares the real scanner.c with its copy taken before.
#
#   scripts/grammar-sanitize.sh check <grammar-dir> <cfg-dir>...
#   scripts/grammar-sanitize.sh plant <grammar-dir> <cfg-dir>...
#
# CI runs both (.github/workflows/ci.yml, job grammar-sanitizers).  Needs
# gcc with its sanitizer runtimes, and npx for tree-sitter-cli 0.26.
set -euo pipefail

CC="${CC:-gcc}"

# Every work directory is made under this one, which is removed when the
# script exits however it exits: `fail` exits from inside functions and
# subshells, where a RETURN trap would not run.
ROOT_TMP="$(mktemp -d)"
trap 'rm -rf "$ROOT_TMP"' EXIT

fail() { printf 'grammar-sanitize: %s\n' "$*" >&2; exit 1; }

# check <grammar-dir> <cfg-dir>...
check() {
	local grammar lang
	grammar="$(cd "$1" && pwd)"
	shift
	lang="${grammar##*/tree-sitter-}"
	local work
	work="$(mktemp -d -p "$ROOT_TMP")"
	mkdir -p "$work/libdir" "$work/reports"

	# The native CLI binary, not npx or the package's node launcher:
	# everything below runs with the sanitizer runtimes preloaded, and
	# node should not run under them.
	local launcher ts
	launcher="$(cd "$grammar" && npx --yes -p tree-sitter-cli@0.26 -c 'command -v tree-sitter')"
	ts="$(dirname "$(readlink -f "$launcher")")/tree-sitter"
	[ -x "$ts" ] && [ "$(head -c 4 "$ts" | od -An -c | tr -d ' ')" = '177ELF' ] ||
		fail "no native tree-sitter binary next to $launcher"

	"$CC" -shared -fPIC -g -O1 -fsanitize=address,undefined -fno-omit-frame-pointer \
		-I "$grammar/src" "$grammar/src/parser.c" "$grammar/src/scanner.c" \
		-o "$work/$lang.so"

	# `tree-sitter test` also runs the highlight tests, which load the
	# grammar through the CLI's own cache and compile it when the cache
	# is stale.  Fill a private cache now, without the sanitizers, so the
	# compiler never runs with them preloaded below; the corpus tests and
	# the parses use the sanitized build named by --lib-path.
	export TREE_SITTER_LIBDIR="$work/libdir"
	(cd "$grammar" && "$ts" test >/dev/null 2>&1) || true

	local preload
	preload="$("$CC" -print-file-name=libasan.so) $("$CC" -print-file-name=libubsan.so)"
	sanitized() {
		LD_PRELOAD="$preload" \
			ASAN_OPTIONS=halt_on_error=1:exitcode=86:log_path="$work/reports/asan" \
			UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1:exitcode=86:log_path="$work/reports/ubsan" \
			"$@"
	}
	reports() {
		local found=0 f
		for f in "$work"/reports/*; do
			[ -e "$f" ] || continue
			found=1
			cat "$f"
		done
		return "$found"
	}

	# STAGES names what to run; `plant` runs each stage on its own so
	# that each is shown to use the sanitized build.
	local stages=" ${STAGES:-corpus sweep} " rc=0
	case "$stages" in *" corpus "*) ;; *) rc=skip ;; esac
	[ "$rc" = skip ] || (cd "$grammar" && sanitized "$ts" test --lib-path "$work/$lang.so" --lang-name "$lang") || rc=$?
	reports || fail "the corpus tests raised a sanitizer report"
	[ "$rc" = skip ] || [ "$rc" -eq 0 ] || fail "the corpus tests failed under the sanitizers (exit $rc)"
	case "$stages" in *" sweep "*) ;; *)
		echo "check: corpus tests ran clean under ASan and UBSan"
		return 0
		;;
	esac

	local -a cfgs=()
	local dir f
	for dir in "$@"; do
		[ -d "$dir" ] || fail "$dir is not a directory"
		dir="$(cd "$dir" && pwd)"
		while IFS= read -r -d '' f; do cfgs+=("$f"); done < <(find "$dir" -name '*.cfg' -type f -print0 | sort -z)
	done
	[ "${#cfgs[@]}" -gt 0 ] || fail "no .cfg files under $*"
	rc=0
	sanitized "$ts" parse --quiet --lib-path "$work/$lang.so" --lang-name "$lang" \
		"${cfgs[@]}" >"$work/parse.out" 2>"$work/parse.err" || rc=$?
	reports || fail "parsing the .cfg files raised a sanitizer report"
	# Exit 1 with an ERROR or MISSING node printed is a syntax error in a
	# file, which is the grammar's business and the sweep tests', not a
	# memory error.  Exit 1 with an `Error:` from the CLI (a file it could
	# not read, a library it could not load) is a failure of this check.
	if [ "$rc" -ne 0 ]; then
		if [ "$rc" -ne 1 ] || grep -q 'Error:' "$work/parse.err" ||
			! grep -qE '\((ERROR|MISSING)' "$work/parse.out"; then
			cat "$work/parse.err" >&2
			fail "the parse exited $rc under the sanitizers"
		fi
	fi
	echo "check: corpus tests and ${#cfgs[@]} .cfg files ran clean under ASan and UBSan"
}

# The line of scanner.c a fault is planted in front of: the first line
# of the scan function, so every call to the external scanner runs it.
ANCHOR='if (!valid_symbols[BLOCK_COMMENT]) return false;'

# plant <grammar-dir> <cfg-dir>...
plant() {
	local grammar
	grammar="$(cd "$1" && pwd)"
	shift
	local work
	work="$(mktemp -d -p "$ROOT_TMP")"
	cp -p "$grammar/src/scanner.c" "$work/scanner.c.before"
	[ "$(grep -cF "$ANCHOR" "$grammar/src/scanner.c")" = 1 ] ||
		fail "the plant anchor is not in $grammar/src/scanner.c exactly once"

	local -a names=(heap-buffer-overflow signed-integer-overflow)
	# The read goes through an opaque allocator so that only ASan, not
	# UBSan's object-size check, can know the block is one byte long.
	local -a faults=(
		'{ void *(*volatile alloc)(size_t) = malloc; char *probe = alloc(1); volatile char byte = probe[1]; (void)byte; free(probe); }'
		'{ volatile int big = INT_MAX; big = big + 1; (void)big; }'
	)
	local -a expect=('AddressSanitizer: heap-buffer-overflow' 'runtime error: signed integer overflow')
	local i copy text out rc stage
	for i in "${!faults[@]}"; do
	for stage in corpus sweep; do
		copy="$work/${names[$i]}/$(basename "$grammar")"
		mkdir -p "$copy"
		tar -C "$grammar" --exclude=node_modules -cf - . | tar -C "$copy" -xf -
		text="$(<"$copy/src/scanner.c")"
		text="${text/"$ANCHOR"/"${faults[$i]}
  $ANCHOR"}"
		printf '#include <limits.h>\n#include <stdlib.h>\n%s\n' "$text" >"$copy/src/scanner.c"
		grep -qF "${faults[$i]}" "$copy/src/scanner.c" || fail "the ${names[$i]} plant did not apply"
		rc=0
		out="$( (STAGES=$stage check "$copy" "$@") 2>&1)" || rc=$?
		if [ "$rc" -eq 0 ]; then
			printf '%s\n' "$out" | head -40 >&2
			fail "PLANT SURVIVED: the $stage stage passed with a ${names[$i]} in scanner.c"
		fi
		grep -qF "${expect[$i]}" <<<"$out" || {
			printf '%s\n' "$out" | head -40 >&2
			fail "the $stage stage failed with a ${names[$i]} planted, but did not report it (${expect[$i]})"
		}
		grep -m1 -F "${expect[$i]}" <<<"$out"
		echo "plant: ${names[$i]} caught by the $stage stage (exit $rc)"
	done
	done
	cmp -s "$work/scanner.c.before" "$grammar/src/scanner.c" ||
		fail "$grammar/src/scanner.c changed while planting"
	echo "plant: $grammar/src/scanner.c is unchanged (cmp)"
}

case "${1:-}" in
check | plant)
	mode="$1"
	shift
	[ $# -ge 2 ] || fail "usage: $0 $mode <grammar-dir> <cfg-dir>..."
	"$mode" "$@"
	;;
*) fail "usage: $0 check|plant <grammar-dir> <cfg-dir>..." ;;
esac
