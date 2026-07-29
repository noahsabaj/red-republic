extends RefCounted

## The map overlays: where everything the simulation knows and hides gets shown.
##
## The archived 1.x build had **no map data overlays at all**, so this is new
## design rather than a port. It has to be, because this simulation knows a
## great deal that has nowhere else to appear:
##
## - **Geology is a three-dimensional field** and resources are deliberately
##   invisible on the terrain. Without a survey overlay the entire subsurface —
##   depth, richness, how much is left — is a thing the player can only infer
##   from whether a mine works.
## - **The ground is state, not calendar.** Soil moisture and frost accumulate,
##   and the worst going of the year arrives a few weeks into spring on its own.
##   Nothing about that is visible in a green field.
## - **Traffic wears the ground into tracks** that are eventually promoted into
##   real roads. Watching a corridor form is the whole appeal of the mechanic,
##   and it is invisible right up until the road appears.
##
## Each is one channel painted over the terrain by `terrain.gdshader`, sampled
## from a small image built off the traversal lattice. The lattice is 100 m
## where the terrain is 10 m, so a 6 km map is a 60x60 texture — which is what
## makes rebuilding one whenever the ground changes free.

enum Mode { NONE, SURVEY, GOING, WEAR }

const NAMES := {
	Mode.NONE: "",
	Mode.SURVEY: "geological survey",
	Mode.GOING: "going",
	Mode.WEAR: "tracks",
}

## Low and high ends of each channel, chosen so the reading is unambiguous
## without a legend: bad ground is red, firm ground is green, and worn ground
## goes from the land's own colour up to the tone of a made track.
const RAMPS := {
	# Going is a BADNESS: `Crossing::going_in` is 0.0 firm to 1.0 impassable.
	# So low is green and high is red, which is the opposite of what it looked
	# like it should be -- the first version had this backwards, painted a dry
	# July map entirely red, and the comment above it confidently asserted the
	# wrong semantics. Read the source, not the name.
	Mode.GOING: [Color(0.34, 0.58, 0.36), Color(0.74, 0.22, 0.16)],
	Mode.WEAR: [Color(0.44, 0.50, 0.36), Color(0.30, 0.26, 0.22)],
	Mode.SURVEY: [Color(0.30, 0.32, 0.36), Color(0.86, 0.72, 0.28)],
}


## Build the sampled field for a mode, or null when the mode paints nothing.
##
## No arithmetic is done to the values: the ramps are ordered to match what each
## field actually means, which is checked against the simulation's own source
## rather than inferred from the name. Getting that backwards is the kind of
## error that looks like a balance problem, and it happened once here already.
static func field_texture(republic: Node, mode: int) -> ImageTexture:
	if mode == Mode.NONE or mode == Mode.SURVEY:
		return null
	var cells: int = republic.lattice_cells()
	if cells <= 0:
		return null
	var flat: PackedFloat32Array = (
		republic.going_field() if mode == Mode.GOING else republic.wear_field()
	)
	if flat.size() < cells * cells:
		return null
	var image := Image.create(cells, cells, false, Image.FORMAT_RF)
	# Wear is gamma-lifted, and that is a correctness fix rather than a taste
	# one. Wear FADES once a corridor is promoted into the road network -- a road
	# is deliberately not worn by the lorries riding it, or a road would wear
	# itself into a better road -- so on an established republic almost every
	# cell sits near zero and a linear ramp showed nothing at all. What a player
	# wants to read is "how close is this ground to becoming a track", and that
	# question lives in the bottom of the range.
	var gamma := 0.42 if mode == Mode.WEAR else 1.0
	for y in cells:
		for x in cells:
			var v: float = clampf(flat[y * cells + x], 0.0, 1.0)
			if gamma != 1.0:
				v = pow(v, gamma)
			image.set_pixel(x, y, Color(v, 0.0, 0.0))
	return ImageTexture.create_from_image(image)


## Apply a mode to the terrain material.
static func apply(material: ShaderMaterial, republic: Node, mode: int, extent: float) -> void:
	material.set_shader_parameter("map_extent", extent)
	if mode == Mode.NONE or mode == Mode.SURVEY:
		material.set_shader_parameter("overlay_strength", 0.0)
		return
	var tex := field_texture(republic, mode)
	if tex == null:
		material.set_shader_parameter("overlay_strength", 0.0)
		return
	var ramp: Array = RAMPS[mode]
	material.set_shader_parameter("overlay_field", tex)
	material.set_shader_parameter("overlay_low", ramp[0])
	material.set_shader_parameter("overlay_high", ramp[1])
	# Strong enough to read at a glance, weak enough that the ground underneath
	# is still the ground. An overlay that replaces the map stops being an
	# overlay and starts being a different screen.
	material.set_shader_parameter("overlay_strength", 0.62)


## Deposits, as translucent discs at their real radius.
##
## Not a heat field: a body of coal has an actual extent and an actual depth,
## and drawing it as a smear would misrepresent the one thing the survey is for.
## Brightness carries how much is left against what was there, so a worked-out
## seam visibly fades rather than vanishing the day it empties.
static func survey_mesh(republic: Node, terrain_height: Callable) -> ArrayMesh:
	var flat: PackedFloat32Array = republic.deposits()
	var stride := 8
	var count := flat.size() / stride
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var segments := 28
	var any := false
	for i in count:
		var o := i * stride
		var mineral := int(flat[o])
		var cx := flat[o + 1]
		var cz := flat[o + 2]
		var radius := flat[o + 3]
		var remaining := flat[o + 5]
		var initial := flat[o + 6]
		if radius <= 0.0:
			continue
		any = true
		var left: float = clampf(remaining / maxf(initial, 1.0), 0.0, 1.0)
		var tone: Color = mineral_colour(mineral)
		tone.a = lerpf(0.30, 0.80, left)
		var y: float = terrain_height.call(cx, cz) + 1.2
		var centre := Vector3(cx, y, cz)
		for k in segments:
			var a0 := TAU * float(k) / float(segments)
			var a1 := TAU * float(k + 1) / float(segments)
			var p0 := centre + Vector3(cos(a0) * radius, 0.0, sin(a0) * radius)
			var p1 := centre + Vector3(cos(a1) * radius, 0.0, sin(a1) * radius)
			for v in [centre, p1, p0]:
				st.set_color(tone)
				st.add_vertex(v)
	if not any:
		return null
	st.generate_normals()
	var mesh: ArrayMesh = st.commit()
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.cull_mode = BaseMaterial3D.CULL_DISABLED
	mesh.surface_set_material(0, mat)
	return mesh


## Keyed to `Mineral::ALL`: coal, iron ore, oil, gravel, groundwater.
static func mineral_colour(index: int) -> Color:
	match index:
		0: return Color(0.16, 0.16, 0.18)
		1: return Color(0.62, 0.33, 0.22)
		2: return Color(0.28, 0.20, 0.34)
		3: return Color(0.66, 0.62, 0.54)
		_: return Color(0.28, 0.52, 0.66)
