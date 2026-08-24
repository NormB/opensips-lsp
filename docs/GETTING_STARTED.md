# Getting Started

This guide assumes no prior experience — just VS Code installed and
an `opensips.cfg` file you want to edit.

## Install

Works the same on **Linux, macOS, and Windows** — every release ships
native builds for all three (x86_64 and arm64), and the platform
extension packages bundle the server, so your editor picks the right
one automatically.

### Option A — from your editor's marketplace

**VSCodium / Cursor / Gitpod** (and other Open VSX editors): press
**Ctrl+Shift+X**, search for **opensips**, click **Install** on
"OpenSIPS Routing Script" — done; the platform builds bundle
everything.

**Standard VS Code** ships with Microsoft's marketplace, where this
extension is not distributed — use Option B (one command, installs
the extension for you) or Option C.

### Option B — one command in a terminal

> **Updates:** this route installs the extension from a downloaded
> file, and an editor never offers updates for a sideloaded VSIX — it
> carries no marketplace metadata. Re-run the script to update, or use
> Option A on an Open VSX editor to have updates arrive on their own.
> (If you are already on an old version this way, the Extensions view's
> **Install Specific Version…** will move you across.)


**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/NormB/opensips-lsp/main/install.ps1 | iex
```

**Linux / macOS:**

Open a terminal (in VS Code: **Terminal → New Terminal**), paste this
line, and press Enter:

```sh
curl -fsSL https://raw.githubusercontent.com/NormB/opensips-lsp/main/install.sh | sh
```

That's it. The script downloads the right build for your machine,
installs the server to `~/.local/bin`, and adds the extension to VS
Code. It prints what it did; if something is missing (for example the
`code` command), it prints exactly what to do instead.

### Option C — by hand, step by step

1. Open <https://github.com/NormB/opensips-lsp/releases/latest> in a
   browser.
2. Download two files from the **Assets** list:
   - `opensips-lsp-…-x86_64-linux-gnu.tar.gz` (or `aarch64` on ARM)
   - `opensips-lsp-ext-….vsix`
3. Install the server — Linux/macOS in a terminal (Windows: just
   unzip `opensips-lsp-…-windows.zip` anywhere, e.g.
   `%LOCALAPPDATA%\opensips-lsp`):

   ```sh
   tar xzf opensips-lsp-*-linux-gnu.tar.gz    # or *-darwin.tar.gz
   mkdir -p ~/.local/bin
   install -m755 opensips-lsp ~/.local/bin/
   ```

4. Install the extension — in VS Code:
   1. Press **Ctrl+Shift+X** (the Extensions panel opens).
   2. Click the **⋯** button in the panel's top-right corner.
   3. Choose **Install from VSIX…**
   4. Pick the `opensips-lsp-ext-….vsix` file you downloaded.

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
`Parameter <fr_timeot> not found in module <tm> - can't set`). Squiggles refresh
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

The core language — `log_level` and the other globals, core
functions, and pseudo-variables — is documented out of the box: the
extension ships a catalogue harvested from OpenSIPS 4.0.1, and hover
tells you so. Module documentation is a different matter, because
which modules exist depends on what you built: for that, and for core
docs exact to your own version, set **Opensips Lsp: Opensips Src** (in
the same Settings page) to a folder containing the OpenSIPS source
code matching your version.

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
| No colors | The file has to match one of the claimed names: `opensips.cfg`, `opensips*.cfg` (so `opensips-proxy.cfg` works), or `*.opensips.cfg`. A plain `.cfg` is not enough — the extension deliberately does not claim every `.cfg` on your disk. |
| No red squiggles | Set **Opensips Path** (step above), save the file, and make sure you trusted the folder. |
| Squiggles on a correct file | The checker uses *your* OpenSIPS version — a config written for another version can legitimately fail. |
| Completion has no documentation | Core functions, parameters and pseudo-variables carry built-in documentation, so this only applies to **module** entries: set **Opensips Src** to an OpenSIPS source folder to get those, and to replace the built-in core docs with ones exact to your build. |
| Still stuck | **View → Output**, pick **OpenSIPS LSP** in the dropdown — the server explains what it is doing (e.g. "ready (193 documented modules)"). |
