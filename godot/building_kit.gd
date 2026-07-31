extends RefCounted

## Assembles one mesh per building kind out of the kit components.
##
## `tools/build_kit.py` makes the parts in Blender -- panel walls, roofs,
## chimneys, silos, a pithead gantry -- in a unit cell. This puts them together
## according to `building_art.gd`, scaled to the real width and depth the
## simulation authors.
##
## Done once at load, giving one mesh per kind, each drawn as its own
## `MultiMesh`. That keeps the per-instance cost at a transform in a packed
## buffer however many buildings a republic ends up with, which is the measured
## rule the whole marshalling boundary is built around.

const Art := preload("res://building_art.gd")

const KIT_PATH := "res://models/kit.glb"


## Load the kit and pull the component meshes out of the imported scene.
static func components() -> Dictionary:
	var packed: PackedScene = load(KIT_PATH)
	var root := packed.instantiate()
	var out := {}
	_collect(root, out)
	root.queue_free()
	return out


static func _collect(node: Node, out: Dictionary) -> void:
	if node is MeshInstance3D and node.mesh != null:
		out[node.name] = node.mesh
	for child in node.get_children():
		_collect(child, out)


## Build the mesh for one building kind.
##
## `width` and `depth` are the real metric footprint from `BuildingDef`; every
## vertical dimension comes from the art table. The result stands on the ground
## with its origin at the centre of its base, so a placement transform is the
## building's position and nothing else.
static func assemble(
	parts: Dictionary, art, width: float, depth: float, tones: Dictionary
) -> ArrayMesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)

	var height: float = art.height()
	var body := Transform3D(Basis.from_scale(Vector3(width, height, depth)), Vector3.ZERO)
	_append(st, parts.get("panel_wall"), body)

	var roof_name: String = ["roof_flat", "roof_pitch", "roof_sawtooth"][art.roof]
	var roof_scale := Vector3(width, 1.0, depth)
	if art.roof == Art.Roof.PITCH:
		roof_scale.y = min(width, depth) * 0.5
	elif art.roof == Art.Roof.SAWTOOTH:
		roof_scale.y = min(width, depth) * 0.35
	_append(
		st,
		parts.get(roof_name),
		Transform3D(Basis.from_scale(roof_scale), Vector3(0.0, height, 0.0))
	)

	# Chimneys along the back edge, spaced so two do not overlap.
	var stack: Mesh = parts.get("stack")
	if stack != null and art.stacks > 0:
		for i in int(art.stacks):
			var span := width * 0.5
			var x: float = 0.0 if art.stacks == 1 else lerpf(-span * 0.5, span * 0.5, float(i) / float(art.stacks - 1))
			_append(st, stack, Transform3D(Basis.IDENTITY, Vector3(x, height * 0.85, -depth * 0.32)))

	# Silos along one flank.
	var silo: Mesh = parts.get("silo")
	if silo != null and art.silos > 0:
		for i in int(art.silos):
			var z: float = lerpf(-depth * 0.3, depth * 0.3, 0.5 if art.silos == 1 else float(i) / float(art.silos - 1))
			_append(st, silo, Transform3D(Basis.IDENTITY, Vector3(width * 0.5 + 3.5, 0.0, z)))

	# The headframe stands BESIDE the shaft house, not on its roof. Put on top it
	# reads as a toy left on a slab, which is exactly how it looked the first
	# time. Scaled with the footprint so it stays the dominant feature of a mine
	# however big the building under it is.
	if art.gantry and parts.has("gantry"):
		var g: float = clampf(min(width, depth) / 30.0, 0.8, 2.2)
		_append(
			st,
			parts.get("gantry"),
			Transform3D(Basis.from_scale(Vector3(g, g, g)), Vector3(0.0, 0.0, depth * 0.5 + 9.0 * g))
		)

	st.generate_normals()
	var mesh: ArrayMesh = st.commit()
	var mat := StandardMaterial3D.new()
	mat.albedo_color = tones.get(art.tone, Color(0.75, 0.72, 0.66))
	mat.roughness = 0.88
	mat.metallic = 0.35 if art.tone == Art.Tone.METAL else 0.0
	mesh.surface_set_material(0, mat)

	# **The windows are their own surface, and that is what makes them light up.**
	# Masking emission inside the wall material would need a per-vertex channel
	# that `_append` does not carry, and a MultiMesh's own per-instance colour
	# already occupies the one that would have. A second surface costs one more
	# draw call per kind and needs nothing carried at all.
	#
	# A window band per storey, inset so it reads as a reveal rather than a
	# stripe. Real height, never stretched -- a five-metre window is the fastest
	# way to make a factory look like a toy.
	var band: Mesh = parts.get("window_band")
	if band != null and art.storeys > 0:
		var glass := SurfaceTool.new()
		glass.begin(Mesh.PRIMITIVE_TRIANGLES)
		for i in int(art.storeys):
			var y: float = (float(i) + 0.45) * art.storey_m
			var t := Transform3D(
				Basis.from_scale(Vector3(width * 1.004, 1.0, depth * 1.004)),
				Vector3(0.0, y, 0.0)
			)
			_append(glass, band, t)
		glass.generate_normals()
		glass.commit(mesh)
		var lit := ShaderMaterial.new()
		lit.shader = load("res://windows.gdshader")
		mesh.surface_set_material(mesh.get_surface_count() - 1, lit)
	return mesh


static func _append(st: SurfaceTool, mesh, transform: Transform3D) -> void:
	if mesh == null:
		return
	for surface in int(mesh.get_surface_count()):
		var arrays: Array = mesh.surface_get_arrays(surface)
		var verts: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
		var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
		if indices.is_empty():
			for i in verts.size():
				st.add_vertex(transform * verts[i])
		else:
			for i in indices:
				st.add_vertex(transform * verts[i])
