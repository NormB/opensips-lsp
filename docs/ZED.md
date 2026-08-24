# Zed setup

Zed cannot be pointed at a language server from its settings file. Its
own documentation is explicit: *"only language server, context server
and debugger extensions require the presence of custom Rust"*. So using
this server from Zed means building a small extension — about forty
lines, most of it copy-paste, and Zed compiles it for you.

This page is the whole procedure. If you just want the reference
version, [`docs/EDITORS.md`](EDITORS.md) has the four files without the
commentary.

## Before you start

You need three things:

| | |
|---|---|
| **Zed** | any recent release |
| **Rust** | via [rustup](https://rustup.rs) — Zed builds the extension with it |
| **`opensips-lsp` on your PATH** | from the [releases page](https://github.com/NormB/opensips-lsp/releases), or `cargo build --release` |

Check the last one before going further, because it is the failure
people hit at the end rather than the start:

```sh
opensips-lsp check --help
```

If that prints a usage line, you are ready.

## 1. Build the extension

Zed needs the tree-sitter grammar to live at the root of a repository,
and this project keeps it in a subdirectory, so the first thing the
script below does is give the grammar a repository of its own on your
machine. Paste the whole block:

```sh
set -e
ROOT="$HOME/.local/share/opensips-zed"
rm -rf "$ROOT" && mkdir -p "$ROOT"

# the grammar, in a repository of its own
git clone --depth 1 https://github.com/NormB/opensips-lsp.git "$ROOT/checkout"
cp -r "$ROOT/checkout/tree-sitter-opensips" "$ROOT/grammar"
git -C "$ROOT/grammar" init -q
git -C "$ROOT/grammar" add -A
git -C "$ROOT/grammar" -c user.email=you@example.com -c user.name=you \
    commit -qm "tree-sitter-opensips"
REV=$(git -C "$ROOT/grammar" rev-parse HEAD)

# the extension itself
EXT="$ROOT/extension"
mkdir -p "$EXT/src" "$EXT/languages/opensips-cfg"

cat > "$EXT/extension.toml" <<TOML
id = "opensips"
name = "OpenSIPS"
version = "0.0.1"
schema_version = 1
authors = ["you <you@example.com>"]
description = "OpenSIPS configuration language and LSP"
repository = "https://github.com/NormB/opensips-lsp"

[grammars.opensips]
repository = "file://$ROOT/grammar"
rev = "$REV"

[language_servers.opensips-lsp]
name = "opensips-lsp"
languages = ["OpenSIPS"]
TOML

cat > "$EXT/languages/opensips-cfg/config.toml" <<'TOML'
name = "OpenSIPS"
grammar = "opensips"
path_suffixes = ["opensips.cfg"]
line_comments = ["# "]
TOML

cat > "$EXT/Cargo.toml" <<'TOML'
[package]
name = "zed-opensips"
version = "0.0.1"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
zed_extension_api = "0.7"
TOML

cat > "$EXT/src/lib.rs" <<'RUST'
use zed_extension_api as zed;

struct OpensipsExtension;

impl zed::Extension for OpensipsExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let path = worktree
            .which("opensips-lsp")
            .ok_or_else(|| "opensips-lsp is not on $PATH".to_string())?;
        Ok(zed::Command {
            command: path,
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(OpensipsExtension);
RUST

echo "extension ready: $EXT"
```

## 2. Install it in Zed

1. Open the **Extensions** page.
2. Run **`zed: install dev extension`** from the command palette.
3. Choose the directory the script printed —
   `~/.local/share/opensips-zed/extension`.

Zed compiles it at this point. If you already have a published
OpenSIPS extension installed, Zed removes it first.

## 3. Check it worked

Open any file named `opensips.cfg` and type `log_`. You should get a
list including `log_level`, `log_prefix` and `log_stdout` — that
documentation is built into the server, so it appears without any
further configuration.

Hover `log_level` and the popup should say it is a core parameter, and
name the OpenSIPS version the documentation came from.

## 4. Optional settings

Everything below is optional. Put it in Zed's `settings.json`:

```json
{
  "file_types": {
    "OpenSIPS": ["opensips*.cfg", "**/*.opensips.cfg"]
  },
  "lsp": {
    "opensips-lsp": {
      "initialization_options": {
        "opensipsPath": "/usr/local/sbin/opensips",
        "opensipsSrc": "/path/to/opensips"
      }
    }
  }
}
```

- **`file_types`** widens which files count as OpenSIPS configs. The
  extension's own `path_suffixes` are suffixes rather than globs, so
  they match `opensips.cfg` and `db.opensips.cfg` but not
  `opensips-proxy.cfg`; this setting takes globs and covers the rest.
- **`opensipsPath`** turns on real diagnostics: the server runs your
  OpenSIPS binary with `-C` and reports what the parser says.
- **`opensipsSrc`** points at a source tree matching your build. The
  core language and every documented module are already built in from
  OpenSIPS 4.0.1; set this when you want documentation exact to your
  own version instead. It replaces the built-in catalogues rather than
  merging with them.

To run a server from somewhere other than `$PATH`:

```json
"lsp": { "opensips-lsp": { "binary": { "path": "/opt/bin/opensips-lsp" } } }
```

## When it does not work

| What you see | What it means |
|---|---|
| The extension fails to build | Rust is missing or too old. `rustup update`, then reinstall the dev extension. |
| No syntax colours, no completion | The file name does not match. Add the `file_types` block above, or rename to `opensips.cfg`. |
| Colours but no completion | The server was not found. `opensips-lsp` must be on the PATH Zed inherits — check with `zed: open log`, or set `lsp.opensips-lsp.binary.path`. |
| Nothing at all, no error | Run `zed --foreground` from a terminal; extension load failures appear there. |
| Completion works, no red squiggles | Diagnostics need a real binary: set `opensipsPath`. |

## What this page does not promise

Every build checks that the shell block above is valid shell, that the
Rust in it is byte-identical to the copy in
[`docs/EDITORS.md`](EDITORS.md), and that both pin the same
`zed_extension_api` version — so the two pages cannot drift apart and
the block cannot rot into something that will not parse. When this
page last changed, the block was also run as written and the extension
it produced was compiled for `wasm32-wasip1`.

Zed itself is not part of any of that. Nobody has verified the
end-to-end result inside a running Zed, so if something here is wrong,
please [open an issue](https://github.com/NormB/opensips-lsp/issues)
and it will be corrected rather than left to rot.
