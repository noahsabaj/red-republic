"""Generate the vehicle models the fleet is drawn with.

Run headless:

    blender --background --python tools/build_vehicles.py

Writes `game/models/vehicles.glb`, holding one mesh per body. Godot draws each
with a `MultiMesh` over the positions the simulation reports.

# What this replaces

Every vehicle in the republic was one `BoxMesh`, 6.0 x 3.0 x 2.5 m, in a flat
colour -- a lorry, a coach, a snow plough and a recovery truck all the same grey
brick. Thirteen vehicle kinds, one silhouette. At the zoom a player actually
watches freight from, the thing that says what a vehicle *is* is its outline,
and there was none to read.

# Why these are deliberately simple

Same argument as `build_trees.py`, and the same range: a vehicle is seen from
between a hundred metres and three kilometres, and what carries it is the
silhouette and the colour rather than the panel gaps. So each body is a few
hundred triangles with a shape recognisable from above and behind, and the
budget goes into having enough *different* ones.

Wheels are the exception worth paying for. A box floating a fixed height over a
road reads as a box; four dark drums under it read as a vehicle, and they cost
almost nothing because they are seven-sided and never seen edge-on from above.

# The bodies

Four, chosen so a road full of traffic reads as traffic rather than one asset
repeated:

* **lorry** -- a cab and a separate box body with a gap between them, which is
  the outline that says "goods vehicle" from directly above.
* **tanker** -- the same cab pulling a horizontal drum. Fuel, oil and chemicals
  move in these, and a cylinder is unmistakable from any angle.
* **coach** -- one long body with a window band, no gap. What carries people.
* **plough** -- a short cab with a blade angled across the front, which is the
  only one of the four you can identify from its shadow.

Dimensions are real metres. Nothing in the simulation authors a vehicle's size
-- it authors capacity, seats and speed -- so these are the shell's decision in
exactly the way `building_art.gd` is, and they are here rather than in Rust for
the same reason: a field no system reads has no business being simulation state.
"""

import math
import os

import bmesh
import bpy

OUT = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "game", "models"
)

# Nothing here is lit as metal. A Soviet lorry is painted steel that has been
# outside for a decade, so the bodies are mid-value and the glass is darker
# rather than reflective -- reflections at this range are a shimmer, not a
# highlight.
CAB = (0.34, 0.38, 0.34, 1.0)
BODY = (0.46, 0.45, 0.41, 1.0)
TANK = (0.55, 0.56, 0.58, 1.0)
GLASS = (0.13, 0.16, 0.18, 1.0)
TYRE = (0.09, 0.09, 0.10, 1.0)
BLADE = (0.62, 0.28, 0.22, 1.0)


def clear_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def new_mesh(name):
    mesh = bpy.data.meshes.new(name)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return obj, bmesh.new()


def finish(obj, bm, colours):
    """Bake per-face colour into a vertex colour layer and hand it to Godot.

    One material with vertex colours rather than four materials, because a
    `MultiMesh` draws one surface per material and four surfaces is four draw
    calls per vehicle kind for no visible gain.
    """
    layer = bm.loops.layers.color.new("Col")
    for face in bm.faces:
        rgba = colours.get(face.index, BODY)
        for loop in face.loops:
            loop[layer] = rgba
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()

    mat = bpy.data.materials.new("%s_mat" % obj.name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.72
    bsdf.inputs["Metallic"].default_value = 0.0
    attr = mat.node_tree.nodes.new("ShaderNodeVertexColor")
    attr.layer_name = "Col"
    mat.node_tree.links.new(attr.outputs["Color"], bsdf.inputs["Base Color"])
    obj.data.materials.append(mat)


def box(bm, centre, size, colours, rgba):
    """An axis-aligned box, recording its faces' colour."""
    cx, cy, cz = centre
    hx, hy, hz = size[0] / 2.0, size[1] / 2.0, size[2] / 2.0
    corners = [
        bm.verts.new((cx + sx * hx, cy + sy * hy, cz + sz * hz))
        for sx in (-1, 1)
        for sy in (-1, 1)
        for sz in (-1, 1)
    ]
    # Indices into `corners`, which is ordered x-major then y then z.
    quads = [
        (0, 1, 3, 2),  # -x
        (4, 6, 7, 5),  # +x
        (0, 4, 5, 1),  # -y
        (2, 3, 7, 6),  # +y
        (0, 2, 6, 4),  # -z
        (1, 5, 7, 3),  # +z
    ]
    for quad in quads:
        face = bm.faces.new(tuple(corners[i] for i in quad))
        face.index = len(bm.faces) - 1
        colours[face.index] = rgba
    bm.faces.index_update()


def drum(bm, centre, radius, length, axis, colours, rgba, segments=9):
    """A cylinder lying along `axis` ('x' or 'y'): a tank, or a wheel."""
    cx, cy, cz = centre
    caps = []
    for end in (-length / 2.0, length / 2.0):
        ring = []
        for i in range(segments):
            a = 2.0 * math.pi * i / segments
            u, v = radius * math.cos(a), radius * math.sin(a)
            if axis == "x":
                ring.append(bm.verts.new((cx + end, cy + u, cz + v)))
            else:
                ring.append(bm.verts.new((cx + u, cy + end, cz + v)))
        caps.append(ring)
    for i in range(segments):
        j = (i + 1) % segments
        face = bm.faces.new((caps[0][i], caps[0][j], caps[1][j], caps[1][i]))
        colours[face.index] = rgba
    for ring, flip in ((caps[0], True), (caps[1], False)):
        face = bm.faces.new(tuple(reversed(ring)) if flip else tuple(ring))
        colours[face.index] = rgba
    bm.faces.index_update()


def wheels(bm, colours, half_length, half_width, radius, pairs):
    """Wheel pairs at the given x offsets, sunk so the tyre meets the road."""
    for x in pairs:
        for sy in (-1, 1):
            drum(
                bm,
                (x, sy * (half_width - radius * 0.35), radius),
                radius,
                radius * 0.9,
                "y",
                colours,
                TYRE,
                segments=7,
            )
    _ = half_length


def build_lorry():
    """Cab, gap, box body. The outline that says goods vehicle from above."""
    obj, bm = new_mesh("lorry")
    colours = {}
    # 7.4 m long overall, 2.5 wide: a ZIL-130 is about that.
    box(bm, (2.35, 0.0, 1.95), (2.3, 2.4, 1.5), colours, CAB)
    box(bm, (2.35, 0.0, 2.45), (1.5, 2.2, 0.5), colours, GLASS)
    box(bm, (-1.2, 0.0, 2.05), (4.6, 2.5, 1.9), colours, BODY)
    wheels(bm, colours, 3.7, 1.25, 0.55, (2.3, -0.9, -2.6))
    finish(obj, bm, colours)


def build_tanker():
    """The same cab pulling a horizontal drum."""
    obj, bm = new_mesh("tanker")
    colours = {}
    box(bm, (2.35, 0.0, 1.95), (2.3, 2.4, 1.5), colours, CAB)
    box(bm, (2.35, 0.0, 2.45), (1.5, 2.2, 0.5), colours, GLASS)
    drum(bm, (-1.2, 0.0, 2.15), 1.15, 4.8, "x", colours, TANK)
    wheels(bm, colours, 3.7, 1.25, 0.55, (2.3, -0.9, -2.6))
    finish(obj, bm, colours)


def build_coach():
    """One long body with a window band. What carries people."""
    obj, bm = new_mesh("coach")
    colours = {}
    box(bm, (0.0, 0.0, 1.85), (10.0, 2.5, 2.3), colours, BODY)
    box(bm, (0.0, 0.0, 2.45), (9.4, 2.55, 0.85), colours, GLASS)
    wheels(bm, colours, 5.0, 1.25, 0.5, (3.6, -3.2))
    finish(obj, bm, colours)


def build_plough():
    """A short cab with a blade across the front, identifiable from its shadow."""
    obj, bm = new_mesh("plough")
    colours = {}
    box(bm, (0.4, 0.0, 2.05), (4.4, 2.5, 1.9), colours, CAB)
    box(bm, (1.4, 0.0, 2.6), (1.6, 2.3, 0.6), colours, GLASS)
    # Wider than the cab and set forward of it, which is the overhang that makes
    # a plough readable from directly overhead.
    box(bm, (3.1, -0.35, 1.15), (0.35, 3.4, 1.5), colours, BLADE)
    wheels(bm, colours, 2.6, 1.25, 0.6, (1.6, -1.3))
    finish(obj, bm, colours)


def main():
    clear_scene()
    build_lorry()
    build_tanker()
    build_coach()
    build_plough()

    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, "vehicles.glb")
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        export_apply=True,
        export_yup=True,
    )
    print("wrote %s" % path)


if __name__ == "__main__":
    main()
