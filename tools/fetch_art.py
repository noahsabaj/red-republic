"""Fetch the CC0 surface materials the terrain and buildings are made of.

Run:

    python tools/fetch_art.py

Downloads from ambientCG, keeps the three maps Godot actually shades with, and
writes `godot/art/textures/` plus a SOURCES.md recording where every file came
from. The results are committed, so this is run when the material set changes
rather than as part of a build -- the same deal as `tools/build_icon.py`.

# Why these are downloaded rather than generated

Everything else in `tools/` makes its own output: the icon is drawn in code, the
building kit is modelled in Blender. Surface materials are the exception, and the
reason is that photoreal ground is not reachable procedurally. Noise can give you
a plausible pattern; it cannot give you the way real grass has seed heads and
dead patches and soil showing through, which is the whole difference between
ground that looks made and ground that looks photographed.

# Licence

Everything here is CC0 from ambientCG -- public domain, no attribution required,
no conflict with this project's proprietary licence. SOURCES.md records the
provenance anyway, because "where did this file come from" is a question worth
being able to answer years later and the answer is cheap to keep.

# Why NormalGL and not NormalDX

Godot expects OpenGL-convention normal maps, where +Y is up. ambientCG ships
both; taking the DirectX one inverts every slope's lighting, which reads as
"the light is coming from the wrong side" and is very hard to see in a still.
"""

import io
import os
import sys
import urllib.request
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "godot", "art", "textures")

# Our semantic name -> the ambientCG asset it comes from. The names on the left
# are what the shaders ask for; the ids on the right are what somebody has to
# look up if a material ever needs replacing.
MATERIALS = {
    "grass": ("Grass004", "Meadow grass, mixed green with dry blades and visible soil."),
    "forest": ("Ground003", "Forest floor: needles, leaf litter and twigs."),
    "rock": ("Rock030", "Bare grey rock, for slopes too steep to hold soil."),
    "dirt": ("Ground037", "Bare earth, for worn ground and cuttings."),
    "shore": ("Gravel022", "Water-worn gravel, for the margin where land meets water."),
    "snow": ("Snow006", "Fresh snow with a wind-textured surface."),
}

# The three the shader samples. Displacement and AO are in the archives too and
# are deliberately left out: this terrain is a heightmapped mesh that already has
# its own relief, and a second displacement fighting it is worse than none.
WANTED = {
    "Color": "colour",
    "NormalGL": "normal",
    "Roughness": "roughness",
}

RESOLUTION = "1K"


def fetch(asset_id):
    url = f"https://ambientcg.com/get?file={asset_id}_{RESOLUTION}-JPG.zip"
    print(f"    {url}")
    request = urllib.request.Request(url, headers={"User-Agent": "red-republic-art-fetch"})
    with urllib.request.urlopen(request) as response:
        return response.read()


def main():
    os.makedirs(OUT, exist_ok=True)
    rows = []

    for name, (asset_id, description) in MATERIALS.items():
        print(f"{name} <- {asset_id}")
        try:
            blob = fetch(asset_id)
        except Exception as error:  # noqa: BLE001 - the message is the point
            print(f"    FAILED: {error}", file=sys.stderr)
            return 1

        kept = []
        with zipfile.ZipFile(io.BytesIO(blob)) as archive:
            for entry in archive.namelist():
                for suffix, ours in WANTED.items():
                    if entry.endswith(f"_{suffix}.jpg"):
                        target = os.path.join(OUT, f"{name}_{ours}.jpg")
                        with open(target, "wb") as handle:
                            handle.write(archive.read(entry))
                        kept.append(ours)
        missing = set(WANTED.values()) - set(kept)
        if missing:
            print(f"    FAILED: {asset_id} has no {', '.join(sorted(missing))}", file=sys.stderr)
            return 1
        print(f"    kept {', '.join(sorted(kept))}")
        rows.append((name, asset_id, description))

    with open(os.path.join(ROOT, "godot", "art", "SOURCES.md"), "w", encoding="utf-8") as handle:
        handle.write("# Where the art came from\n\n")
        handle.write(
            "Written by `tools/fetch_art.py`. Everything below is **CC0** from\n"
            "[ambientCG](https://ambientcg.com) — public domain, no attribution required.\n"
            "It is recorded anyway: CC0 does not oblige anyone to say where a file came\n"
            "from, and being unable to answer that about your own repository is its own\n"
            "problem.\n\n"
            f"Downloaded at {RESOLUTION}, keeping colour, OpenGL-convention normal and\n"
            "roughness. Displacement and ambient occlusion are in the source archives and\n"
            "are deliberately not kept — this terrain is a heightmapped mesh with its own\n"
            "relief, and a second displacement fighting it is worse than none.\n\n"
            "| used as | ambientCG asset | what it is |\n|---|---|---|\n"
        )
        for name, asset_id, description in rows:
            handle.write(f"| `{name}` | [{asset_id}](https://ambientcg.com/view?id={asset_id}) | {description} |\n")

    total = sum(
        os.path.getsize(os.path.join(OUT, f)) for f in os.listdir(OUT) if f.endswith(".jpg")
    )
    print(f"\nwrote {len(rows)} materials to godot/art/textures ({total / 1024 / 1024:.1f} MB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
