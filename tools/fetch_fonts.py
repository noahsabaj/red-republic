"""Fetch the typefaces the interface is set in.

Run:

    python tools/fetch_fonts.py

Downloads from the Google Fonts repository, writes `godot/art/fonts/`, and
records the provenance and the licence beside them. The results are committed,
so this is run when the type changes rather than as part of a build -- the same
deal as `tools/fetch_art.py` and `tools/build_icon.py`.

# Why PT, and not a typeface chosen for looking Soviet

PT Sans, PT Sans Narrow and PT Mono are ParaType's, drawn for the *Public Types
of the Russian Federation* programme -- a state-commissioned family, released
under the SIL Open Font Licence, whose brief was signage and documents for
public institutions. The interface this project wants is the paperwork of a
ministry, so the type is not an impression of a state typeface: it is one.

They also solve the three problems the interface actually has:

- **PT Sans Narrow** sets a heading in capitals at a width that fits, which is
  what a stamped title block needs and what a normal-width grotesque cannot do
  without wrapping.
- **PT Mono** has tabular figures, so a column of tonnages lines up on the
  decimal point instead of shuffling as the numbers change. Every table in this
  game is numbers that update every frame; proportional digits make them crawl.
- **PT Sans** carries running prose at 15 px without the closed apertures that
  make Godot's default face muddy at small sizes.

# Licence

SIL Open Font Licence 1.1 -- redistribution allowed, including inside a
proprietary application, provided the licence travels with the files. `OFL.txt`
is written beside them for exactly that reason, and the installer ships the
whole directory.
"""

import os
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "godot", "art", "fonts")

RAW = "https://raw.githubusercontent.com/google/fonts/main/"

# Our name -> (path in google/fonts, what it is for). The names on the left are
# what `godot/ui/theme.tres` asks for; the paths on the right are what somebody
# has to look up if a face ever needs replacing.
FACES = {
    "PTSans-Regular.ttf": (
        "ofl/ptsans/PT_Sans-Web-Regular.ttf",
        "Running prose, and anything a player reads a sentence of.",
    ),
    "PTSans-Bold.ttf": (
        "ofl/ptsans/PT_Sans-Web-Bold.ttf",
        "Emphasis inside prose, and the name of a thing in a row.",
    ),
    "PTSans-Italic.ttf": (
        "ofl/ptsans/PT_Sans-Web-Italic.ttf",
        "The briefing, which is somebody speaking rather than the state stating.",
    ),
    "PTSansNarrow-Regular.ttf": (
        "ofl/ptsansnarrow/PT_Sans-Narrow-Web-Regular.ttf",
        "Labels, column heads and buttons: capitals, letterspaced, in a hurry.",
    ),
    "PTSansNarrow-Bold.ttf": (
        "ofl/ptsansnarrow/PT_Sans-Narrow-Web-Bold.ttf",
        "Title blocks and section heads -- the stamped part of a form.",
    ),
    "PTMono-Regular.ttf": (
        "ofl/ptmono/PTM55FT.ttf",
        "Every figure. Tabular, so a column of numbers stops shuffling.",
    ),
}

LICENCE = "ofl/ptsans/OFL.txt"


def fetch(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=60) as response:
        return response.read()


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    for name, (path, _) in FACES.items():
        data = fetch(RAW + path)
        with open(os.path.join(OUT, name), "wb") as handle:
            handle.write(data)
        print("wrote %s (%d KB)" % (name, len(data) // 1024))

    with open(os.path.join(OUT, "OFL.txt"), "wb") as handle:
        handle.write(fetch(RAW + LICENCE))

    rows = "\n".join(
        "| `%s` | [%s](https://github.com/google/fonts/blob/main/%s) | %s |"
        % (name, path.rsplit("/", 1)[1], path, why)
        for name, (path, why) in FACES.items()
    )
    with open(os.path.join(OUT, "SOURCES.md"), "w", encoding="utf-8") as handle:
        handle.write(
            "# Where the type came from\n\n"
            "Written by `tools/fetch_fonts.py`. Everything below is ParaType's\n"
            "PT family under the **SIL Open Font Licence 1.1** (`OFL.txt` beside\n"
            "this file), drawn for the *Public Types of the Russian Federation*\n"
            "programme -- state type for state documents, which is what this\n"
            "interface is dressed as.\n\n"
            "| file | upstream | what it sets |\n|---|---|---|\n" + rows + "\n"
        )
    print("wrote OFL.txt and SOURCES.md")


if __name__ == "__main__":
    main()
