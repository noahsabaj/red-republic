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

const SEED := 1961
const EXTENT_M := 6000.0
const CLIMATE := 0  ## indexes ClimateId::ALL: plains, taiga, steppe, maritime
const SETTLERS := 120

@onready var republic: Node = $Republic
@onready var rig: Node3D = $CameraRig
@onready var terrain_node: MeshInstance3D = $Terrain
@onready var buildings_node: MultiMeshInstance3D = $Buildings
@onready var vehicles_node: MultiMeshInstance3D = $Vehicles
@onready var roads_node: MeshInstance3D = $Roads
@onready var status: Label = $HUD/Status

var _buildings_shown := -1
var _roads_shown := -1
var _shot_path := ""
var _shot_after := 90
var _frames := 0
var _start_speed := 0
var _bench_frames := 0
var _bench_times: PackedFloat64Array = PackedFloat64Array()


func _ready() -> void:
	_read_arguments()
	republic.found(SEED, EXTENT_M, CLIMATE, SETTLERS)
	_build_terrain()
	_build_instance_meshes()
	rig.frame_map(EXTENT_M, republic.centre_x(), republic.centre_y())
	_refresh_buildings()
	_refresh_roads()
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
	_refresh_vehicles()
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
	terrain_node.material_override = mat


func _build_instance_meshes() -> void:
	# A unit cube standing on the ground, scaled per instance to the real metric
	# footprint a BuildingDef authors. Placeholder geometry on purpose: the kit
	# of parts replaces it, and the transform buffer does not change when it does.
	var box := BoxMesh.new()
	box.size = Vector3.ONE
	var box_mat := StandardMaterial3D.new()
	box_mat.albedo_color = Color(0.72, 0.68, 0.60)
	box.material = box_mat

	var bm := MultiMesh.new()
	bm.transform_format = MultiMesh.TRANSFORM_3D
	bm.mesh = box
	buildings_node.multimesh = bm

	var van := BoxMesh.new()
	van.size = Vector3(6.0, 3.0, 2.5)
	var van_mat := StandardMaterial3D.new()
	van_mat.albedo_color = Color(0.55, 0.16, 0.13)
	van.material = van_mat

	var vm := MultiMesh.new()
	vm.transform_format = MultiMesh.TRANSFORM_3D
	vm.mesh = van
	vehicles_node.multimesh = vm


func _refresh_buildings() -> void:
	# Event-driven rather than per-frame: the transform buffer only changes when
	# something is commissioned or demolished, and uploading it every frame
	# would be paying the one cost this boundary is careful about for nothing.
	var count: int = republic.building_count()
	if count == _buildings_shown:
		return
	_buildings_shown = count
	var mm := buildings_node.multimesh
	mm.instance_count = count
	if count > 0:
		mm.buffer = republic.building_transforms()


func _refresh_vehicles() -> void:
	var flat: PackedFloat32Array = republic.vehicle_positions()
	var count := flat.size() / 3
	var mm := vehicles_node.multimesh
	if mm.instance_count != count:
		mm.instance_count = count
	for i in count:
		var at := Vector3(flat[i * 3], flat[i * 3 + 1] + 1.5, flat[i * 3 + 2])
		mm.set_instance_transform(i, Transform3D(Basis.IDENTITY, at))


func _refresh_roads() -> void:
	# Roads appear without being ordered -- traffic wears the ground, and a worn
	# corridor is promoted into the network as a dirt track -- so the segment
	# count is genuinely a thing that changes while nobody is building.
	var flat: PackedFloat32Array = republic.road_segments()
	var count := flat.size() / 6
	if count == _roads_shown:
		return
	_roads_shown = count
	var mesh := ImmediateMesh.new()
	if count > 0:
		var mat := StandardMaterial3D.new()
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.albedo_color = Color(0.20, 0.18, 0.16)
		mesh.surface_begin(Mesh.PRIMITIVE_LINES, mat)
		for i in count:
			var o := i * 6
			mesh.surface_add_vertex(Vector3(flat[o], flat[o + 1] + 0.6, flat[o + 2]))
			mesh.surface_add_vertex(Vector3(flat[o + 3], flat[o + 4] + 0.6, flat[o + 5]))
		mesh.surface_end()
	roads_node.mesh = mesh


func _refresh_status() -> void:
	var names := ["paused", "real time", "1 h/s", "2 h/s", "4 h/s", "8 h/s"]
	status.text = "%s   %s   pop %d (%d at work)   %d buildings   %d lorries   %.1f degC   %s roubles" % [
		republic.date_text(),
		names[republic.speed()],
		republic.population(),
		republic.employed(),
		republic.building_count(),
		republic.vehicle_count(),
		republic.temperature_c(),
		_thousands(republic.rubles()),
	]


func _thousands(value: float) -> String:
	var whole := int(abs(value))
	var out := ""
	while whole >= 1000:
		out = " %03d%s" % [whole % 1000, out]
		whole /= 1000
	out = "%d%s" % [whole, out]
	return ("-" + out) if value < 0.0 else out
