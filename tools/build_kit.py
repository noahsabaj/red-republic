"""Generate the building kit of parts.

Run headless:

    blender --background --python tools/build_kit.py

Writes `godot/models/kit.glb`, holding one mesh per component. Nothing here
knows what a Coal Mine looks like — it makes *panels, roofs, stacks, silos and
gantries*, and Godot assembles a building out of them from the art data
authored beside it. That split is the whole point:

- A new building becomes a data row rather than a modelling job, which is what
  makes a hundred-plus roster affordable at all.
- The roster stays visually coherent by construction rather than by discipline,
  because every building is made of the same parts.
- It is also how Soviet construction actually worked. Standardised prefabricated
  panels, assembled to a type plan on site. The pipeline and the subject agree,
  which is rarely free.

Every component is modelled in a **unit cell** so Godot can scale it to the real
metric footprint a `BuildingDef` authors: 1 m wide, 1 m deep, 1 m tall, sitting
on the ground plane with its origin at the centre of its base. Anything that
must not stretch — a chimney's roundness, a window's reveal depth — is built at
real size and placed rather than scaled.
"""

import math
import os
import sys

import bpy
import bmesh
from mathutils import Vector

OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "godot", "models")

# One prefab panel is three metres across. Everything that reads as "made of
# panels" is a multiple of this, and it is the number that makes a wall look
# assembled rather than extruded.
PANEL_M = 3.0


def clear_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def new_mesh(name):
    mesh = bpy.data.meshes.new(name)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return obj, bmesh.new()


def finish(obj, bm):
    bm.to_mesh(obj.data)
    bm.free()
    return obj


def box(bm, centre, size):
    """An axis-aligned box, added into an existing bmesh."""
    cx, cy, cz = centre
    sx, sy, sz = (s * 0.5 for s in size)
    verts = [
        bm.verts.new((cx - sx, cy - sy, cz - sz)),
        bm.verts.new((cx + sx, cy - sy, cz - sz)),
        bm.verts.new((cx + sx, cy + sy, cz - sz)),
        bm.verts.new((cx - sx, cy + sy, cz - sz)),
        bm.verts.new((cx - sx, cy - sy, cz + sz)),
        bm.verts.new((cx + sx, cy - sy, cz + sz)),
        bm.verts.new((cx + sx, cy + sy, cz + sz)),
        bm.verts.new((cx - sx, cy + sy, cz + sz)),
    ]
    faces = [
        (0, 3, 2, 1), (4, 5, 6, 7),
        (0, 1, 5, 4), (1, 2, 6, 5),
        (2, 3, 7, 6), (3, 0, 4, 7),
    ]
    for f in faces:
        bm.faces.new([verts[i] for i in f])


def cylinder(bm, centre, radius, height, segments=16):
    cx, cy, cz = centre
    bottom, top = [], []
    for i in range(segments):
        a = 2.0 * math.pi * i / segments
        x, y = cx + radius * math.cos(a), cy + radius * math.sin(a)
        bottom.append(bm.verts.new((x, y, cz)))
        top.append(bm.verts.new((x, y, cz + height)))
    for i in range(segments):
        j = (i + 1) % segments
        bm.faces.new([bottom[i], bottom[j], top[j], top[i]])
    bm.faces.new(list(reversed(bottom)))
    bm.faces.new(top)


# ---------------------------------------------------------------------------
# The components.
# ---------------------------------------------------------------------------


def panel_wall():
    """The body of a building: a unit box with a panel grid pressed into it.

    Scaled to the real footprint by Godot, so the seams stretch with it. That is
    correct rather than a compromise — a bigger building really is more panels,
    and at the distance this game is played from the seam count reads as scale.
    """
    obj, bm = new_mesh("panel_wall")
    box(bm, (0.0, 0.0, 0.5), (1.0, 1.0, 1.0))
    # Recess a shallow grid so the flat faces catch light unevenly.
    inset = 0.012
    for k in range(1, 4):
        z = k / 4.0
        for sx, sy in ((0.0, 0.5), (0.0, -0.5), (0.5, 0.0), (-0.5, 0.0)):
            n = (0.0, sy * 2.0, 0.0) if sy else (sx * 2.0, 0.0, 0.0)
            centre = (sx * (1.0 - inset), sy * (1.0 - inset), z)
            size = (1.0 if sy else inset * 2.0, inset * 2.0 if sy else 1.0, 0.02)
            box(bm, centre, size)
    return finish(obj, bm)


def window_band():
    """One storey's worth of glazing, as a strip around all four walls.

    A **frame**, not a slab. The first version was a solid unit box, which does
    not read as a window at all: scaled slightly proud of the wall it became a
    protruding ledge, and a two-storey building came out looking like a stack of
    six shelves. Only a close render showed it.

    Height is real rather than unit -- a window is about 1.4 m whatever the
    building, and stretching one is the fastest way to make a factory look like
    a toy -- so this is built 1x1 in plan and 1.4 m tall, and Godot scales only
    the plan.
    """
    obj, bm = new_mesh("window_band")
    t = 0.06
    for cx, cy, sx, sy in (
        (0.0, 0.5, 1.0, t), (0.0, -0.5, 1.0, t),
        (0.5, 0.0, t, 1.0), (-0.5, 0.0, t, 1.0),
    ):
        box(bm, (cx, cy, 0.7), (sx, sy, 1.4))
    return finish(obj, bm)


def roof_flat():
    """Flat roof with a parapet. The default for anything residential or civic."""
    obj, bm = new_mesh("roof_flat")
    box(bm, (0.0, 0.0, 0.06), (1.0, 1.0, 0.12))
    for sx, sy, w, d in ((0.0, 0.5, 1.0, 0.06), (0.0, -0.5, 1.0, 0.06),
                         (0.5, 0.0, 0.06, 1.0), (-0.5, 0.0, 0.06, 1.0)):
        box(bm, (sx, sy, 0.28), (w, d, 0.44))
    return finish(obj, bm)


def roof_pitch():
    """A shallow gable, for small housing and outbuildings."""
    obj, bm = new_mesh("roof_pitch")
    h = 0.55
    v = [
        bm.verts.new((-0.5, -0.5, 0.0)), bm.verts.new((0.5, -0.5, 0.0)),
        bm.verts.new((0.5, 0.5, 0.0)), bm.verts.new((-0.5, 0.5, 0.0)),
        bm.verts.new((-0.5, 0.0, h)), bm.verts.new((0.5, 0.0, h)),
    ]
    bm.faces.new([v[0], v[3], v[2], v[1]])
    bm.faces.new([v[0], v[1], v[5], v[4]])
    bm.faces.new([v[2], v[3], v[4], v[5]])
    bm.faces.new([v[1], v[2], v[5]])
    bm.faces.new([v[3], v[0], v[4]])
    return finish(obj, bm)


def roof_sawtooth():
    """North-light sawtooth. The roof that says *this is a works* at a glance."""
    obj, bm = new_mesh("roof_sawtooth")
    teeth = 5
    w = 1.0 / teeth
    for i in range(teeth):
        y0 = -0.5 + i * w
        v = [
            bm.verts.new((-0.5, y0, 0.0)), bm.verts.new((0.5, y0, 0.0)),
            bm.verts.new((0.5, y0 + w, 0.0)), bm.verts.new((-0.5, y0 + w, 0.0)),
            bm.verts.new((-0.5, y0, 0.42)), bm.verts.new((0.5, y0, 0.42)),
        ]
        bm.faces.new([v[0], v[3], v[2], v[1]])
        bm.faces.new([v[0], v[1], v[5], v[4]])
        bm.faces.new([v[2], v[3], v[4], v[5]])
        bm.faces.new([v[1], v[2], v[5]])
        bm.faces.new([v[3], v[0], v[4]])
    return finish(obj, bm)


def stack():
    """A chimney, at real size. Anything that burns something gets one."""
    obj, bm = new_mesh("stack")
    cylinder(bm, (0.0, 0.0, 0.0), 1.4, 22.0, segments=14)
    cylinder(bm, (0.0, 0.0, 22.0), 1.7, 1.2, segments=14)
    return finish(obj, bm)


def silo():
    """A storage cylinder with a shallow cap. Grain, cement, aggregate."""
    obj, bm = new_mesh("silo")
    cylinder(bm, (0.0, 0.0, 0.0), 3.0, 12.0, segments=18)
    verts = []
    for i in range(18):
        a = 2.0 * math.pi * i / 18
        verts.append(bm.verts.new((3.0 * math.cos(a), 3.0 * math.sin(a), 12.0)))
    apex = bm.verts.new((0.0, 0.0, 15.0))
    for i in range(18):
        bm.faces.new([verts[i], verts[(i + 1) % 18], apex])
    return finish(obj, bm)


def gantry():
    """A pithead headframe. What makes a mine read as a mine from the air."""
    obj, bm = new_mesh("gantry")
    leg = 0.6
    for sx in (-4.0, 4.0):
        for sy in (-4.0, 4.0):
            box(bm, (sx, sy, 9.0), (leg, leg, 18.0))
    box(bm, (0.0, 0.0, 18.2), (9.2, 9.2, 0.7))
    # The winding wheel, laid on its side across the head.
    for sx in (-1.6, 1.6):
        cylinder(bm, (sx, 0.0, 19.0), 2.6, 0.35, segments=16)
    box(bm, (0.0, 0.0, 4.0), (9.6, 0.5, 0.5))
    return finish(obj, bm)


COMPONENTS = [
    panel_wall, window_band, roof_flat, roof_pitch, roof_sawtooth,
    stack, silo, gantry,
]


def main():
    clear_scene()
    for build in COMPONENTS:
        build()

    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, "kit.glb")
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        export_apply=True,
        export_yup=True,
    )
    names = sorted(o.name for o in bpy.data.objects)
    print(f"KIT: wrote {path} with {len(names)} components: {', '.join(names)}")


if __name__ == "__main__":
    main()
