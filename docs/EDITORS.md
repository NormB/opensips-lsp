# Editor and tool setup

The server speaks LSP 3.17 over stdio and knows nothing about any
particular editor, so **any LSP client can drive it**. The sections
below are worked examples for the clients people actually use; if
yours is not here, the shape is always the same — run `opensips-lsp`,
speak LSP on its stdin/stdout, and pass the settings either as
`initializationOptions` or as environment variables.

Install the server first — either grab a prebuilt binary from the
[releases page](https://github.com/NormB/opensips-lsp/releases)
(Linux/macOS tarballs and Windows zips, x86_64 and aarch64/arm64), or
`cargo build --release` and put `target/release/opensips-lsp` on PATH.

**You do not need a source tree to get started.** The core language
and all documented modules are built in, harvested from OpenSIPS
4.0.1, so completion and hover work immediately. Set `opensipsSrc`
when you want documentation exact to your own build — it replaces the
built-in catalogues wholesale. Every setting is optional; the full
list is in [`docs/FEATURES.md`](FEATURES.md).

## Which files the server should be given

The VS Code extension claims `opensips.cfg`, `opensips*.cfg` (so
`opensips-proxy.cfg` works) and `*.opensips.cfg`. It deliberately does
NOT claim every `.cfg` on disk — that would hijack unrelated files.
Configure your client to match the same set rather than a bare `.cfg`
glob.

### Files your configuration includes

An `include_file`/`import_file` target is usually named something no
pattern above would catch. The server handles this on its own once it
sees the file: a fragment is analysed in the context of the root that
includes it, `opensips -C` is run on that root, and navigation spans
the root's closure — so nothing here needs configuring for the
answers to be right.

What the pattern decides is whether your client hands the file to the
server at all. The VS Code extension asks the server and sets the
language itself; every other client needs the fragment matched by
whatever glob you configure, or opened through a link from the root.
Clients that want to do the same thing can send the non-LSP request
`opensips/analysisRoot`. It answers with the root's URI, or `null`
when the file is a program in its own right — or when nothing in the
workspace includes it:

```json
--> {"jsonrpc": "2.0", "id": 7, "method": "opensips/analysisRoot",
     "params": {"uri": "file:///etc/opensips/routing/inbound.cfg"}}
<-- {"jsonrpc": "2.0", "id": 7, "result": "file:///etc/opensips/opensips.cfg"}

--> {"jsonrpc": "2.0", "id": 8, "method": "opensips/analysisRoot",
     "params": {"uri": "file:///etc/opensips/opensips.cfg"}}
<-- {"jsonrpc": "2.0", "id": 8, "result": null}
```

A non-`null` answer means "this is part of an OpenSIPS
configuration", which is enough to decide whether to hand the file to
the server.

**Send `workspaceFolders` in `initialize`.** The include graph is
built from the configs under those folders; a client that passes none
gets `null` for everything and every fragment is analysed as a
program of its own.

## VS Code / VSCodium

**Novice?** Use the [Getting Started guide](GETTING_STARTED.md)
instead — one-command install and full usage walkthrough. The notes
below are for building the extension from source.

```sh
cd client && npm install && npm run compile
npx @vscode/vsce package        # produces opensips-lsp-ext-<version>.vsix
code --install-extension opensips-lsp-ext-*.vsix
```

Settings live under the `opensipsLsp.` prefix — `opensipsLsp.serverPath`,
`opensipsLsp.opensipsPath`, `opensipsLsp.opensipsSrc`,
`opensipsLsp.checkTimeoutMs`, and the rest listed in
[`docs/FEATURES.md`](FEATURES.md).

## Neovim (0.10+, built-in LSP)

```lua
vim.filetype.add({
  filename = { ["opensips.cfg"] = "opensips-cfg" },
  pattern = {
    ["opensips.*%.cfg"] = "opensips-cfg",
    [".*%.opensips%.cfg"] = "opensips-cfg",
  },
})
vim.api.nvim_create_autocmd("FileType", {
  pattern = "opensips-cfg",
  callback = function()
    vim.lsp.start({
      name = "opensips-lsp",
      cmd = { "opensips-lsp" },
      root_dir = vim.fs.dirname(vim.api.nvim_buf_get_name(0)),
      -- every option is optional; omit the table for built-in docs
      init_options = {
        opensipsPath = "/usr/local/sbin/opensips",
        opensipsSrc = "/path/to/opensips",
        checkTimeoutMs = 10000,
      },
    })
  end,
})
```

## coc.nvim

`:CocConfig`:

```json
{
  "languageserver": {
    "opensips-lsp": {
      "command": "opensips-lsp",
      "filetypes": ["opensips-cfg"],
      "initializationOptions": {
        "opensipsPath": "/usr/local/sbin/opensips"
      }
    }
  }
}
```

## Helix

`~/.config/helix/languages.toml`:

```toml
[language-server.opensips-lsp]
command = "opensips-lsp"

[language-server.opensips-lsp.config]
opensipsPath = "/usr/local/sbin/opensips"

[[language]]
name = "opensips-cfg"
scope = "source.opensips"
file-types = [
  { glob = "opensips.cfg" },
  { glob = "opensips*.cfg" },
  { glob = "*.opensips.cfg" },
]
comment-token = "#"
language-servers = ["opensips-lsp"]
```

Helix can also use the tree-sitter grammar in `tree-sitter-opensips/`
for highlighting and folding; see the repository README.

## Emacs (eglot, built-in since 29)

```elisp
(define-derived-mode opensips-cfg-mode prog-mode "OpenSIPS-cfg"
  (setq-local comment-start "# "))
(add-to-list 'auto-mode-alist '("opensips[^/]*\\.cfg\\'" . opensips-cfg-mode))
(add-to-list 'auto-mode-alist '("\\.opensips\\.cfg\\'" . opensips-cfg-mode))
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               `(opensips-cfg-mode
                 . ("opensips-lsp"
                    :initializationOptions
                    (:opensipsPath "/usr/local/sbin/opensips"
                     :checkTimeoutMs 10000)))))
```

## Vim (prabirshrestha/vim-lsp)

```vim
if executable('opensips-lsp')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'opensips-lsp',
    \ 'cmd': {server_info->['opensips-lsp']},
    \ 'initialization_options': {
    \   'opensipsPath': '/usr/local/sbin/opensips'},
    \ 'allowlist': ['opensips-cfg'],
    \ })
endif
au BufRead,BufNewFile opensips.cfg,opensips*.cfg,*.opensips.cfg setfiletype opensips-cfg
```

## Sublime Text (LSP package)

`Preferences → Package Settings → LSP → Settings`:

```json
{
  "clients": {
    "opensips-lsp": {
      "enabled": true,
      "command": ["opensips-lsp"],
      "selector": "source.opensips",
      "initializationOptions": {
        "opensipsPath": "/usr/local/sbin/opensips"
      }
    }
  }
}
```

## Kate

`Settings → Configure Kate → LSP Client → User Server Settings`:

```json
{
  "servers": {
    "opensips-cfg": {
      "command": ["opensips-lsp"],
      "highlightingModeRegex": "^INI Files$",
      "initializationOptions": {
        "opensipsPath": "/usr/local/sbin/opensips"
      }
    }
  }
}
```

## JetBrains IDEs (IntelliJ, PyCharm, …)

JetBrains IDEs do not read a config file for third-party language
servers; install the **LSP4IJ** plugin, then *Settings → Languages &
Frameworks → Language Servers → +* and fill in:

- **Command:** `opensips-lsp`
- **Mappings → File name patterns:** `opensips.cfg`, `opensips*.cfg`,
  `*.opensips.cfg`
- **Configuration:** the same JSON object the other clients pass as
  `initializationOptions`

## Zed

Zed cannot be pointed at a language server from `settings.json` alone:
only "language server, context server and debugger extensions require
the presence of custom Rust", so registering this one means a small
extension compiled to WebAssembly. It is four files.

`extension.toml`:

```toml
id = "opensips"
name = "OpenSIPS"
version = "0.0.1"
schema_version = 1
authors = ["you <you@example.com>"]
description = "OpenSIPS configuration language and LSP"
repository = "https://github.com/you/zed-opensips"

[grammars.opensips]
repository = "https://github.com/NormB/opensips-lsp"
rev = "<commit sha>"

[language_servers.opensips-lsp]
name = "opensips-lsp"
languages = ["OpenSIPS"]
```

`languages/opensips-cfg/config.toml`:

```toml
name = "OpenSIPS"
grammar = "opensips"
path_suffixes = ["opensips.cfg"]
line_comments = ["# "]
```

`Cargo.toml`:

```toml
[package]
name = "zed-opensips"
version = "0.0.1"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
zed_extension_api = "0.7"
```

`src/lib.rs`:

```rust
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
```

The step-by-step version of all of this, including getting the grammar
into a repository of its own and what to do when it does not work, is
[`docs/ZED.md`](ZED.md).

Then open the extensions page, run **`zed: install dev extension`**,
and pick the directory. Zed builds it; `zed: open log` or
`zed --foreground` shows what happened if it does not appear.

Two caveats worth knowing before you start:

- **`path_suffixes` are suffixes, not globs.** `"opensips.cfg"` matches
  any path ENDING in that, so `opensips.cfg` and `db.opensips.cfg` are
  covered but `opensips-proxy.cfg` is not. Zed's `file_types` setting
  does take globs, so a user can widen it in `settings.json`:

  ```json
  "file_types": { "OpenSIPS": ["opensips*.cfg", "**/*.opensips.cfg"] }
  ```

- **`[grammars]` clones a repository and expects the grammar at its
  root**, and this project keeps its grammar in the
  `tree-sitter-opensips/` subdirectory. Publishing that directory as
  its own repository is the fix for a shareable extension; for local
  development a `file://` URL in `repository` works.

The Rust above is compiled and checked against `zed_extension_api`
0.7 (`cargo build --release --target wasm32-wasip1`) — but no part of
this has been exercised inside a running Zed, so treat the end-to-end
result as unverified.

## Any other LSP client

The server reads LSP on stdin and writes it on stdout. Nothing else is
required: no port, no daemon, no configuration file. Clients that
cannot pass `initializationOptions` can use the environment instead:

```sh
export OPENSIPS_LSP_BIN=/usr/local/sbin/opensips
export OPENSIPS_LSP_SRC=/path/to/opensips
export OPENSIPS_LSP_CHECK_TIMEOUT_MS=10000
```

## Without an editor: CI, hooks, and scripts

`opensips-lsp check` runs the same analysis in batch and prints
`file:line:col: severity: message`, which most tooling already parses:

```console
$ opensips-lsp check /etc/opensips/opensips.cfg
/etc/opensips/opensips.cfg:2:11: warning: route 'MISSING' is not defined here or in included files
```

Exit codes are 0 clean, 1 problems found (errors, or warnings under
`--strict`), 2 usage or read failure — so it drops straight into a
gate. Usage is
`opensips-lsp check [--strict] [--bin <opensips>] <file>...`.

GitHub Actions:

```yaml
- name: Check OpenSIPS configs
  run: opensips-lsp check --strict $(git ls-files 'opensips*.cfg' '*.opensips.cfg')
```

A pre-commit hook:

```sh
#!/bin/sh
files=$(git diff --cached --name-only --diff-filter=ACM | grep -E '(^|/)opensips[^/]*\.cfg$|\.opensips\.cfg$')
[ -z "$files" ] && exit 0
opensips-lsp check --strict $files
```

Passing `--bin` (or `OPENSIPS_LSP_BIN`) additionally runs the real
OpenSIPS parser over each file, so CI catches what only the binary
knows. Without it the check is static analysis alone.
