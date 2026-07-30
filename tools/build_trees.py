"""Generate the tree models the forests are made of.

Run headless:

    blender --background --python tools/build_trees.py

Writes `godot/models/trees.glb`, holding one mesh per species. Godot scatters
them with a `MultiMesh` over the cells the simulation says are forest.

# Why these are deliberately simple

A forest here is seen from between three hundred metres and three kilometres.
At that range what carries a tree is its **silhouette and its colour**, not its
leaves: a photoreal canopy with alpha-tested foliage costs overdraw on every one
of thirty thousand instances to render detail smaller than a pixel. So each
species is a few hundred triangles of solid geometry with a shape you can
recognise from above, and the budget goes into having enough of them.

The trunk matters less than it seems for the same reason -- from above you see
canopy -- but it is there because at the closest zoom a canopy floating over
nothing is worse than a cheap trunk.

# The species

Three, chosen so a mixed wood reads as a wood rather than as one asset repeated:
a spruce that is a tall dark cone, a broadleaf that is a wide round mass, and a
birch that is narrow and pale. Godot picks between them per instance and tints
each one slightly, which is what stops a forest looking stamped.
"""

import math
import os
import sys

import bmesh
import bpy
from mathutils import Vector

OUT = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "godot", "models"
)

# Metres. Every species is modelled at its real height so Godot can place it
# without a scale factor nobody can check.
TRUNK_BROWN = (0.20, 0.14, 0.09, 1.0)


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
    obj.data.update()


def cone(bm, base, radius, height, segments=7):
    """A closed cone. Seven sides is enough at this range and cheap enough."""
    ring = []
    for i in range(segments):
        angle = 2.0 * math.pi * i / segments
        ring.append(
            bm.verts.new((base.x + radius * math.cos(angle), base.y + radius * math.sin(angle), base.z))
        )
    tip = bm.verts.new((base.x, base.y, base.z + height))
    for i in range(segments):
        bm.faces.new((ring[i], ring[(i + 1) % segments], tip))
    bm.faces.new(tuple(reversed(ring)))


def drum(bm, base, lower, upper, height, segments=7):
    """A tapered cylinder: trunks, and the barrel of a broadleaf canopy."""
    bottom, top = [], []
    for i in range(segments):
        angle = 2.0 * math.pi * i / segments
        bottom.append(
            bm.verts.new((base.x + lower * math.cos(angle), base.y + lower * math.sin(angle), base.z))
        )
        top.append(
            bm.verts.new(
                (base.x + upper * math.cos(angle), base.y + upper * math.sin(angle), base.z + height)
            )
        )
    for i in range(segments):
        j = (i + 1) % segments
        bm.faces.new((bottom[i], bottom[j], top[j], top[i]))
    bm.faces.new(tuple(reversed(bottom)))
    bm.faces.new(tuple(top))


def blob(bm, centre, radius, squash=1.0, rings=3, segments=7):
    """A lumpy ellipsoid. The mass a broadleaf canopy reads as from above."""
    grid = []
    for r in range(1, rings):
        phi = math.pi * r / rings
        row = []
        for s in range(segments):
            theta = 2.0 * math.pi * s / segments
            # A little per-vertex wobble, so no two lobes are the same sphere
            # and the silhouette has bumps in it.
            wobble = 1.0 + 0.16 * math.sin(3.0 * theta + 5.0 * phi)
            row.append(
                bm.verts.new(
                    (
                        centre.x + radius * wobble * math.sin(phi) * math.cos(theta),
                        centre.y + radius * wobble * math.sin(phi) * math.sin(theta),
                        centre.z + radius * wobble * squash * math.cos(phi),
                    )
                )
            )
        grid.append(row)
    top = bm.verts.new((centre.x, centre.y, centre.z + radius * squash))
    bottom = bm.verts.new((centre.x, centre.y, centre.z - radius * squash))
    for s in range(segments):
        t = (s + 1) % segments
        bm.faces.new((grid[0][s], grid[0][t], top))
        bm.faces.new((bottom, grid[-1][t], grid[-1][s]))
    for r in range(len(grid) - 1):
        for s in range(segments):
            t = (s + 1) % segments
            bm.faces.new((grid[r][s], grid[r][t], grid[r + 1][t], grid[r + 1][s]))


def spruce():
    """Tall dark cone. The one that makes a taiga read as taiga."""
    obj, bm = new_mesh("spruce")
    drum(bm, Vector((0, 0, 0)), 0.34, 0.18, 3.2)
    # Three skirts of decreasing radius. A single cone reads as a party hat;
    # the steps are what say conifer.
    for i, (z, radius, height) in enumerate([(2.0, 2.5, 6.0), (5.4, 1.8, 5.2), (8.4, 1.1, 4.4)]):
        cone(bm, Vector((0, 0, z)), radius, height)
    finish(obj, bm)
    return obj


def broadleaf():
    """Wide round mass on a short trunk."""
    obj, bm = new_mesh("broadleaf")
    drum(bm, Vector((0, 0, 0)), 0.42, 0.30, 4.2)
    blob(bm, Vector((0, 0, 7.4)), 3.5, squash=0.82)
    # A second smaller lobe, offset, so the crown is not one ball.
    blob(bm, Vector((1.5, 0.7, 6.2)), 2.1, squash=0.9)
    finish(obj, bm)
    return obj


def birch():
    """Narrow and pale, and shorter than the other two."""
    obj, bm = new_mesh("birch")
    drum(bm, Vector((0, 0, 0)), 0.20, 0.13, 5.0)
    blob(bm, Vector((0, 0, 7.2)), 1.9, squash=1.45)
    finish(obj, bm)
    return obj


SPECIES = [spruce, broadleaf, birch]


def main():
    clear_scene()
    for build in SPECIES:
        obj = build()
        # Flat shading. These are faceted shapes and smoothing them makes a cone
        # look like a rubber cone; the facets are what read as foliage clumps.
        for polygon in obj.data.polygons:
            polygon.use_smooth = False

    os.makedirs(OUT, exist_ok=True)
    target = os.path.join(OUT, "trees.glb")
    bpy.ops.export_scene.gltf(
        filepath=target,
        export_format="GLB",
        export_apply=True,
        export_yup=True,
    )
    total = sum(len(o.data.polygons) for o in bpy.data.objects if o.type == "MESH")
    print(f"wrote {target}: {len(SPECIES)} species, {total} faces")


if __name__ == "__main__":
    main()
    sys.exit(0)
