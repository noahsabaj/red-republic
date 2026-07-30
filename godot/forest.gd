extends RefCounted

## Scatters trees over the ground the simulation calls forest.
##
## `tools/build_trees.py` makes three species in Blender. `terrain_mesh::
## forest_buffer` on the Rust side decides where each one stands and hands back a
## ready-made `MultiMesh` instance buffer, so a whole wood costs three draw calls
## and this file does no work per tree.
##
## # Forest was a colour until now
##
## `looks.gd` had a `forest` tint and `terrain.gdshader` mixed it in, and that
## was the entire representation: a green patch on the ground. Nothing was ever
## instanced. It is the largest single gap between this and a game that looks
## real, and it is why bare forest floor read as desert once the flat colours
## were replaced with a photograph of leaf litter.
##
## # Why the placement is in Rust
##
## The first version did it here, walking the finished terrain mesh's 361,201
## vertices in GDScript and building a Transform3D per tree. It hung a render for
## eight minutes. Rust walks the terrain directly and returns the buffer in the
## exact layout `MultiMesh` takes, which is the same marshalling rule the rest of
## this boundary follows: never a structure per entity, always one packed array
## with a documented stride.

## Metres between candidate planting sites. Rust jitters within this, so it is
## the average spacing of the wood rather than a grid pitch.
##
## Every terrain cell is 10 m, so planting on all of them would be a tree every
## ten metres across every forested cell -- denser than a real wood and far more
## than the frame budget wants.
##
## 22 m gave 48,892 trees and measured 15.3 ms p50 / 17.5 ms p95, which is over
## the 16.7 ms a 60 fps frame has. Real-time is the thesis, so the wood loses
## the argument: density is the one thing here that trades directly against it.
const SPACING := 34.0


## Tint per species, multiplied into the instance colour.
##
## Deliberately not per-tree random hues: a wood is one or two colours with
## variation inside them, and a field of individually-coloured trees reads as
## confetti. The variation below is in brightness, which is what dappled light
## on a canopy actually does.
const TINTS := [
	Color(0.24, 0.32, 0.24),  # spruce, dark and blue-green
	Color(0.38, 0.44, 0.24),  # broadleaf, warmer
	Color(0.46, 0.52, 0.33),  # birch, pale
]


static func species_meshes() -> Array:
	var packed: PackedScene = load("res://models/trees.glb")
	var root := packed.instantiate()
	var out := []
	_collect(root, out)
	root.queue_free()
	return out


static func _collect(node: Node, out: Array) -> void:
	if node is MeshInstance3D and node.mesh != null:
		out.append(node.mesh)
	for child in node.get_children():
		_collect(child, out)


## Floats per instance in the buffer Rust hands over: a 3x4 transform, then RGBA.
const FLOATS_PER_TREE := 16


## Build one MultiMeshInstance3D per species under `parent`.
##
## All of the work is on the Rust side -- see `terrain_mesh::forest_buffer`,
## which returns the instance buffer already in the layout `MultiMesh` wants.
## This assigns it and sets up the material, and does nothing per tree.
static func plant(parent: Node3D, republic: Node, meshes: Array) -> int:
	for child in parent.get_children():
		child.queue_free()
	if meshes.is_empty():
		return 0

	var planted := 0
	for s in meshes.size():
		var buffer: PackedFloat32Array = republic.forest_buffer(s, meshes.size(), SPACING)
		var count := buffer.size() / FLOATS_PER_TREE
		if count == 0:
			continue

		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.use_colors = true
		mm.mesh = meshes[s]
		# Order matters: `instance_count` allocates, `buffer` fills. Assigning
		# the buffer first silently does nothing.
		mm.instance_count = count
		mm.buffer = buffer

		var node := MultiMeshInstance3D.new()
		node.multimesh = mm
		node.name = "Species%d" % s
		# The species tint lives here rather than in the buffer, because what
		# colour a spruce is happens to be an art decision and Rust does not get
		# those. The buffer's colour is a neutral brightness lift that multiplies
		# against this.
		var mat := StandardMaterial3D.new()
		mat.albedo_color = TINTS[s % TINTS.size()]
		mat.vertex_color_use_as_albedo = true
		mat.roughness = 0.95
		node.material_override = mat

		# Beyond this the canopy is smaller than a pixel and the wood is carried
		# by the forest-floor colour underneath it.
		#
		# **No `visibility_range_fade_mode`.** Setting it to FADE_SELF forces the
		# material into transparent rendering, which for tens of thousands of
		# dense overlapping canopies means no early-Z and a depth sort every
		# frame. A hard cutoff pops slightly at three kilometres; the alternative
		# was seconds per frame.
		node.visibility_range_end = 3200.0

		# Trees do not cast into the shadow map, and this is the single largest
		# frame saving in the whole forest.
		#
		# Measured: halving the wood from 48,892 trees to 21,715 saved only 1.7 ms
		# of about 9 ms, which says the cost is not per-tree at all -- it is that
		# every one of these MultiMeshes is drawn again into each of four shadow
		# cascades at 8192 square. Turning that off is worth several times what
		# thinning the wood was.
		#
		# What it costs is dappling on the forest floor, which at the distance a
		# wood is ever seen from is carried by the canopy being dark anyway. They
		# still RECEIVE shadow, so a hillside still shades its own trees.
		node.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		parent.add_child(node)
		planted += count

	return planted
