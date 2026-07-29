extends Node3D

## Wires the scene to the simulation.
##
## Everything this script does is one of three things: hand Godot a mesh built
## from simulation state, hand the camera the map's size, or read a handful of
## numbers for the status line. It holds no simulation of its own, and it must
## not start: if a value here would change how a republic turns out, it belongs
## in crates/sim.
##
## The heavy transfers are one-off or event-driven, which is what makes the
## marshalling boundary affordable. The terrain mesh is built once at load
## (30 ms, measured). Buildings are uploaded only when the count changes.
## Vehicles are the one per-frame read, and it is 0.9 microseconds.

const Looks := preload("res://looks.gd")
const Kit := preload("res://building_kit.gd")
const Art := preload("res://building_art.gd")
const Overlays := preload("res://ui/overlays.gd")

const SEED := 1961
const EXTENT_M := 6000.0
const CLIMATE := 0  ## indexes ClimateId::ALL: plains, taiga, steppe, maritime
## How many settlers arrive is the simulation's to say, not this file's. It was
## a constant here once and the two drifted, which is how a founding ended up
## with more jobs than people and a customs house nobody ever worked.

@onready var republic: Node = $Republic
@onready var rig: Node3D = $CameraRig
@onready var terrain_node: MeshInstance3D = $Terrain
@onready var buildings_node: MultiMeshInstance3D = $Buildings
@onready var vehicles_node: MultiMeshInstance3D = $Vehicles
@onready var newcomers_node: MultiMeshInstance3D = $Newcomers
@onready var roads_node: MeshInstance3D = $Roads
@onready var lines_node: MeshInstance3D = $Lines
@onready var ways_node: MeshInstance3D = $Ways
@onready var hud: CanvasLayer = $HUD
@onready var survey_node: MeshInstance3D = $Survey
@onready var frontier_node: MeshInstance3D = $Frontier

var _buildings_shown := -1
var _roads_shown := -1
var _lines_shown := -1
var _ways_shown := -1
var _shot_path := ""
var _shot_after := 90
var _frames := 0
var _start_speed := 0
var _bench_frames := 0
var _look: Looks.Look = null
var _view_distance := 0.0
var _kind_nodes: Array[MultiMeshInstance3D] = []
var _overlay := Overlays.Mode.NONE
var _start_overlay := ""
var _terrain_material: ShaderMaterial = null
var _overlay_dirty := true
var _overlay_day := -1
var _advance_days := 0
var _bench_times: PackedFloat64Array = PackedFloat64Array()


func _ready() -> void:
	_read_arguments()
	_look = Looks.current()
	_apply_look()
	republic.found(SEED, EXTENT_M, CLIMATE, republic.founding_settlers())
	_build_terrain()
	_build_instance_meshes()
	if _advance_days > 0:
		republic.advance_days(_advance_days)
	rig.frame_map(EXTENT_M, republic.centre_x(), republic.centre_y())
	if _view_distance > 0.0:
		rig.set_distance(_view_distance)
	match _start_overlay:
		"going": _overlay = Overlays.Mode.GOING
		"tracks": _overlay = Overlays.Mode.WEAR
		"survey": _overlay = Overlays.Mode.SURVEY
		"pollution": _overlay = Overlays.Mode.POLLUTION
	hud.set_resource_names(republic.resource_names())
	hud.set_contentment_names(republic.contentment_names())
	hud.set_utility_names(republic.utility_names())
	hud.set_way_names(republic.way_names())
	hud.set_hint(
		"0-5 speed  ·  space pause  ·  F none  G going  T tracks  R survey  P smoke  ·  "
		+ "WASD pan  ·  right-drag orbit  ·  wheel zoom"
	)
	_refresh_buildings()
	_refresh_roads()
	_refresh_lines()
	_refresh_ways()
	_build_frontier()
	# Founded paused. The first thing a posting should do is let you look at it.
	republic.set_speed(_start_speed)

	if OS.is_debug_build():
		# A headless run that errors prints; a headless run that quietly does
		# nothing also prints nothing. This is the line that tells the two
		# apart, and it is why it exists rather than trusting silence.
		var verts: int = terrain_node.mesh.surface_get_array_len(0)
		print("founded %s: %d verts, %d buildings, %d lorries, %d pop, %.0f m map" % [
			republic.date_text(), verts, republic.building_count(),
			republic.vehicle_count(), republic.population(), republic.map_extent(),
		])


func _process(_delta: float) -> void:
	_refresh_buildings()
	_refresh_roads()
	_refresh_lines()
	_refresh_ways()
	_refresh_vehicles()
	_refresh_newcomers()
	_refresh_overlay()
	_refresh_status()
	_maybe_bench(_delta)
	_maybe_capture()


## Time frames with the simulation genuinely running, then report and quit.
##
## Vsync is off in project.godot, and that is load-bearing: with it on every
## p50 comes back as exactly 16.67 ms whatever the load, so a scene that is
## drowning reports a healthy number. Measured on this machine -- it silently
## invalidated a whole probe run before anyone noticed.
##
## Real-time is the thesis, so "does it hold frame rate while the republic is
## actually being simulated" is not a nice-to-have measurement. It is the
## constraint the renderer exists under.
func _maybe_bench(delta: float) -> void:
	if _bench_frames <= 0:
		return
	_bench_times.append(delta * 1000.0)
	if _bench_times.size() < _bench_frames:
		return
	var sorted := _bench_times.duplicate()
	sorted.sort()
	# Drop the first tenth: pipeline compilation makes the opening frames
	# unrepresentative, and the first frame alone was 146 ms when measured.
	var warm := _bench_frames / 10
	var p50 := sorted[int(warm + (sorted.size() - warm) * 0.50)]
	var p95 := sorted[int(warm + (sorted.size() - warm) * 0.95)]
	print("bench speed %d: %d frames, p50 %.2f ms, p95 %.2f ms, %d buildings, %d lorries, %s" % [
		republic.speed(), _bench_frames, p50, p95,
		republic.building_count(), republic.vehicle_count(), republic.date_text(),
	])
	get_tree().quit()


## Capture the viewport and quit, when asked for on the command line:
##
##     godot --path godot -- --shot out.png [--after 90] [--speed 2]
##
## In-process rather than an OS screenshot, so it captures exactly what was
## rendered with no window-focus or monitor guesswork, and it works the same
## from a script as from a hand. A rendered frame is the only way to check the
## things numbers cannot say -- that the terrain is the right way up, that
## buildings are standing on it rather than buried in it, that the camera is
## pointed at the republic.
func _maybe_capture() -> void:
	if _shot_path == "":
		return
	_frames += 1
	if _frames < _shot_after:
		return
	await RenderingServer.frame_post_draw
	var image := get_viewport().get_texture().get_image()
	image.save_png(_shot_path)
	print("captured %s after %d frames" % [_shot_path, _frames])
	get_tree().quit()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		# 0 pauses; 1 is real-time -- one real second is one in-game second --
		# then a second buys 1, 2, 4 or 8 in-game hours.
		var speeds := {
			KEY_0: 0, KEY_1: 1, KEY_2: 2, KEY_3: 3, KEY_4: 4, KEY_5: 5,
		}
		if speeds.has(event.keycode):
			republic.set_speed(speeds[event.keycode])
		elif event.keycode == KEY_SPACE:
			republic.set_speed(0 if republic.speed() > 0 else 1)
		else:
			var modes := {
				KEY_F: Overlays.Mode.NONE,
				KEY_G: Overlays.Mode.GOING,
				KEY_T: Overlays.Mode.WEAR,
				KEY_R: Overlays.Mode.SURVEY,
				KEY_P: Overlays.Mode.POLLUTION,
			}
			if modes.has(event.keycode):
				_overlay = modes[event.keycode]
				_overlay_dirty = true


## Arguments after a bare `--` on the command line. Used for capture runs and
## for starting at a speed, so a check can be scripted without editing this file.
func _read_arguments() -> void:
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		match args[i]:
			"--shot":
				if i + 1 < args.size():
					_shot_path = args[i + 1]
			"--after":
				if i + 1 < args.size():
					_shot_after = int(args[i + 1])
			"--speed":
				if i + 1 < args.size():
					_start_speed = int(args[i + 1])
			"--bench":
				if i + 1 < args.size():
					_bench_frames = int(args[i + 1])
			"--dist":
				if i + 1 < args.size():
					_view_distance = float(args[i + 1])
			"--advance":
				if i + 1 < args.size():
					_advance_days = int(args[i + 1])
			"--overlay":
				if i + 1 < args.size():
					_start_overlay = args[i + 1]


## Sun, sky and air. Presentation only -- the weather the simulation models is a
## different thing entirely, and this does not read it.
func _apply_look() -> void:
	var sun: DirectionalLight3D = $Sun
	sun.light_color = _look.sun_colour
	sun.light_energy = _look.sun_energy
	sun.rotation_degrees = Vector3(-_look.sun_elevation, _look.sun_azimuth, 0.0)
	sun.shadow_enabled = true

	var sky_mat := ProceduralSkyMaterial.new()
	sky_mat.sky_top_color = _look.sky_top
	sky_mat.sky_horizon_color = _look.sky_horizon
	sky_mat.ground_bottom_color = _look.ground_horizon
	sky_mat.ground_horizon_color = _look.ground_horizon
	sky_mat.sun_angle_max = 12.0

	var sky := Sky.new()
	sky.sky_material = sky_mat

	var env := Environment.new()
	env.background_mode = Environment.BG_SKY
	env.sky = sky
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = _look.ambient_colour
	env.ambient_light_energy = _look.ambient_energy
	env.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	env.tonemap_exposure = _look.tonemap_exposure
	# Distance fog rather than volumetric: the job is to give a 6 km map depth,
	# not to render weather, and it costs nothing.
	env.fog_enabled = true
	env.fog_light_color = _look.fog_colour
	env.fog_density = _look.fog_density
	# The sky is already the right colour; letting fog repaint it flattens the
	# horizon into a single wash and takes the depth cue with it.
	env.fog_sky_affect = 0.0
	env.fog_aerial_perspective = 0.35
	env.ssao_enabled = true
	env.ssao_intensity = 1.2

	$Sky.environment = env


func _build_terrain() -> void:
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, republic.terrain_surface())
	terrain_node.mesh = mesh

	# Vertex colour carries the surface kind as a one-hot channel -- red grass,
	# green forest, blue rock, black water -- and terrain.gdshader decides what
	# each looks like. Keeping the tones there rather than in Rust is what lets
	# the art direction change without recompiling the simulation's renderer.
	var mat := ShaderMaterial.new()
	mat.shader = load("res://terrain.gdshader")
	mat.set_shader_parameter("grass_colour", _look.grass)
	mat.set_shader_parameter("forest_colour", _look.forest)
	mat.set_shader_parameter("rock_colour", _look.rock)
	mat.set_shader_parameter("water_colour", _look.water)
	mat.set_shader_parameter("contour_strength", _look.contour_strength)
	_terrain_material = mat
	terrain_node.material_override = mat


## One mesh per building kind, assembled from the kit, each with its own
## MultiMesh. Done once at load: a kind's mesh never changes, only how many of
## them are standing.
func _build_instance_meshes() -> void:
	var names := PackedStringArray()
	for k in republic.building_kind_count():
		names.append(republic.building_kind_name(k))
	# An unauthored building would otherwise render as a default box nobody
	# chose. This is the guard that makes adding a building to the Rust table
	# fail loudly here instead.
	Art.check(names)

	var parts := Kit.components()
	if OS.is_debug_build():
		var found := parts.keys()
		found.sort()
		print("kit components: %s" % ", ".join(found))
	var art := Art.table()
	for k in republic.building_kind_count():
		var size: Vector2 = republic.building_kind_size(k)
		var mesh := Kit.assemble(parts, art[names[k]], size.x, size.y, _look.tones)
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.mesh = mesh
		var node := MultiMeshInstance3D.new()
		node.multimesh = mm
		node.name = "Kind%d" % k
		buildings_node.add_child(node)
		_kind_nodes.append(node)

	var van := BoxMesh.new()
	van.size = Vector3(6.0, 3.0, 2.5)
	var van_mat := StandardMaterial3D.new()
	van_mat.albedo_color = _look.vehicle
	van.material = van_mat

	var vm := MultiMesh.new()
	vm.transform_format = MultiMesh.TRANSFORM_3D
	vm.mesh = van
	vehicles_node.multimesh = vm

	# Settlers standing at a frontier post. A marker rather than figures: what
	# matters is that they are somewhere on the map that a coach has to reach,
	# and a republic with no road out to that post can see the problem it has.
	var marker := CylinderMesh.new()
	# A marker post rather than a pile of people: tall and thin, so it reads
	# as a vertical mark at map zoom where a group-sized object would be a few
	# pixels indistinguishable from a shed. Verified by looking -- at physical
	# size it vanished into the terrain at the zoom a player actually watches
	# their whole republic from.
	marker.top_radius = 5.0
	marker.bottom_radius = 14.0
	marker.height = 70.0
	var marker_mat := StandardMaterial3D.new()
	marker_mat.albedo_color = Color(0.86, 0.62, 0.34)
	marker_mat.emission_enabled = true
	marker_mat.emission = Color(0.5, 0.28, 0.1)
	marker_mat.emission_energy_multiplier = 0.4
	marker.material = marker_mat

	var nm := MultiMesh.new()
	nm.transform_format = MultiMesh.TRANSFORM_3D
	nm.mesh = marker
	newcomers_node.multimesh = nm


func _refresh_buildings() -> void:
	# Event-driven rather than per-frame: a kind's transform buffer only changes
	# when something of that kind is commissioned or demolished, and uploading
	# it every frame would be paying the one cost this boundary is careful about
	# for nothing.
	var total: int = republic.building_count()
	if total == _buildings_shown:
		return
	_buildings_shown = total
	for k in _kind_nodes.size():
		var count: int = republic.building_count_of_kind(k)
		var mm: MultiMesh = _kind_nodes[k].multimesh
		if mm.instance_count != count:
			mm.instance_count = count
		if count > 0:
			mm.buffer = republic.building_transforms_of_kind(k)


func _refresh_vehicles() -> void:
	var flat: PackedFloat32Array = republic.vehicle_positions()
	var count := flat.size() / 3
	var mm := vehicles_node.multimesh
	if mm.instance_count != count:
		mm.instance_count = count
	for i in count:
		var at := Vector3(flat[i * 3], flat[i * 3 + 1] + 1.5, flat[i * 3 + 2])
		mm.set_instance_transform(i, Transform3D(Basis.IDENTITY, at))


## Settlers standing at the frontier, waiting for a coach.
##
## They have to be on the map. An immigrant who appeared inside an apartment
## block would be exactly the click-a-button-and-it-happens shape this build
## refuses -- and a group standing at a post with no road to it is a decision
## the player can act on, but only if they can see it.
##
## The marker grows with the size of the group, so a crowd reads as a crowd.
func _refresh_newcomers() -> void:
	var flat: PackedFloat32Array = republic.newcomers()
	var stride := 4
	var count := flat.size() / stride
	var mm := newcomers_node.multimesh
	if mm.instance_count != count:
		mm.instance_count = count
	for i in count:
		var x := flat[i * stride]
		var z := flat[i * stride + 1]
		var heads := flat[i * stride + 2]
		var scale := clampf(0.6 + heads / 40.0, 0.6, 2.0)
		var at := Vector3(x, republic.ground_height(x, z) + 35.0 * scale, z)
		mm.set_instance_transform(
			i, Transform3D(Basis.IDENTITY.scaled(Vector3.ONE * scale), at)
		)


## The frontier, drawn once: a coloured band around the whole perimeter with a
## marker at each post.
##
## Built at load and never rebuilt, because a frontier does not move. The two
## blocs hold different stretches and the colours are the only thing that says
## which way is west -- which decides what currency this republic can earn, so
## it is not decoration.
func _build_frontier() -> void:
	var line: PackedFloat32Array = republic.frontier_line(240)
	var stride := 4
	var count := line.size() / stride
	if count < 2:
		return
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	# Inward, so the band lies on the republic's own ground rather than off the
	# edge of the mesh where there is nothing to draw on.
	var mid := EXTENT_M * 0.5
	for i in count - 1:
		var a := Vector3(line[i * stride], 0.0, line[i * stride + 1])
		var b := Vector3(line[(i + 1) * stride], 0.0, line[(i + 1) * stride + 1])
		var bloc := int(line[i * stride + 2])
		var tone: Color = _look.bloc_east if bloc == 0 else _look.bloc_west
		for p in [a, b]:
			p.y = republic.ground_height(p.x, p.z) + 1.0
		a.y = republic.ground_height(a.x, a.z) + 1.0
		b.y = republic.ground_height(b.x, b.z) + 1.0
		var inward_a := (Vector3(mid, a.y, mid) - a).normalized() * 26.0
		var inward_b := (Vector3(mid, b.y, mid) - b).normalized() * 26.0
		var p0 := a
		var p1 := b
		var p2 := b + inward_b
		var p3 := a + inward_a
		for v in [p0, p3, p2, p0, p2, p1]:
			st.set_color(tone)
			st.add_vertex(v)

	# A post at each crossing, standing proud so it reads from altitude.
	var posts: PackedFloat32Array = republic.crossings()
	for i in posts.size() / 4:
		var px := posts[i * 4]
		var pz := posts[i * 4 + 1]
		var tone: Color = _look.bloc_east if int(posts[i * 4 + 2]) == 0 else _look.bloc_west
		var base := Vector3(px, republic.ground_height(px, pz), pz)
		_pillar(st, base, 9.0, 26.0, tone)

	st.generate_normals()
	var mesh: ArrayMesh = st.commit()
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.cull_mode = BaseMaterial3D.CULL_DISABLED
	mesh.surface_set_material(0, mat)
	frontier_node.mesh = mesh


func _pillar(st: SurfaceTool, base: Vector3, half: float, height: float, tone: Color) -> void:
	var corners := [
		Vector3(-half, 0.0, -half), Vector3(half, 0.0, -half),
		Vector3(half, 0.0, half), Vector3(-half, 0.0, half),
	]
	for i in 4:
		var a: Vector3 = base + corners[i]
		var b: Vector3 = base + corners[(i + 1) % 4]
		var at := a + Vector3(0.0, height, 0.0)
		var bt := b + Vector3(0.0, height, 0.0)
		for v in [a, b, bt, a, bt, at]:
			st.set_color(tone)
			st.add_vertex(v)


func _refresh_roads() -> void:
	# Roads appear without being ordered -- traffic wears the ground, and a worn
	# corridor is promoted into the network as a dirt track -- so the segment
	# count genuinely changes while nobody is building.
	#
	# Drawn as ribbons rather than lines. A hairline reads as debug geometry,
	# and it also cannot show a grade: a dirt track and tarmac differ in width
	# and colour here because they differ in what a lorry can do on them, and
	# that has to be visible without opening a panel.
	var flat: PackedFloat32Array = republic.road_segments()
	var stride := 7
	var count := flat.size() / stride
	if count == _roads_shown:
		return
	_roads_shown = count
	var mesh := ImmediateMesh.new()
	if count > 0:
		var mat := StandardMaterial3D.new()
		mat.vertex_color_use_as_albedo = true
		mat.roughness = 0.9
		mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, mat)
		for i in count:
			var o := i * stride
			var a := Vector3(flat[o], flat[o + 1], flat[o + 2])
			var b := Vector3(flat[o + 3], flat[o + 4], flat[o + 5])
			var kph := flat[o + 6]
			var along := b - a
			along.y = 0.0
			if along.length() < 0.01:
				continue
			# Dirt 25, gravel 45, paved 60 km/h. Width and tone follow from the
			# limit, so a grade added later needs nothing here.
			var t := clampf((kph - 25.0) / 35.0, 0.0, 1.0)
			var half := lerpf(2.6, 4.4, t)
			var tone := _look.road_dirt.lerp(_look.road_paved, t)
			var side := along.normalized().cross(Vector3.UP).normalized() * half
			# Lifted clear of the ground so it does not z-fight the terrain.
			var lift := Vector3(0.0, 0.35, 0.0)
			var p0 := a - side + lift
			var p1 := a + side + lift
			var p2 := b + side + lift
			var p3 := b - side + lift
			# Wound so the ribbon faces UP. Getting this backwards is the second
			# time in this scene that correct-looking geometry rendered as
			# nothing at all -- the terrain did it first. Godot culls back
			# faces, and a flat quad seen from the wrong side is invisible
			# rather than wrong-looking, which is what makes it so easy to miss.
			for v in [p0, p3, p2, p0, p2, p1]:
				mesh.surface_set_color(tone)
				mesh.surface_add_vertex(v)
		mesh.surface_end()
	roads_node.mesh = mesh


## Power lines and heat mains, drawn as thin ribbons above the ground.
##
## They have to be on the map. A plant lights only what it is strung to and a
## boiler warms only what a main runs past, so "why is this block cold" is a
## question about a line the player either drew or did not -- which is
## unanswerable if the lines are invisible.
##
## Event-driven like the roads: a span changes only when one is ordered or
## energised, which is rare.
func _refresh_lines() -> void:
	var built: PackedFloat32Array = republic.utility_lines()
	var sites: PackedFloat32Array = republic.utility_sites()
	@warning_ignore("integer_division")
	var count: int = built.size() / 5 + sites.size() / 6
	if count == _lines_shown:
		return
	_lines_shown = count
	var mesh := ImmediateMesh.new()
	if count > 0:
		var mat := StandardMaterial3D.new()
		mat.vertex_color_use_as_albedo = true
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.cull_mode = BaseMaterial3D.CULL_DISABLED
		mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, mat)
		@warning_ignore("integer_division")
		var spans: int = built.size() / 5
		for i in spans:
			var o := i * 5
			var tone: Color = _utility_tone(int(built[o + 4]))
			_span(mesh, built[o], built[o + 1], built[o + 2], built[o + 3], tone, 1.8, 9.0)
		@warning_ignore("integer_division")
		var pending: int = sites.size() / 6
		for i in pending:
			var o := i * 6
			# A site is thinner and paler, and fills in as it is strung -- the
			# same rule a road site answers to: a half-built thing must look
			# half-built rather than leave the player wondering why the lights
			# are still out.
			var tone: Color = _utility_tone(int(sites[o + 5]))
			tone.a = lerpf(0.25, 0.8, sites[o + 4])
			_span(mesh, sites[o], sites[o + 1], sites[o + 2], sites[o + 3], tone, 1.0, 5.0)
		mesh.surface_end()
	lines_node.mesh = mesh


## The rails and the rivers, drawn from the same span machinery as the grid.
##
## Roads have their own mesh already; these are the three ways that did not
## exist before. Water is in here beside the built ones deliberately -- the one
## network nobody builds is invisible on the ground at this camera height, and a
## republic that cannot see its river will not think to put a port on it.
##
## Air is deliberately **not** drawn. It is a fully connected graph between
## aerodromes, so drawing it means a spray of straight lines across the whole
## map that says nothing a list of aerodromes does not; the HUD carries its
## length instead.
func _refresh_ways() -> void:
	var rail: PackedFloat32Array = republic.ways(WAY_RAIL)
	var water: PackedFloat32Array = republic.ways(WAY_WATER)
	@warning_ignore("integer_division")
	var count: int = rail.size() / 5 + water.size() / 5
	if count == _ways_shown:
		return
	_ways_shown = count
	var mesh := ImmediateMesh.new()
	if count > 0:
		var mat := StandardMaterial3D.new()
		mat.vertex_color_use_as_albedo = true
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.cull_mode = BaseMaterial3D.CULL_DISABLED
		mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, mat)
		# Lift matters more than it looks. Water was first drawn 0.4 m off the
		# ground, which reads as a hatched mess at map height rather than as a
		# river: a flat ribbon that close to the terrain z-fights with it, and
		# the result looks like a rendering fault rather than like a feature.
		# Caught by looking at a frame -- nothing that counts spans can see it.
		for pair in [[rail, RAIL_TONE, 6.0, 2.5], [water, WATER_TONE, 30.0, 1.5]]:
			var spans: PackedFloat32Array = pair[0]
			@warning_ignore("integer_division")
			var n: int = spans.size() / 5
			for i in n:
				var o := i * 5
				_span(mesh, spans[o], spans[o + 1], spans[o + 2], spans[o + 3],
					pair[1], pair[2], pair[3])
		mesh.surface_end()
	ways_node.mesh = mesh


## Indices into `Medium::ALL`, which is the order the shell hands them over in.
const WAY_ROAD := 0
const WAY_RAIL := 1
const WAY_WATER := 2
const WAY_AIR := 3

## Track reads as dark steel; water as a broad pale ribbon lying almost flat on
## the ground, because it is the surface rather than something built on it.
const RAIL_TONE := Color(0.32, 0.30, 0.28)
const WATER_TONE := Color(0.35, 0.47, 0.60, 0.85)


## In the order `Utility::ALL` declares: power, heat, conveyor, pipeline.
##
## Four distinct hues rather than a ramp, because these are categories and not a
## quantity -- a player has to be able to tell a belt from a pipe at a glance,
## and two shades of the same colour would read as "more of the same thing".
const UTILITY_TONES := [
	Color(0.86, 0.80, 0.42),  # power: overhead line
	Color(0.78, 0.42, 0.32),  # heat: hot main
	Color(0.55, 0.60, 0.66),  # conveyor: steel belt
	Color(0.44, 0.56, 0.44),  # pipeline: painted pipe
]


func _utility_tone(kind: int) -> Color:
	return UTILITY_TONES[kind] if kind >= 0 and kind < UTILITY_TONES.size() else Color.WHITE


## One ribbon between two ground positions, lifted clear and wound to face up.
##
## Wound so the ribbon faces UP. Getting this backwards is the third time in
## this scene that correct-looking geometry would render as nothing at all --
## Godot culls back faces, and a flat quad seen from the wrong side is invisible
## rather than wrong-looking.
func _span(
	mesh: ImmediateMesh, ax: float, az: float, bx: float, bz: float,
	tone: Color, half: float, lift: float
) -> void:
	var a := Vector3(ax, republic.ground_height(ax, az) + lift, az)
	var b := Vector3(bx, republic.ground_height(bx, bz) + lift, bz)
	var along := b - a
	along.y = 0.0
	if along.length() < 0.01:
		return
	var side := along.normalized().cross(Vector3.UP).normalized() * half
	var p0 := a - side
	var p1 := a + side
	var p2 := b + side
	var p3 := b - side
	for v in [p0, p3, p2, p0, p2, p1]:
		mesh.surface_set_color(tone)
		mesh.surface_add_vertex(v)


const SPEED_NAMES := ["paused", "real time", "1 h/s", "2 h/s", "4 h/s", "8 h/s"]


func _refresh_status() -> void:
	hud.refresh(republic, _overlay, SPEED_NAMES)


## Overlays are rebuilt on the day boundary rather than per frame.
##
## Going and wear are ground state and the ground changes daily, so a per-frame
## rebuild would repaint an identical texture sixty times a second. This is the
## same event-driven discipline the building buffer uses, for the same reason:
## the marshalling boundary is affordable precisely because nothing bulk crosses
## it on a schedule.
func _refresh_overlay() -> void:
	if _terrain_material == null:
		return
	var day: int = int(republic.date_text().replace("-", ""))
	if not _overlay_dirty and day == _overlay_day:
		return
	_overlay_dirty = false
	_overlay_day = day
	Overlays.apply(_terrain_material, republic, _overlay, EXTENT_M)
	if _overlay == Overlays.Mode.SURVEY:
		survey_node.mesh = Overlays.survey_mesh(republic, _ground_height)
	else:
		survey_node.mesh = null


func _ground_height(x: float, z: float) -> float:
	# The terrain mesh is the ground, so a disc drawn on it has to sit on the
	# same surface rather than on a plane at zero.
	return republic.ground_height(x, z)
