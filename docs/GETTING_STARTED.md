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

This all works before you configure anything. The extension ships
documentation harvested from OpenSIPS 4.0.1 — the core language
(`log_level` and the other globals, core functions, pseudo-variables)
and all 186 documented modules with their functions and parameters —
and hover tells you which version an entry came from.

Set **Opensips Lsp: Opensips Src** (in the same Settings page) to a
folder containing the OpenSIPS source code matching your version when
you want documentation exact to your own build rather than to 4.0.1.
A configured folder replaces the built-in catalogues entirely, which
is the point: mixing two versions is worse than either.

### Reading and moving around

- **Hover** the mouse over any function, parameter, or `$variable`
  to read what it does.
- **Ctrl+Click** on a route name inside `route(name)` to jump to
  where that route is defined.
- Press **Ctrl+Shift+O** to see every route in the file and jump
  between them.

### Split configurations (`include_file`)

Most real deployments split the config up:

```
/etc/opensips/
├── opensips.cfg          <- the root: everything starts here
├── modules.cfg           <- include_file "modules.cfg"
└── routing/
    ├── inbound.cfg       <- include_file "routing/inbound.cfg"
    └── carriers.cfg      <- include_file "routing/carriers.cfg"
```

with `opensips.cfg` pulling the rest in:

```
include_file "modules.cfg"
include_file "routing/inbound.cfg"
include_file "routing/carriers.cfg"

route {
    route(inbound);
}
```

**What you have to do: open the FOLDER, not the single file.**
`File → Open Folder…` and pick `/etc/opensips` (or wherever your
config lives). The extension finds the root by reading the configs in
the folder you opened; with a single file open there is nothing to
read, and every fragment is treated as a program of its own.

Then open `routing/inbound.cfg` — a name no pattern could recognize
— and it behaves like part of the whole:

- It gets **syntax colors**, even though nothing about the filename
  says "opensips". The extension asks the server whether anything in
  the folder includes it, and sets the language when something does.
- A `route(send_to_carrier)` defined over in `carriers.cfg`
  **Ctrl+Clicks through** and is **offered while you type**.
- It is **not** flagged for using routes it does not define. Before
  0.20.0 every one of those was underlined as undefined — an artefact
  of opening the file, not a problem with it.
- Error checking runs `opensips -C` on `opensips.cfg` (the only file
  that *is* a program) and puts each error on the file it belongs to.
  A mistake on line 12 of `inbound.cfg` is underlined on line 12 of
  `inbound.cfg`.

Nothing above needs configuring. Two things you may want to change:

**To turn the automatic coloring off** — `Ctrl+,`, search
`opensips`, and untick **Opensips Lsp › Associate Included Files**.
Or in `settings.json`:

```json
{ "opensipsLsp.associateIncludedFiles": false }
```

**If a file is still plain text**, the includes do not reach it — it
is not included by anything in the folder you opened, or another
extension already claimed `.cfg` and this one leaves those alone. Tell
VS Code directly, in `settings.json` (`Ctrl+Shift+P` → *Preferences:
Open User Settings (JSON)*):

```json
{
  "files.associations": {
    "routing/*.cfg": "opensips-cfg"
  }
}
```

To do it for the open file only, click the language name in the
bottom-right status bar (it will say **Plain Text**) and pick
**OpenSIPS config**.

## When something doesn't work

| Symptom | Fix |
|---|---|
| No colors | The file has to match one of the claimed names: `opensips.cfg`, `opensips*.cfg` (so `opensips-proxy.cfg` works), or `*.opensips.cfg`. A plain `.cfg` is not enough — the extension deliberately does not claim every `.cfg` on your disk. |
| No colors on an included file | Open the FOLDER (`File → Open Folder…`), not the single file — the root that includes it has to be somewhere the server can read. If you just added the `include_file` line, save the root and reopen the fragment. |
| An included file reports routes its parent defines as undefined | Same cause: with no folder open the fragment is treated as a program of its own. Open the folder containing the root. |
| No red squiggles | Set **Opensips Path** (step above), save the file, and make sure you trusted the folder. |
| Squiggles on a correct file | The checker uses *your* OpenSIPS version — a config written for another version can legitimately fail. |
| Completion has no documentation | Core and module entries both carry built-in documentation from 4.0.1, so an entry with none is one 4.0.1 does not document: set **Opensips Src** to an OpenSIPS source folder matching your build. |
| A module I have is not offered | The built-in list is what 4.0.1 documents, not what you compiled. Set **Opensips Src** to your own tree. |
| Still stuck | **View → Output**, pick **OpenSIPS LSP** in the dropdown — the server explains what it is doing (e.g. "ready (186 documented modules, 66 core functions, core and module docs built in from 4.0.1)"). |
