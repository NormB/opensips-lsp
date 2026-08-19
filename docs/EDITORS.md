# Editor setup

Install the server first: `cargo build --release` and put
`target/release/opensips-lsp` on PATH (or note its absolute path).
All examples pass the same three `initializationOptions` documented
in `docs/ADMIN.md`.

## VS Code

Use the bundled extension in `client/`:

```sh
cd client && npm install && npm run compile
npx @vscode/vsce package        # produces opensips-cfg-<version>.vsix
code --install-extension opensips-cfg-*.vsix
```

Settings: `opensipsLsp.serverPath`, `opensipsLsp.opensipsPath`,
`opensipsLsp.opensipsSrc`, `opensipsLsp.checkTimeoutMs`.

## Neovim (0.10+, built-in LSP)

```lua
vim.filetype.add({ filename = { ["opensips.cfg"] = "opensips-cfg" } })
vim.api.nvim_create_autocmd("FileType", {
  pattern = "opensips-cfg",
  callback = function()
    vim.lsp.start({
      name = "opensips-lsp",
      cmd = { "opensips-lsp" },
      init_options = {
        opensipsPath = "/usr/local/sbin/opensips",
        opensipsSrc = "/path/to/opensips",
        checkTimeoutMs = 10000,
      },
    })
  end,
})
```

## Helix

`~/.config/helix/languages.toml`:

```toml
[language-server.opensips-lsp]
command = "opensips-lsp"

[language-server.opensips-lsp.config]
opensipsPath = "/usr/local/sbin/opensips"
opensipsSrc = "/path/to/opensips"

[[language]]
name = "opensips-cfg"
scope = "source.opensips"
file-types = [{ glob = "opensips.cfg" }, "cfg"]
comment-token = "#"
language-servers = ["opensips-lsp"]
```

## Emacs (eglot, built-in since 29)

```elisp
(define-derived-mode opensips-cfg-mode prog-mode "OpenSIPS-cfg"
  (setq-local comment-start "# "))
(add-to-list 'auto-mode-alist '("opensips\\.cfg\\'" . opensips-cfg-mode))
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(opensips-cfg-mode . ("opensips-lsp")))
  (setq-default eglot-workspace-configuration '())
  ;; initializationOptions:
  (add-to-list 'eglot-server-programs
               `(opensips-cfg-mode
                 . ("opensips-lsp"
                    :initializationOptions
                    (:opensipsPath "/usr/local/sbin/opensips"
                     :opensipsSrc "/path/to/opensips")))))
```

## Vim (prabirshrestha/vim-lsp)

```vim
if executable('opensips-lsp')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'opensips-lsp',
    \ 'cmd': {server_info->['opensips-lsp']},
    \ 'initialization_options': {
    \   'opensipsPath': '/usr/local/sbin/opensips',
    \   'opensipsSrc': '/path/to/opensips'},
    \ 'allowlist': ['opensips-cfg'],
    \ })
endif
au BufRead,BufNewFile opensips.cfg setfiletype opensips-cfg
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
        "opensipsPath": "/usr/local/sbin/opensips",
        "opensipsSrc": "/path/to/opensips"
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
        "opensipsPath": "/usr/local/sbin/opensips",
        "opensipsSrc": "/path/to/opensips"
      }
    }
  }
}
```

## Environment-variable fallback (any client)

Clients that cannot pass `initializationOptions` can export:

```sh
export OPENSIPS_LSP_BIN=/usr/local/sbin/opensips
export OPENSIPS_LSP_SRC=/path/to/opensips
export OPENSIPS_LSP_CHECK_TIMEOUT_MS=10000
```
