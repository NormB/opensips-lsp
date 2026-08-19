#!/bin/sh
# opensips-lsp one-command installer.
#
#   curl -fsSL https://raw.githubusercontent.com/NormB/opensips-lsp/main/install.sh | sh
#
# Downloads the latest release for this machine, installs the server
# to ~/.local/bin/opensips-lsp, and — when the `code` command is
# available — installs the VS Code extension too. Overrides:
#   OPENSIPS_LSP_VERSION   release tag        (default: latest)
#   OPENSIPS_LSP_PREFIX    server install dir (default: ~/.local/bin)
set -eu

REPO="NormB/opensips-lsp"
PREFIX="${OPENSIPS_LSP_PREFIX:-$HOME/.local/bin}"

say()  { printf '%s\n' "$*"; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar  >/dev/null 2>&1 || fail "tar is required"

case "$(uname -s)" in
    Linux)  OS=linux-gnu ;;
    Darwin) OS=darwin ;;
    *) fail "no prebuilt binaries for $(uname -s); see README 'Build & test'" ;;
esac
case "$(uname -m)" in
    x86_64|amd64)   ARCH=x86_64 ;;
    aarch64|arm64)  ARCH=aarch64 ;;
    *) fail "no prebuilt binary for $(uname -m); see README 'Build & test'" ;;
esac

if [ -n "${OPENSIPS_LSP_VERSION:-}" ]; then
    TAG="$OPENSIPS_LSP_VERSION"
else
    TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | tr ',' '\n' | grep -m1 '"tag_name"' | cut -d'"' -f4)
    [ -n "$TAG" ] || fail "could not determine the latest release"
fi
say "Installing opensips-lsp $TAG for $ARCH ..."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

BASE="https://github.com/$REPO/releases/download/$TAG"
curl -fsSL -o "$TMP/server.tar.gz" \
    "$BASE/opensips-lsp-$TAG-$ARCH-$OS.tar.gz" \
    || fail "download failed: $BASE/opensips-lsp-$TAG-$ARCH-$OS.tar.gz"
tar -C "$TMP" -xzf "$TMP/server.tar.gz"
mkdir -p "$PREFIX"
install -m755 "$TMP/opensips-lsp" "$PREFIX/opensips-lsp"
say "Server installed: $PREFIX/opensips-lsp"

case ":$PATH:" in
    *":$PREFIX:"*) ;;
    *) say "NOTE: $PREFIX is not on your PATH — add it, e.g.:"
       say "      echo 'export PATH=\"$PREFIX:\$PATH\"' >> ~/.profile" ;;
esac

if command -v code >/dev/null 2>&1; then
    curl -fsSL -o "$TMP/ext.vsix" "$BASE/opensips-lsp-ext-$TAG.vsix" \
        || fail "download failed: $BASE/opensips-lsp-ext-$TAG.vsix"
    code --install-extension "$TMP/ext.vsix" --force >/dev/null
    say "VS Code extension installed."
    say
    say "Done. Open any opensips.cfg in VS Code and it just works."
    say "Optional settings (File > Preferences > Settings, search 'opensips'):"
    say "  - Opensips Path: your opensips binary (enables live error checking)"
    say "  - Opensips Src:  an OpenSIPS source tree (richer completion docs)"
else
    say
    say "VS Code's 'code' command was not found, so the extension was"
    say "not installed automatically. To add it by hand:"
    say "  1. Download: $BASE/opensips-lsp-ext-$TAG.vsix"
    say "  2. In VS Code press Ctrl+Shift+X, click the '...' menu"
    say "     (top-right of the Extensions panel), choose"
    say "     'Install from VSIX...' and pick the downloaded file."
fi
