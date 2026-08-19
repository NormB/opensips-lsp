#!/usr/bin/env python3
"""Generate the GitHub Wiki tree from the in-repo docs.

docs/ + README.md are the single source of truth; the wiki is a
generated mirror (same model as sipnab).  Mapping:

    README.md        -> Home.md
    docs/ADMIN.md    -> Admin-Guide.md
    docs/EDITORS.md  -> Editor-Setup.md

Transformations: strip the leading H1 (the wiki renders the page
title), rewrite inter-doc links to wiki page names, emit _Sidebar.

Run from the repo root:  python3 scripts/build-wiki.py [OUTPUT_DIR]
"""
import re
import sys
from pathlib import Path

PAGES = {
    "README.md": "Home",
    "docs/ADMIN.md": "Admin-Guide",
    "docs/EDITORS.md": "Editor-Setup",
}

# every spelling a source doc may use for another doc
LINK_REWRITES = {}
for src, page in PAGES.items():
    LINK_REWRITES[src] = page
    LINK_REWRITES[Path(src).name] = page


def rewrite_links(text: str) -> str:
    def sub(m):
        label, target = m.group(1), m.group(2)
        page = LINK_REWRITES.get(target) or LINK_REWRITES.get(target.lstrip("./"))
        if page:
            # a label that is just the file path (optionally in a code
            # span) reads wrong on the wiki: use the page name instead
            if label.strip("`") in (target, Path(target).name):
                return f"[[{page}]]"
            return f"[[{label}|{page}]]"
        return m.group(0)

    return re.sub(r"\[([^\]]+)\]\(([^)#]+\.md)\)", sub, text)


def strip_h1(text: str) -> str:
    lines = text.splitlines()
    if lines and lines[0].startswith("# "):
        lines = lines[1:]
        while lines and not lines[0].strip():
            lines = lines[1:]
    return "\n".join(lines) + "\n"


def main() -> int:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "build/wiki")
    out.mkdir(parents=True, exist_ok=True)
    for src, page in PAGES.items():
        text = Path(src).read_text(encoding="utf-8")
        (out / f"{page}.md").write_text(
            strip_h1(rewrite_links(text)), encoding="utf-8"
        )
    sidebar = "\n".join(
        ["### opensips-lsp", "", "* [[Home]]"]
        + [f"* [[{p}]]" for s, p in PAGES.items() if p != "Home"]
    )
    (out / "_Sidebar.md").write_text(sidebar + "\n", encoding="utf-8")
    (out / "_Footer.md").write_text(
        "Generated from the repository docs — edit `docs/` in the repo, "
        "not the wiki.\n",
        encoding="utf-8",
    )
    print(f"wiki tree written to {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
