#!/usr/bin/env bash
# Provision the real OpenSIPS tree and binary that the proof suite
# runs against.
#
# A skipped test is a failed test in this repo: the suite refuses to
# run without this environment rather than quietly reporting green
# while proving nothing.  This script is how you satisfy it, and CI
# runs this same script — the gate and its fixer derive from one rule,
# so a green CI means the proofs actually ran.
#
#   eval "$(scripts/proof-env.sh)"   # local shell
#   scripts/proof-env.sh             # in CI: appends to $GITHUB_ENV
#
# Everything lands under .proof/ (gitignored) and is reused on the
# next run; CI caches that directory keyed on the tag below.
set -euo pipefail

TAG="${OPENSIPS_TAG:-4.0.1}"
# The releases the versioned catalogue covers: the newest of each live
# line, oldest first. Only $TAG is ever BUILT — the older ones are
# harvested from source alone, which is all the catalogue needs, so
# adding a release costs a shallow clone rather than a compile.
OLDER_TAGS="${OPENSIPS_OLDER_TAGS:-3.5.9 3.6.8}"
ROOT="${PROOF_ROOT:-${PWD}/.proof}"
SRC="$ROOT/opensips-$TAG"
INST="$ROOT/inst-$TAG"

# The only modules any test loads.  Building the full set is minutes
# of compile for no extra coverage.
MODULES="tm"

log() { printf '%s\n' "$*" >&2; }

if [ ! -x "$INST/sbin/opensips" ]; then
	log "provisioning OpenSIPS $TAG into $ROOT"
	rm -rf "$SRC" "$INST"
	mkdir -p "$ROOT"
	git clone -q --depth 1 --branch "$TAG" \
		https://github.com/OpenSIPS/opensips.git "$SRC"

	exclude=""
	for m in $(ls "$SRC/modules"); do
		case " $MODULES " in
		*" $m "*) ;;
		*) exclude="$exclude $m" ;;
		esac
	done

	make -C "$SRC" PREFIX="$INST" exclude_modules="$exclude" \
		-j"$(nproc)" all >/dev/null
	make -C "$SRC" PREFIX="$INST" exclude_modules="$exclude" \
		install >/dev/null
	log "built $("$INST/sbin/opensips" -V 2>&1 | head -1)"
fi

# lib vs lib64 varies by platform; ask the filesystem rather than guess
TM="$(find "$INST" -name tm.so -print -quit)"
[ -n "$TM" ] || {
	log "tm.so missing under $INST — the build did not install modules"
	exit 1
}
MPATH="$(dirname "$TM")/"

# Source-only checkouts of the older supported releases. The
# versioned catalogue is base-plus-deltas across these, and its
# round-trip proof needs every one of them present.
TREES=""
for t in $OLDER_TAGS; do
	d="$ROOT/opensips-$t"
	if [ ! -d "$d/modules" ]; then
		log "cloning OpenSIPS $t (source only) into $ROOT"
		rm -rf "$d"
		git clone -q --depth 1 --branch "$t" \
			https://github.com/OpenSIPS/opensips.git "$d"
	fi
	TREES="${TREES:+$TREES,}$t=$d"
done
TREES="${TREES:+$TREES,}$TAG=$SRC"

emit() {
	printf 'OPENSIPS_LSP_TEST_TREES=%s\n' "$TREES"
	printf 'OPENSIPS_LSP_TEST_TREE=%s\n' "$SRC"
	printf 'OPENSIPS_LSP_TEST_BIN=%s\n' "$INST/sbin/opensips"
	printf 'OPENSIPS_LSP_TEST_MPATH=%s\n' "$MPATH"
}

if [ -n "${GITHUB_ENV:-}" ]; then
	emit >>"$GITHUB_ENV"
	log "proof environment written to \$GITHUB_ENV"
else
	emit | sed 's/^/export /'
fi
