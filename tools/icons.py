#!/usr/bin/env python3
"""Lyra's symbolic icons, drawn from the mockups' own icon set (nyx-design's
mockups/polish/mock.js and mockups/lyra/lyra.js) so the app shows the same glyphs.

GTK fills every shape of a symbolic icon with the text colour, so the mockups' stroked
outlines are turned into filled shapes first (picosvg: `pip install picosvg`, e.g. in a
virtualenv). Writes apps/lyra/data/icons/lyra-<name>-symbolic.svg and apps/lyra/src/icons.rs
(the list the app installs). Usage: tools/icons.py [path to nyx-design]
"""
import pathlib
import re
import sys

from picosvg.svg import SVG

root = pathlib.Path(__file__).resolve().parent.parent
design = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else root.parent / "nyx-design")
sources = [
    (design / "mockups/polish/mock.js", "const I = {"),
    (design / "mockups/lyra/lyra.js", "const LI = {"),
]

icons = {}
for path, start in sources:
    text = path.read_text()
    i = text.index(start)
    j = text.index("};", i)
    for m in re.finditer(r"""^\s*"?([a-z0-9-]+)"?:\s*'([^']*)'""", text[i:j], re.M):
        icons[m.group(1)] = m.group(2)

out = root / "apps/lyra/data/icons"
for old in out.glob("lyra-*-symbolic.svg"):
    old.unlink()
names = []
for name, body in sorted(icons.items()):
    body = body.replace("currentColor", "#2e3436")
    drawn = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">'
        '<g fill="none" stroke="#2e3436" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">'
        f"{body}</g></svg>"
    )
    filled = SVG.fromstring(drawn).topicosvg().tostring(pretty_print=False)
    svg = filled.replace("<svg ", '<svg width="16" height="16" ', 1) + "\n"
    (out / f"lyra-{name}-symbolic.svg").write_text(svg)
    names.append(name)

rs = ["// Written by tools/icons.py from the mockups' icon set; run it again to change them.", "pub const ICONS: &[(&str, &str)] = &["]
for name in names:
    rs.append(f'    ("lyra-{name}-symbolic.svg", include_str!("../data/icons/lyra-{name}-symbolic.svg")),')
rs.append("];")
(root / "apps/lyra/src/icons.rs").write_text("\n".join(rs) + "\n")
print(f"{len(names)} icons")
