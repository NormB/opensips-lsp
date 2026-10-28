#!/usr/bin/env bash
# Tests the git hooks in .githooks/ in a throwaway repository with a small
# crate, a fuzz package and a client of its own, a local bare remote and its
# own SSH signing keys. Each case either commits or pushes, and must be
# accepted or refused as it says. The hooks and the scripts they run are this
# tree's own copies; only the crate, the fuzz package and the client are
# stand-ins, small enough to build in seconds.
#
# The forbidden samples are put together at run time, so this file does not
# contain them and can be committed through the hooks it tests.
set -euo pipefail
src="$(git rev-parse --show-toplevel)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export CARGO_TARGET_DIR="$work/target"

ssh-keygen -q -t ed25519 -N '' -C test -f "$work/good"
ssh-keygen -q -t ed25519 -N '' -C test -f "$work/stranger"
printf 'test@example.com %s\n' "$(cut -d' ' -f1,2 "$work/good.pub")" >"$work/allowed_signers"
git init -q --bare "$work/remote.git"

repo="$work/repo"
mkdir -p "$repo/src" "$repo/scripts" "$repo/fuzz/src" "$repo/client/node_modules"
cd "$repo"
git init -q -b main
git config user.name "Test User"
git config user.email "test@example.com"
git config gpg.format ssh
git config user.signingkey "$work/good.pub"
git config gpg.ssh.allowedSignersFile "$work/allowed_signers"
git config commit.gpgsign true
git config core.hooksPath .githooks
git remote add origin "$work/remote.git"

cp -R "$src/.githooks" .
cp "$src/rustfmt.toml" .
for s in attribution-scan.py fmt.sh gate.sh; do cp "$src/scripts/$s" scripts/; done

# The real manifest's [lints] tables, so the stand-in crate is linted with
# exactly the lints the server is.
lints="$(awk '/^\[lints/ { on = 1 } /^\[/ && !/^\[lints/ { on = 0 } on' "$src/Cargo.toml")"
[ -n "$lints" ] || { echo "FAIL: Cargo.toml has no [lints] table to copy"; exit 1; }
msrv="$(grep -m1 '^rust-version' "$src/Cargo.toml")"
cat >Cargo.toml <<TOML
[package]
name = "hook-fixture"
version = "0.1.0"
edition = "2024"
$msrv
publish = false

$lints
TOML
good_lib='//! A stand-in crate.

/// Adds one.
pub fn add_one(x: u32) -> u32 {
    x + 1
}

#[cfg(test)]
mod tests {
    #[test]
    fn adds_one() {
        assert_eq!(super::add_one(1), 2, "one more than one");
    }
}'
printf '%s\n' "$good_lib" >src/lib.rs
cat >fuzz/Cargo.toml <<TOML
[package]
name = "hook-fixture-fuzz"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

$lints
TOML
printf '%s\n' '//! A stand-in fuzz package.' '' 'fn main() {}' >fuzz/src/main.rs
cat >client/package.json <<'JSON'
{
  "name": "hook-fixture-client",
  "version": "0.0.0",
  "private": true,
  "scripts": {
    "compile": "node compile.js",
    "test": "node test.js"
  }
}
JSON
printf '%s\n' 'process.exit(0);' >client/compile.js
printf '%s\n' 'process.exit(0);' >client/test.js
printf '%s\n' '/target' '/fuzz/target' 'client/node_modules/' >.gitignore
cargo generate-lockfile -q
cargo generate-lockfile -q --manifest-path fuzz/Cargo.toml
git add -A
if ! git commit -q -m "Start the fixture" >"$work/out" 2>&1; then
  echo "FAIL: the clean first commit was refused"
  sed 's/^/    /' "$work/out"
  exit 1
fi

pass=0
fail=0
n=0
# result <accept|refuse> <what> <got> [text a refusal must print]
# Every refusal must come from a hook, which says "refused", and not from
# something else failing on the way: a push the remote rejects, say.
result() {
  local needle="${4:-refused}"
  n=$((n + 1))
  if [ "$3" = "$1" ] && { [ "$1" = accept ] || grep -qF -- "$needle" "$work/out"; }; then
    pass=$((pass + 1))
  elif [ "$3" = "$1" ]; then
    fail=$((fail + 1))
    echo "FAIL (refused, but without \"$needle\"): $2"
    sed 's/^/    /' "$work/out" | tail -8
  else
    fail=$((fail + 1))
    echo "FAIL ($1 expected, got $3): $2"
    sed 's/^/    /' "$work/out" | tail -8
  fi
}
reset() {
  git reset -q --hard "${1:-HEAD}"
  git clean -qfdx -e target -e client/node_modules
}

# commit <accept|refuse> <what> <file> <content> [message] [text a refusal must print]
commit() {
  local want="$1" what="$2" file="$3" content="$4" msg="${5:-Change a file}" needle="${6:-refused}" got
  mkdir -p "$(dirname "$file")"
  printf '%s\n' "$content" >"$file"
  git add -f -- "$file"
  if git commit -q -m "$msg" >"$work/out" 2>&1; then got=accept; else got=refuse; fi
  result "$want" "$what" "$got" "$needle"
  reset
}

c=cl; a=an
tool="${c}aude"
vendor="${a}thropic"
helper="Copi""lot"
robot="$(printf '\xf0\x9f\xa4\x96')"

# ── commit-msg and the staged-text scan ──────────────────────────────
commit accept "clean change" notes.txt "plain words"
commit refuse "AI tool named in a file" notes.txt "written with ${tool^}"
commit refuse "AI vendor named in a file" notes.txt "see ${vendor}.com"
commit refuse "robot emoji in a file" notes.txt "done $robot"
commit accept "agent file ignored in .gitignore" .gitignore "/target
/fuzz/target
client/node_modules/
${tool^^}.md"
commit refuse "AI tool named elsewhere in .gitignore" .gitignore "# for ${tool^}"
commit refuse "AI trailer in the message" notes.txt "message one" "Fix it

Co-Authored-By: ${tool^} Opus <noreply@${vendor}.com>"
commit refuse "other AI tool's trailer in the message" notes.txt "message two" "Fix it

Co-authored-by: ${helper} <${helper,,}@example.com>"
commit refuse "generated-with footer in the message" notes.txt "message three" "Fix it

Generated with ${tool^} Code"
commit refuse "robot emoji in the message" notes.txt "message four" "Fix it $robot"
commit accept "human co-author in the message" notes.txt "message five" "Fix it

Co-authored-by: Test User <test@example.com>"
# git commit -m keeps a line that starts with #, so the hook reads it too.
commit refuse "AI tool named on a #-line of the message" notes.txt "message six" "Fix it
# ${tool^} helped"

# ── what pre-commit builds and runs ──────────────────────────────────
commit refuse "unformatted code" src/lib.rs "pub fn add_one(x:u32)->u32{x+1}" "Change a file" "Diff in"
commit refuse "upper-case hex literal (unstable rustfmt option)" src/lib.rs "//! A stand-in crate.

/// A mask.
pub const MASK: u32 = 0xFF;" "Change a file" "+pub const MASK: u32 = 0xff;"
commit refuse "imports not one module per use (unstable rustfmt option)" src/lib.rs "//! A stand-in crate.

use std::{collections::BTreeMap, fmt::Write};

/// A map.
pub fn map() -> BTreeMap<u8, String> {
    let mut s = String::new();
    let _ = write!(s, \"x\");
    BTreeMap::new()
}" "Change a file" "+use std::fmt::Write;"
commit refuse "clippy warning" src/lib.rs "//! A stand-in crate.

/// Adds one.
pub fn add_one(x: u32) -> u32 {
    return x + 1;
}"
# unsafe_code is allowed unless [lints] forbids it, so this proves the
# stand-in crate is linted with the real tables.
commit refuse "a lint from [lints]: unsafe_code is forbidden" src/lib.rs "//! A stand-in crate.

/// The letter a.
pub fn a() -> &'static str {
    // SAFETY: b\"a\" is one ASCII byte, which is valid UTF-8.
    unsafe { std::str::from_utf8_unchecked(b\"a\") }
}" "Change a file" "usage of an \`unsafe\` block"
commit refuse "clippy warning in the fuzz package" fuzz/src/main.rs "//! A stand-in fuzz package.

fn main() {
    let x = 1;
    return assert_eq!(x, 1, \"one\");
}"
commit refuse "failing test" src/lib.rs "${good_lib/x + 1/x + 2}"
commit accept "formatted, lint-clean, passing" src/lib.rs "${good_lib/Adds one/Adds one to x}"

# A change left out of the index would be committed unchecked, or checked
# without being committed.
printf '%s\n' "${good_lib/Adds one/Adds one, unstaged}" >src/lib.rs
printf '%s\n' "words" >notes.txt
git add notes.txt
if git commit -q -m "Change a file" >"$work/out" 2>&1; then got=accept; else got=refuse; fi
result refuse "unstaged change to a tracked file" "$got"
reset
printf '%s\n' "fn stray() {}" >src/stray.rs
printf '%s\n' "words" >notes.txt
git add notes.txt
if git commit -q -m "Change a file" >"$work/out" 2>&1; then got=accept; else got=refuse; fi
result refuse "untracked file" "$got"
reset

# ── pre-push ─────────────────────────────────────────────────────────
base="$(git rev-parse HEAD)"
git push -q origin main >"$work/out" 2>&1 && got=accept || got=refuse
result accept "first push of good commits" "$got"

# push <accept|refuse> <what> [text a refusal must print]: pushes HEAD to a
# branch of its own, so no case depends on what an earlier one pushed.
push() {
  if git push -q origin "HEAD:refs/heads/case-$n" >"$work/out" 2>&1; then got=accept; else got=refuse; fi
  result "$1" "$2" "$got" "${3:-refused}"
  reset "$base"
}
quick() { git -c core.hooksPath=/dev/null commit -q "$@" >/dev/null 2>&1; }

echo a >notes.txt && git add notes.txt && quick -m "Add a note"
push accept "signed, own identity"

echo b >notes.txt && git add notes.txt
git -c core.hooksPath=/dev/null -c commit.gpgsign=false commit -q -m "Add a note" >/dev/null 2>&1
push refuse "unsigned commit"

echo c >notes.txt && git add notes.txt && quick -S"$work/stranger.pub" -m "Add a note"
push refuse "signed by a key that is not an allowed signer"

echo d >notes.txt && git add notes.txt
GIT_AUTHOR_NAME=Other GIT_AUTHOR_EMAIL=other@example.com quick -m "Add a note"
push refuse "author is not the configured identity"

echo e >notes.txt && git add notes.txt
GIT_COMMITTER_NAME=Other GIT_COMMITTER_EMAIL=other@example.com quick -m "Add a note"
push refuse "committer is not the configured identity"

echo f >notes.txt && git add notes.txt
quick -m "Add a note

Co-Authored-By: ${tool^} <noreply@${vendor}.com>"
push refuse "AI trailer in an outgoing message (committed past commit-msg)"

echo g >notes.txt && git add notes.txt && quick -m "Add a note"
echo h >notes.txt && git add notes.txt && quick -m "Add another note"
if git push -q origin "HEAD~1:refs/heads/case-$n" >"$work/out" 2>&1; then got=accept; else got=refuse; fi
result refuse "pushing a commit that is not the checked-out one" "$got"
reset "$base"

printf '%s\n' "${good_lib/Adds one./Adds one, see [\`missing\`].}" >src/lib.rs
git add src/lib.rs && quick -m "Link a missing item"
push refuse "broken rustdoc link"

printf '%s\n' 'process.exit(1);' >client/test.js
git add client/test.js && quick -m "Break the client test"
push refuse "failing client test"

rm -rf client/node_modules
echo i >notes.txt && git add notes.txt && quick -m "Add a note"
push refuse "client dependencies not installed"
mkdir -p client/node_modules

# Deleting a remote branch sends no commits and needs no checks.
git push -q origin "$base:refs/heads/topic" >"$work/out" 2>&1 || { echo "FAIL: could not push the branch to delete"; cat "$work/out"; exit 1; }
if git push -q origin :topic >"$work/out" 2>&1; then got=accept; else got=refuse; fi
result accept "deleting a remote branch" "$got"

echo "test-hooks: $pass of $n passed"
[ "$fail" -eq 0 ]
