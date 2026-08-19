# Getting Started

This guide assumes no prior experience — just VS Code installed and
an `opensips.cfg` file you want to edit. Two pieces work together:
a small **server program** that does the analysis, and a **VS Code
extension** that talks to it. The installer sets up both.

## Install

### Option A — one command (recommended)

Open a terminal (in VS Code: **Terminal → New Terminal**), paste this
line, and press Enter:

```sh
curl -fsSL https://raw.githubusercontent.com/NormB/opensips-lsp/main/install.sh | sh
```

That's it. The script downloads the right build for your machine,
installs the server to `~/.local/bin`, and adds the extension to VS
Code. It prints what it did; if something is missing (for example the
`code` command), it prints exactly what to do instead.

### Option B — by hand, step by step

1. Open <https://github.com/NormB/opensips-lsp/releases/latest> in a
   browser.
2. Download two files from the **Assets** list:
   - `opensips-lsp-…-x86_64-linux-gnu.tar.gz` (or `aarch64` on ARM)
   - `opensips-cfg-….vsix`
3. Install the server — in a terminal:

   ```sh
   tar xzf opensips-lsp-*-linux-gnu.tar.gz
   mkdir -p ~/.local/bin
   install -m755 opensips-lsp ~/.local/bin/
   ```

4. Install the extension — in VS Code:
   1. Press **Ctrl+Shift+X** (the Extensions panel opens).
   2. Click the **⋯** button in the panel's top-right corner.
   3. Choose **Install from VSIX…**
   4. Pick the `opensips-cfg-….vsix` file you downloaded.

## First use

Open a folder containing an `opensips.cfg` (**File → Open Folder…**)
and click the file. You should immediately see **syntax colors**.
If VS Code asks *"Do you trust the authors of the files in this
folder?"* — answer honestly: in an untrusted folder the extension
still colors, completes, and navigates, but it will not run the
OpenSIPS checker on the file (that is a safety feature, because
checking a config executes parts of it).

### See your mistakes as you type (diagnostics)

This needs OpenSIPS itself installed on the same machine.

1. Press **Ctrl+,** (Settings), type `opensips` in the search box.
2. In **Opensips Lsp: Opensips Path** enter the full path of your
   `opensips` binary, e.g. `/usr/local/sbin/opensips`.
3. Open your `opensips.cfg` and save it (**Ctrl+S**).

Mistakes now get **red squiggles** at the exact spot — hover one to
read the message (it is the real OpenSIPS parser talking, e.g.
`Parameter <fr_timeot> not found in module <tm>`). Squiggles refresh
every time you save.

### Autocomplete

- Type `loadmodule "` — a list of every module appears. Keep typing
  to filter, press **Enter** to accept.
- Type `modparam("tm", "` — the list shows only tm's parameters,
  each with its documentation.
- Inside a route, type the first letters of a function
  (`t_re…` → `t_relay`) — functions of the modules you loaded, plus
  core functions, appear with their signatures.
- Type `$` — pseudo-variables (`$ru`, `$si`, …) with descriptions.
- If a list ever disappears, press **Ctrl+Space** to bring it back.

For the richest documentation in these popups, also set
**Opensips Lsp: Opensips Src** (in the same Settings page) to a
folder containing the OpenSIPS source code matching your version.

### Reading and moving around

- **Hover** the mouse over any function, parameter, or `$variable`
  to read what it does.
- **Ctrl+Click** on a route name inside `route(name)` to jump to
  where that route is defined.
- Press **Ctrl+Shift+O** to see every route in the file and jump
  between them.

## When something doesn't work

| Symptom | Fix |
|---|---|
| No colors | The file must be named `opensips.cfg` or end in `.cfg`. |
| No red squiggles | Set **Opensips Path** (step above), save the file, and make sure you trusted the folder. |
| Squiggles on a correct file | The checker uses *your* OpenSIPS version — a config written for another version can legitimately fail. |
| Completion has no documentation | Set **Opensips Src** to an OpenSIPS source folder. |
| Still stuck | **View → Output**, pick **OpenSIPS LSP** in the dropdown — the server explains what it is doing (e.g. "ready (193 documented modules)"). |
