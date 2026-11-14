#!/usr/bin/env python3
"""Regenerates crates/omacontrol-core/src/themes_gen.rs from Omarchy's themes.

Usage: python3 scripts/gen_themes.py /path/to/omarchy/themes
(clone https://github.com/omacom/omarchy first; Python 3.11+ for tomllib)
"""
import os
import sys
import tomllib

themes = sys.argv[1] if len(sys.argv) > 1 else "omarchy/themes"
out = os.path.join(os.path.dirname(__file__), "..", "crates", "omacontrol-core", "src", "themes_gen.rs")
lines = [
    "// Generated from omacom/omarchy themes/*/colors.toml (background, foreground, accent, mode).",
    "// Run `python3 scripts/gen_themes.py` to refresh.",
    "",
    "pub(crate) const BUILT_IN: &[(&str, &str, &str, &str, bool)] = &[",
]
for d in sorted(os.listdir(themes)):
    path = os.path.join(themes, d, "colors.toml")
    if not os.path.exists(path):
        continue
    c = tomllib.load(open(path, "rb"))
    name = " ".join(w.capitalize() for w in d.split("-"))
    dark = c.get("mode", "dark") == "dark"
    accent = c.get("accent", c.get("blue", "#7aa2f7"))
    lines.append(f'    ("{name}", "{c["background"]}", "{c["foreground"]}", "{accent}", {str(dark).lower()}),')
lines.append("];")
open(out, "w").write("\n".join(lines) + "\n")
print(f"wrote {out}")
